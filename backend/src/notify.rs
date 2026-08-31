//! 通知系统:维修通知 + 未读红点
//!
//! - 维修通知:市政人员/系统管理员(`notify:send`)对异常路灯发起,收件人 = 路灯管理员(admin 角色)
//! - 红点未读:GET /api/notifications/unread-count
//!
//! 每日日报在 `crate::report` 模块;日报生成后经 [`insert_report_notification`]
//! 落一条全员可见的通知。
use crate::api::{text_enum, Error};
use crate::auth::Auth;
use crate::AppState;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

text_enum! {
    /// 通知类型(`notification.type`:维修通知/日报)
    NotificationType { Alert => "alert", Report => "report" }
}

text_enum! {
    /// 通知收件角色(`notification.receiver_role`)。
    /// 注意与 `auth` 的 `role_code(municipal/admin/super_admin)` 不是同一命名空间:
    /// 这里 admin = 路灯管理员,all = 全体角色可见。
    ReceiverRole { Admin => "admin", All => "all" }
}

impl ReceiverRole {
    /// 入库/比对用的文本值
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::All => "all",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct NotificationOut {
    pub id: i64,
    pub title: String,
    pub content: String,
    #[sqlx(try_from = "String")]
    pub r#type: NotificationType,
    pub device_id: Option<String>,
    #[sqlx(try_from = "String")]
    pub receiver_role: ReceiverRole,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct UnreadCountOut {
    pub unread: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct NotifyIn {
    pub title: String,
    pub content: Option<String>,
    pub device_id: Option<String>,
    /// 收件角色:admin(路灯管理员,默认)/ all(全体);非法值由 handler 拒收
    pub receiver_role: Option<ReceiverRole>,
}

/// POST /api/notifications —— 发起维修通知(notify:send,市政人员/系统管理员)
#[utoipa::path(
    post,
    path = "/api/notifications",
    request_body = NotifyIn,
    responses(
        (status = 200, description = "通知已创建", body = NotificationOut),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
async fn create_notification(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<NotifyIn>,
) -> Result<Json<NotificationOut>, Error> {
    auth.require(&s, "notify:send").await?;
    if body.title.trim().is_empty() {
        return Err(Error::BadRequest("通知标题不能为空".into()));
    }
    // 备选清单只在 ReceiverRole 类型里有一处权威定义;
    // serde(other) 兜到的非法值(Unknown)在这里显式拒收
    let role = match body.receiver_role {
        None => ReceiverRole::Admin,
        Some(r @ (ReceiverRole::Admin | ReceiverRole::All)) => r,
        Some(ReceiverRole::Unknown) => {
            return Err(Error::BadRequest("receiver_role 仅支持 admin / all".into()));
        }
    };
    let row = sqlx::query_as::<_, NotificationOut>(
        "INSERT INTO notification (title, content, type, device_id, receiver_role) \
         VALUES ($1, $2, 'alert', $3, $4) \
         RETURNING id, title, content, type, device_id, receiver_role, is_read, created_at",
    )
    .bind(body.title.trim())
    .bind(body.content.unwrap_or_default())
    .bind(&body.device_id)
    .bind(role.as_str())
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

/// GET /api/notifications —— 当前角色可见的通知(`receiver_role` = 自己 或 all)
#[utoipa::path(
    get,
    path = "/api/notifications",
    responses((status = 200, description = "通知列表", body = Vec<NotificationOut>)),
    security(("bearer_auth" = []))
)]
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

/// GET /api/notifications/unread-count —— 未读数(红点)
#[utoipa::path(
    get,
    path = "/api/notifications/unread-count",
    responses((status = 200, description = "未读数", body = UnreadCountOut)),
    security(("bearer_auth" = []))
)]
async fn unread_count(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<UnreadCountOut>, Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notification \
         WHERE receiver_role IN ($1, 'all') AND is_read = FALSE",
    )
    .bind(&auth.role_code)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(UnreadCountOut { unread: n }))
}

/// POST /api/notifications/{id}/read —— 标记已读
#[utoipa::path(
    post,
    path = "/api/notifications/{id}/read",
    params(("id" = i64, Path, description = "通知 ID")),
    responses(
        (status = 204, description = "已标记已读"),
        (status = 404, description = "通知不存在或当前角色不可见")
    ),
    security(("bearer_auth" = []))
)]
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

/// 供 `crate::report` 在生成日报后写入一条全员可见的日报通知。
///
/// 去重由调用方保证(`daily_report.report_date` 唯一 + 生成前 EXISTS 预查);
/// `notification` 表没有唯一约束,故此处不写 `ON CONFLICT`(写了也不会生效)。
pub async fn insert_report_notification(
    db: &PgPool,
    title: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO notification (title, content, type, device_id, receiver_role) \
         VALUES ($1, $2, 'report', NULL, 'all')",
    )
    .bind(title)
    .bind(content)
    .execute(db)
    .await?;
    Ok(())
}

/// 供设备自动同步(`iothub::sync_devices`)写入提醒(收件人 = 路灯管理员)。
///
/// 去重:同设备同标题的**未读**提醒已存在时跳过——漂移设备每轮同步都会命中,
/// 但提醒只保留到管理员读掉为止,不会按同步间隔重复刷屏。
/// (`IS NOT DISTINCT FROM` 同时覆盖 device_id 为 NULL 的情况。)
pub async fn insert_sync_notification(
    db: &PgPool,
    title: &str,
    content: &str,
    device_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO notification (title, content, type, device_id, receiver_role) \
         SELECT $1, $2, 'alert', $3, 'admin' \
         WHERE NOT EXISTS ( \
             SELECT 1 FROM notification \
             WHERE title = $1 AND is_read = FALSE \
             AND device_id IS NOT DISTINCT FROM $3)",
    )
    .bind(title)
    .bind(content)
    .bind(device_id)
    .execute(db)
    .await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/notifications",
            get(list_notifications).post(create_notification),
        )
        .route("/api/notifications/unread-count", get(unread_count))
        .route("/api/notifications/{id}/read", post(mark_read))
        .with_state(state)
}
