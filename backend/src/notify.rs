// 通知系统 + 每日日报
//
// 功能：
//   - 维修通知：市政人员/系统管理员对异常路灯发起，通知路灯管理员（admin 角色）
//   - 红点未读：GET /api/notifications/unread-count
//   - 每日日报：每天 09:00 由定时任务生成（懒生成兜底），所有角色可查
use crate::api::Error;
use crate::auth::Auth;
use crate::AppState;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;


// ---------------- 通知 ----------------

#[derive(Serialize, sqlx::FromRow)]
pub struct NotificationOut {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub r#type: String,
    pub device_id: Option<String>,
    pub receiver_role: String,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct NotifyIn {
    pub title: String,
    pub content: Option<String>,
    pub device_id: Option<String>,
    /// 收件角色：admin（路灯管理员，默认）/ all（全体）
    pub receiver_role: Option<String>,
}

/// POST /api/notifications —— 发起维修通知（notify:send，市政人员/系统管理员）
async fn create_notification(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<NotifyIn>,
) -> Result<Json<NotificationOut>, Error> {
    auth.require(&s, "notify:send").await?;
    if body.title.trim().is_empty() {
        return Err(Error::BadRequest("通知标题不能为空".into()));
    }
    let role = body.receiver_role.unwrap_or_else(|| "admin".into());
    if role != "admin" && role != "all" {
        return Err(Error::BadRequest("receiver_role 仅支持 admin / all".into()));
    }
    let row = sqlx::query_as::<_, NotificationOut>(
        "INSERT INTO notification (title, content, type, device_id, receiver_role) \
         VALUES ($1, $2, 'alert', $3, $4) \
         RETURNING id, title, content, type, device_id, receiver_role, is_read, created_at",
    )
    .bind(&body.title)
    .bind(body.content.unwrap_or_default())
    .bind(&body.device_id)
    .bind(&role)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

/// GET /api/notifications —— 当前角色可见的通知（receiver_role = 自己 或 all）
async fn list_notifications(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<NotificationOut>>, Error> {
    let rows = sqlx::query_as::<_, NotificationOut>(
        "SELECT id, title, content, type, device_id, receiver_role, is_read, created_at \
         FROM notification WHERE receiver_role IN ($1, 'all') \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&auth.role_code)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

/// GET /api/notifications/unread-count —— 未读数（红点）
async fn unread_count(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<serde_json::Value>, Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notification \
         WHERE receiver_role IN ($1, 'all') AND is_read = FALSE",
    )
    .bind(&auth.role_code)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(serde_json::json!({ "unread": n })))
}

/// POST /api/notifications/{id}/read —— 标记已读
async fn mark_read(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<i64>,
) -> Result<axum::http::StatusCode, Error> {
    let updated = sqlx::query(
        "UPDATE notification SET is_read = TRUE \
         WHERE id = $1 AND receiver_role IN ($2, 'all')",
    )
    .bind(id)
    .bind(&auth.role_code)
    .execute(&s.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(Error::NotFound(format!("通知 {id} 不存在")));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------- 每日日报 ----------------

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize)]
pub struct ReportOut {
    pub report_date: String,
    pub content: ReportContent,
    pub created_at: DateTime<Utc>,
}

/// 聚合生成某一天的日报（写入 daily_report + 生成全体通知）
async fn generate_report(db: &PgPool, date: NaiveDate) -> Result<(), sqlx::Error> {
    let day_start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let day_end = day_start + Duration::days(1);

    let (devices_total, devices_online, lamp_on): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'online'), \
                COUNT(*) FILTER (WHERE lamp = 'on') FROM device",
    )
    .fetch_one(db)
    .await?;
    let (alarms_today, alarms_unhandled): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE resolved_at IS NULL) \
         FROM alarm WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db)
    .await?;
    let (reports_lux, avg_lux): (i64, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(*), AVG(lux)::float8 FROM lux_record \
         WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db)
    .await?;
    let (cmd_manual, cmd_auto): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE source = 'manual'), \
                COUNT(*) FILTER (WHERE source = 'auto') \
         FROM command_record WHERE created_at >= $1 AND created_at < $2",
    )
    .bind(day_start)
    .bind(day_end)
    .fetch_one(db)
    .await?;

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

    let json = serde_json::to_value(&content).unwrap_or_default();
    sqlx::query(
        "INSERT INTO daily_report (report_date, content) VALUES ($1, $2) \
         ON CONFLICT (report_date) DO UPDATE SET content = EXCLUDED.content",
    )
    .bind(date)
    .bind(&json)
    .execute(db)
    .await?;
    // 生成一条全体可见的日报通知
    sqlx::query(
        "INSERT INTO notification (title, content, type, device_id, receiver_role) \
         VALUES ('每日日报', $1, 'report', NULL, 'all') \
         ON CONFLICT DO NOTHING",
    )
    .bind(format!(
        "{} 设备 {} 台（在线 {}），当日告警 {} 条，平均光照 {} lux，手动指令 {} 次",
        content.date,
        content.devices_total,
        content.devices_online,
        content.alarms_today,
        content.avg_lux as i64,
        content.cmd_manual,
    ))
    .execute(db)
    .await?;
    Ok(())
}

