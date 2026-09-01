//! 每日日报:聚合生成 + 查询
//!
//! 口径([`ReportContent`] 字段分两类):
//! - **当日窗口**:按北京时间(UTC+8)自然日切分,`created_at ∈ [当天 00:00,次日 00:00)`,
//!   是这一整天真实发生的量——当日活跃设备率、告警、光照、指令统计;
//! - **生成时刻快照**:`device` 表只存当前状态、没有历史表,只能反映生成那一瞬的值——
//!   设备总数、在线数、亮灯数、亮灯率(`_now` 后缀明示)。日报在 09:00 生成,故这些
//!   是 09:00 那一瞬的快照,既不是全天平均,也不是全天某个时点。
//!
//! 其余口径:
//! - 定时每天北京时间 09:00 生成"前一天"日报;服务启动时立即补一次(停机跨天兜底);
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

/// 百分比(保留 1 位小数);分母为 0 返回 None,表示"无从计算"而非 0%。
#[allow(clippy::cast_precision_loss)] // 计数值远小于 2^52,转换无精度损失
fn pct1(part: i64, total: i64) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(((part as f64 / total as f64) * 100.0 * 10.0).round() / 10.0)
    }
}

/// Option 显式化为可读字符串:None 显示 "--"(无从计算/旧数据缺字段),绝不冒充 0。
fn opt_text<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "--".to_string(), |x| x.to_string())
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ReportContent {
    /// 日报对应的北京时间自然日 (YYYY-MM-DD)
    pub date: String,

    // ── 设备:生成时刻快照(device 表无历史,只能反映生成瞬间) ──
    /// 设备总数(生成时刻快照)
    pub devices_total: i64,
    /// 在线设备数(生成时刻快照,取生成那一瞬的 `device.status`)
    pub devices_online: i64,
    /// 亮灯设备数(生成时刻快照,取生成那一瞬的 `device.lamp`)
    pub lamp_on: i64,
    /// 亮灯率 %(生成时刻快照,= `lamp_on` / `devices_total`;无设备时 null)
    #[serde(default)]
    pub lamp_on_rate_now: Option<f64>,

    // ── 设备:当日窗口 ──
    /// 当日有光照上报的不同设备数(当日活跃设备)
    #[serde(default)]
    pub devices_active: Option<i64>,
    /// 当日在线率 %(当日活跃设备 / 设备总数;无设备时 null)
    #[serde(default)]
    pub online_rate: Option<f64>,

    // ── 告警:当日窗口 ──
    /// 当日告警总数
    pub alarms_today: i64,
    /// 当日未处理告警数(`resolved_at IS NULL`)
    pub alarms_unhandled: i64,
    /// 当日离线类告警数(type = offline)
    #[serde(default)]
    pub alarm_offline: Option<i64>,

    // ── 光照:当日窗口 ──
    /// 当日平均光照(lux,已四舍五入)
    pub avg_lux: f64,
    /// 当日光照最高值(lux;无记录时 null)
    #[serde(default)]
    pub lux_max: Option<i64>,
    /// 当日光照最低值(lux;无记录时 null)
    #[serde(default)]
    pub lux_min: Option<i64>,
    /// 当日光照记录条数
    pub reports_lux: i64,

    // ── 指令:当日窗口 ──
    /// 当日手动下发指令数(source = manual;目前 source 恒为 manual)
    pub cmd_manual: i64,
    /// 当日"开灯"指令数(action = on)
    #[serde(default)]
    pub cmd_on: Option<i64>,
    /// 当日"关灯"指令数(action = off)
    #[serde(default)]
    pub cmd_off: Option<i64>,
    /// 当日"恢复自动"指令数(action = auto)
    #[serde(default)]
    pub cmd_restore_auto: Option<i64>,
    /// 当日指令下发失败数(status = failed)
    #[serde(default)]
    pub cmd_failed: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ReportOut {
    pub report_date: String,
    pub content: ReportContent,
    pub created_at: DateTime<Utc>,
}

// 各聚合查询的具名行结构:SQL 用 `AS 别名` 与字段名对齐,替代易错的位置解构。

/// 设备表快照(无时间窗,反映生成瞬间的当前状态)。
#[derive(sqlx::FromRow)]
struct DeviceSnapshot {
    total: i64,
    online: i64,
    lamp_on: i64,
}

/// 当日活跃设备数(有光照上报的不同 `device_id` 数)。
#[derive(sqlx::FromRow)]
struct ActiveDeviceCount {
    active: i64,
}

/// 当日告警统计。
#[derive(sqlx::FromRow)]
struct AlarmStats {
    total: i64,
    unhandled: i64,
    offline: i64,
}

/// 当日光照统计;无记录时 avg/max/min 为 None(去掉 COALESCE 以区分"无数据"与真实 0)。
#[derive(sqlx::FromRow)]
struct LuxStats {
    count: i64,
    avg_lux: Option<f64>,
    lux_max: Option<i64>,
    lux_min: Option<i64>,
}

/// 当日指令统计。
#[derive(sqlx::FromRow)]
struct CmdStats {
    manual: i64,
    on_count: i64,
    off_count: i64,
    restore_auto: i64,
    failed: i64,
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

    // 五条互不依赖的聚合查询并发执行(与 dashboard/lux_stats 的约定一致)。
    // 设备快照无时间窗,是唯一"生成时刻快照";其余四条均落在 [day_start, day_end) 当日窗口。
    let dev_fut = sqlx::query_as::<_, DeviceSnapshot>(
        "SELECT COUNT(*) AS total, \
                COUNT(*) FILTER (WHERE status = 'online') AS online, \
                COUNT(*) FILTER (WHERE lamp = 'on') AS lamp_on \
         FROM device",
    )
    .fetch_one(db);
    let active_fut = sqlx::query_as::<_, ActiveDeviceCount>(
        "SELECT COUNT(DISTINCT device_id) AS active \
         FROM lux_record WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);
    let alarm_fut = sqlx::query_as::<_, AlarmStats>(
        "SELECT COUNT(*) AS total, \
                COUNT(*) FILTER (WHERE resolved_at IS NULL) AS unhandled, \
                COUNT(*) FILTER (WHERE type = 'offline') AS offline \
         FROM alarm WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);
    let lux_fut = sqlx::query_as::<_, LuxStats>(
        "SELECT COUNT(*) AS count, \
                AVG(lux)::float8 AS avg_lux, \
                MAX(lux)::bigint AS lux_max, \
                MIN(lux)::bigint AS lux_min \
         FROM lux_record WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);
    let cmd_fut = sqlx::query_as::<_, CmdStats>(
        "SELECT COUNT(*) FILTER (WHERE source = 'manual') AS manual, \
                COUNT(*) FILTER (WHERE action = 'on') AS on_count, \
                COUNT(*) FILTER (WHERE action = 'off') AS off_count, \
                COUNT(*) FILTER (WHERE action = 'auto') AS restore_auto, \
                COUNT(*) FILTER (WHERE status = 'failed') AS failed \
         FROM command_record WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db);

    let (dev_snap, active_cnt, alarm_stat, lux_stat, cmd_stat) =
        tokio::try_join!(dev_fut, active_fut, alarm_fut, lux_fut, cmd_fut)?;

    // 当日在线率 = 当日活跃设备 / 设备总数;亮灯率 = 生成时刻快照。
    let online_rate = pct1(active_cnt.active, dev_snap.total);
    let lamp_on_rate_now = pct1(dev_snap.lamp_on, dev_snap.total);

    let content = ReportContent {
        date: date.format("%Y-%m-%d").to_string(),
        devices_total: dev_snap.total,
        devices_online: dev_snap.online,
        lamp_on: dev_snap.lamp_on,
        lamp_on_rate_now,
        devices_active: Some(active_cnt.active),
        online_rate,
        alarms_today: alarm_stat.total,
        alarms_unhandled: alarm_stat.unhandled,
        alarm_offline: Some(alarm_stat.offline),
        avg_lux: lux_stat.avg_lux.unwrap_or(0.0).round(),
        lux_max: lux_stat.lux_max,
        lux_min: lux_stat.lux_min,
        reports_lux: lux_stat.count,
        cmd_manual: cmd_stat.manual,
        cmd_on: Some(cmd_stat.on_count),
        cmd_off: Some(cmd_stat.off_count),
        cmd_restore_auto: Some(cmd_stat.restore_auto),
        cmd_failed: Some(cmd_stat.failed),
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

    notify_report(db, &content).await?;
    Ok(())
}

/// 生成日报后落一条全员可见的通知;`Option` 为 None 时显示 "--"(旧的 "没有数据"
/// 语义被前端用 null 判断,绝不能打印 0 冒充真实数据)。
async fn notify_report(
    db: &PgPool,
    content: &ReportContent,
) -> anyhow::Result<()> {
    notify::insert_report_notification(
        db,
        "每日日报",
        &format!(
            "{} 设备 {} 台(当前在线 {} · 当日在线率 {}%),\
             告警 {} 条(离线 {}),平均光照 {:.0} lux,手动指令 {} 次(失败 {})",
            content.date,
            content.devices_total,
            content.devices_online,
            opt_text(content.online_rate),
            content.alarms_today,
            opt_text(content.alarm_offline),
            content.avg_lux,
            content.cmd_manual,
            opt_text(content.cmd_failed),
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
    let mut interval =
        tokio::time::interval(std::time::Duration::from_mins(30));
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

#[cfg(test)]
mod tests {
    use super::{pct1, ReportContent};

    /// KAT:2026-08-31 真实入库的历史日报(字段扩充前的原样内容)。
    /// 含已删除的死字段 `cmd_auto`,且缺全部新增字段。
    const LEGACY_REPORT_JSON: &str = r#"{
        "date": "2026-08-31", "avg_lux": 8904.0, "lamp_on": 0,
        "cmd_auto": 0, "cmd_manual": 0, "reports_lux": 608,
        "alarms_today": 9, "devices_total": 3, "devices_online": 0,
        "alarms_unhandled": 3
    }"#;

    fn approx(got: Option<f64>, want: f64) -> bool {
        got.is_some_and(|x| (x - want).abs() < 1e-9)
    }

    /// 历史日报缺新字段时必须解析成 None,**不得退化成 Some(0)**——
    /// 前端按 null 显示 "--";若变成 0,"当时没这项数据"会被渲染成
    /// "实测为 0"(本次修复的核心回归点)。
    #[test]
    fn legacy_json_missing_fields_become_none() {
        let c: ReportContent =
            serde_json::from_str(LEGACY_REPORT_JSON).unwrap();
        // 旧字段照常解析;已删除的 cmd_auto 作为未知字段被忽略,不报错
        assert_eq!(c.devices_total, 3);
        assert_eq!(c.reports_lux, 608);
        assert_eq!(c.alarms_today, 9);
        // 新字段一律 None
        assert!(c.online_rate.is_none());
        assert!(c.lamp_on_rate_now.is_none());
        assert!(c.devices_active.is_none());
        assert!(c.alarm_offline.is_none());
        assert!(c.lux_max.is_none());
        assert!(c.lux_min.is_none());
        assert!(c.cmd_on.is_none());
        assert!(c.cmd_off.is_none());
        assert!(c.cmd_restore_auto.is_none());
        assert!(c.cmd_failed.is_none());
    }

    /// None 必须序列化成 JSON null(而非 0),前端 `v == null` 兜底才生效。
    #[test]
    fn none_serializes_as_json_null() {
        let c: ReportContent =
            serde_json::from_str(LEGACY_REPORT_JSON).unwrap();
        let v = serde_json::to_value(&c).unwrap();
        assert!(v["online_rate"].is_null());
        assert!(v["lux_max"].is_null());
        assert!(v["cmd_failed"].is_null());
        assert!(v.get("cmd_auto").is_none(), "死字段不应再出现在响应里");
    }

    /// 百分比:分母为 0 → None(无从计算,而非 0%);否则保留 1 位小数。
    #[test]
    fn pct1_guards_zero_total_and_rounds() {
        assert!(pct1(0, 0).is_none());
        assert!(approx(pct1(1, 3), 33.3));
        assert!(approx(pct1(2, 3), 66.7));
        assert!(approx(pct1(3, 3), 100.0));
        assert!(approx(pct1(0, 3), 0.0));
    }
}
