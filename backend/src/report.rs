//! 每日日报:聚合生成 + 查询
//!
//! 口径:
//! - "一天" = 北京时间(UTC+8,无夏令时)自然日,数据窗口按此切分;
//! - 每天北京时间 09:00 起,定时任务生成"前一天"的日报;服务启动时立即补一次(停机跨天兜底);
//! - 生成时经 `notify::insert_report_notification` 落一条全员可见的日报通知;
//! - GET /api/reports/today 只读最近一份日报,不做任何写操作(命令查询分离)。
use crate::api::Error;
use crate::auth::Auth;
use crate::notify;
use crate::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

/// 业务时区:北京时间(UTC+8,无夏令时)。
const fn cn_tz() -> chrono::FixedOffset {
    match chrono::FixedOffset::east_opt(8 * 3600) {
        Some(tz) => tz,
        None => panic!("+08:00 是合法固定偏移"),
    }
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ReportContent {
    pub date: String,
    pub devices_total: i64,
    pub devices_online: i64,
    pub lamp_on: i64,
    pub alarms_today: i64,
    pub alarms_unhandled: i64,
    pub avg_lux: f64,
    pub reports_lux: i64,
    pub cmd_manual: i64,
    pub cmd_auto: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ReportOut {
    pub report_date: String,
    pub content: ReportContent,
    pub created_at: DateTime<Utc>,
}

/// 聚合生成 `date`(北京时间自然日)的日报:写入 `daily_report` + 落一条日报通知
async fn generate_report(db: &PgPool, date: NaiveDate) -> anyhow::Result<()> {
    // 北京时间 00:00 转为 UTC 作为当天数据窗口起点
    let day_start = date
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 恒为合法时刻")
        .and_local_timezone(cn_tz())
        .single()
        .expect("固定偏移时区无歧义")
        .with_timezone(&Utc);
    let day_end = day_start + Duration::hours(24);

    // 四条互不依赖的聚合查询并发执行(与 dashboard/lux_stats 的约定一致)
    let dev_fut = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'online'), \
                COUNT(*) FILTER (WHERE lamp = 'on') FROM device",
    )
    .fetch_one(db);
    let alarm_fut = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE resolved_at IS NULL) \
         FROM alarm WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);
    let lux_fut = sqlx::query_as::<_, (i64, Option<f64>)>(
        "SELECT COUNT(*), AVG(lux)::float8 FROM lux_record \
         WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);
    let cmd_fut = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*) FILTER (WHERE source = 'manual'), \
                COUNT(*) FILTER (WHERE source = 'auto') \
         FROM command_record WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);

    let (
        (devices_total, devices_online, lamp_on),
        (alarms_today, alarms_unhandled),
        (reports_lux, avg_lux),
        (cmd_manual, cmd_auto),
    ) = tokio::try_join!(dev_fut, alarm_fut, lux_fut, cmd_fut)?;

    let content = ReportContent {
        date: date.format("%Y-%m-%d").to_string(),
        devices_total,
        devices_online,
        lamp_on,
        alarms_today,
        alarms_unhandled,
        avg_lux: avg_lux.unwrap_or(0.0).round(),
        reports_lux,
        cmd_manual,
        cmd_auto,
    };

    // 序列化失败必须显式传播,不能静默写 null(后置条件:content 恒为合法日报 JSON)
    let json = serde_json::to_value(&content)
        .map_err(|e| anyhow::anyhow!("日报内容序列化失败: {e}"))?;
    sqlx::query(
        "INSERT INTO daily_report (report_date, content) VALUES ($1, $2) \
         ON CONFLICT (report_date) DO UPDATE SET content = EXCLUDED.content",
    )
    .bind(date)
    .bind(&json)
    .execute(db)
    .await?;

    notify::insert_report_notification(
        db,
        "每日日报",
        &format!(
            "{} 设备 {} 台(在线 {}),当日告警 {} 条,平均光照 {:.0} lux,手动指令 {} 次",
            content.date,
            content.devices_total,
            content.devices_online,
            content.alarms_today,
            content.avg_lux,
            content.cmd_manual,
        ),
    )
    .await?;
    Ok(())
}

/// GET /api/reports/today —— 最近一份日报(纯读;生成由定时任务/启动补做)
#[utoipa::path(
    get,
    path = "/api/reports/today",
    responses(
        (status = 200, description = "最近一份日报", body = ReportOut),
        (status = 404, description = "尚无日报(每日 09:00 后生成前一日)")
    ),
    security(("bearer_auth" = []))
)]
async fn report_today(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<ReportOut>, Error> {
    auth.require(&s, "luminance:history").await?;
    let (report_date, content_json, created_at) =
        sqlx::query_as::<_, (NaiveDate, serde_json::Value, DateTime<Utc>)>(
            "SELECT report_date, content, created_at FROM daily_report \
             ORDER BY report_date DESC LIMIT 1",
        )
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| {
            Error::NotFound("暂无日报,每天 09:00(北京时间)后生成前一日数据".into())
        })?;
    // daily_report 只由本服务写入,解析失败 = 数据损坏/结构演进,属 500 而非客户端 400
    let content: ReportContent = serde_json::from_value(content_json)
        .map_err(|e| Error::Internal(format!("日报内容解析失败: {e}")))?;
    Ok(Json(ReportOut {
        report_date: report_date.format("%Y-%m-%d").to_string(),
        content,
        created_at,
    }))
}

// ---------------- 定时任务:每天北京时间 09:00 生成前一日日报 ----------------

pub async fn run(db: PgPool) {
    // 启动立即补一次(服务停机跨过 09:00 的场景),之后每 30 分钟检查
    generate_if_missing(&db).await;
    let mut interval = tokio::time::interval(std::time::Duration::from_mins(30));
    loop {
        interval.tick().await;
        generate_if_missing(&db).await;
    }
}

/// 北京时间 09:00 后若"前一天"日报缺失则生成(先查后写,幂等)
async fn generate_if_missing(db: &PgPool) {
    let now = Utc::now().with_timezone(&cn_tz());
    if now.time().hour() < 9 {
        return;
    }
    // 日报口径:每日 09:00 后总结"前一天"全天数据
    let date = now.date_naive() - Duration::days(1);
    let exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM daily_report WHERE report_date = $1)",
    )
    .bind(date)
    .fetch_one(db)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("日报存在性检查失败(按不存在处理): {e}");
        false
    });
    if exists {
        return;
    }
    match generate_report(db, date).await {
        Ok(()) => tracing::info!("日报已生成: {date}"),
        Err(e) => tracing::warn!("日报生成失败: {e:#}"),
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/reports/today", get(report_today))
        .with_state(state)
}