/// GET /api/reports/today —— 最近一份日报（每天 09:00 生成的是"前一日"数据；无则懒生成昨日）
async fn report_today(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<ReportOut>, Error> {
    auth.require(&s, "luminance:history").await?;
    let row = sqlx::query_as::<_, (NaiveDate, serde_json::Value, DateTime<Utc>)>(
        "SELECT report_date, content, created_at FROM daily_report \
         ORDER BY report_date DESC LIMIT 1",
    )
    .fetch_optional(&s.db)
    .await?;
    let row = match row {
        Some(r) => r,
        None => {
            // 无任何日报时，懒生成"昨天"的（与定时任务口径一致：日报 = 前一日数据）
            let yesterday = Local::now().date_naive() - Duration::days(1);
            generate_report(&s.db, yesterday).await?;
            sqlx::query_as::<_, (NaiveDate, serde_json::Value, DateTime<Utc>)>(
                "SELECT report_date, content, created_at FROM daily_report \
                 ORDER BY report_date DESC LIMIT 1",
            )
            .fetch_one(&s.db)
            .await?
        }
    };
    let content: ReportContent = serde_json::from_value(row.1).map_err(|e| {
        Error::BadRequest(format!("日报内容解析失败: {e}"))
    })?;
    Ok(Json(ReportOut {
        report_date: row.0.format("%Y-%m-%d").to_string(),
        content,
        created_at: row.2,
    }))
}

// ---------------- 定时任务：每天 09:00 生成日报 ----------------

pub async fn run(db: PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800));
    interval.tick().await; // 首次立即 tick，随后每 30 分钟检查一次
    loop {
        interval.tick().await;
        let now = Local::now();
        if now.time().hour() >= 9 {
            // 每天 09:00 生成的是"前一天"的日报（对昨日全天数据做总结）
            let date = now.date_naive() - Duration::days(1);
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM daily_report WHERE report_date = $1)",
            )
            .bind(date)
            .fetch_one(&db)
            .await
            .unwrap_or(false);
            if !exists {
                match generate_report(&db, date).await {
                    Ok(()) => tracing::info!("日报已生成: {date}"),
                    Err(e) => tracing::warn!("日报生成失败: {e}"),
                }
            }
        }
    }
}

// ---------------- 路由 ----------------

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/notifications", get(list_notifications).post(create_notification))
        .route("/api/notifications/unread-count", get(unread_count))
        .route("/api/notifications/{id}/read", post(mark_read))
        .route("/api/reports/today", get(report_today))
        .with_state(state)
}
