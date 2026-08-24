use crate::AppState;
use crate::auth::Auth;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use utoipa::ToSchema;

/// API 统一错误:`IntoResponse` 映射为 (status, message),handler 全程 `?` 组合
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("IoTDA 北向调用失败: {0:#}")]
    Iothub(#[from] anyhow::Error),
    #[error("IoTDA 北向未配置(HUAWEI_* 环境变量缺失)")]
    IothubUnavailable,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Db(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Iothub(_) => StatusCode::BAD_GATEWAY,
            Self::IothubUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
        };
        (status, self.to_string()).into_response()
    }
}

/// 灯控动作:serde 按小写反序列化(on/off/auto),非法值由 axum 直接拒收
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LampAction {
    On,
    Off,
    Auto,
}

impl LampAction {
    /// `IoTDA` 命令参数取值(大写)
    #[must_use]
    pub const fn as_iotda_str(self) -> &'static str {
        match self {
            Self::On => "ON",
            Self::Off => "OFF",
            Self::Auto => "AUTO",
        }
    }
}

impl fmt::Display for LampAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Auto => "auto",
        })
    }
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub location: String,
    pub status: String,
    pub lamp: String,
    pub mode: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct LuxRecord {
    pub id: i64,
    pub device_id: String,
    pub lux: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Alarm {
    pub id: i64,
    pub device_id: String,
    pub r#type: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct CommandRecord {
    pub id: i64,
    pub device_id: String,
    pub action: String,
    pub source: String,
    pub status: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateDevice {
    pub id: String,
    pub name: Option<String>,
    pub location: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateDevice {
    pub name: Option<String>,
    pub location: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct LampBody {
    pub action: LampAction,
}

#[derive(Deserialize)]
pub struct CommandQuery {
    pub device_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct AlarmQuery {
    pub device_id: Option<String>,
    pub resolved: Option<bool>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ThresholdResponse {
    pub device_id: String,
    pub threshold: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct ThresholdBody {
    pub threshold: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct AlarmPatch {
    pub resolved: bool,
}

#[derive(Serialize, ToSchema)]
pub struct LuxStats {
    pub device_id: String,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub count: i64,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub avg: Option<f64>,
    pub latest: Option<LuxRecord>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct DeviceLuxLatest {
    pub device_id: String,
    pub id: Option<i64>,
    pub lux: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema)]
pub struct Dashboard {
    pub devices: DashboardDevices,
    pub alarms: DashboardAlarms,
    pub lux_24h: DashboardLux,
    pub commands_24h: DashboardCommands,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardDevices {
    pub total: i64,
    pub online: i64,
    pub lamp_on: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardAlarms {
    pub open: i64,
    pub last_24h: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardLux {
    pub reports_24h: i64,
    pub avg_lux_24h: Option<f64>,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardCommands {
    pub manual_24h: i64,
    pub auto_24h: i64,
}

#[derive(sqlx::FromRow)]
struct LuxAgg {
    count: i64,
    min: Option<i32>,
    max: Option<i32>,
    avg: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct DeviceCounts {
    total: i64,
    online: i64,
    lamp_on: i64,
}

#[derive(sqlx::FromRow)]
struct AlarmCounts {
    open: i64,
    last_24h: i64,
}

#[derive(sqlx::FromRow)]
struct LuxCounts {
    reports_24h: i64,
    avg_lux_24h: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct CommandCounts {
    manual_24h: i64,
    auto_24h: i64,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/devices", get(list_devices).post(create_device))
        .route(
            "/api/devices/{id}",
            patch(update_device).delete(delete_device),
        )
        .route("/api/devices/{id}/lux/latest", get(lux_latest))
        .route("/api/devices/{id}/lux/history", get(lux_history))
        .route("/api/devices/{id}/lux/stats", get(lux_stats))
        .route("/api/devices/{id}/lamp", post(set_lamp))
        .route("/api/devices/{id}/commands", get(list_device_commands))
        .route(
            "/api/devices/{id}/threshold",
            get(get_threshold).put(put_threshold),
        )
        .route("/api/alarms", get(list_alarms))
        .route("/api/alarms/{id}", patch(patch_alarm))
        .route("/api/lux/latest", get(global_lux_latest))
        .route("/api/commands", get(list_global_commands))
        .route("/api/dashboard", get(dashboard))
        .with_state(state)
}

fn parse_ts(param: &str, raw: &str) -> Result<DateTime<Utc>, Error> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            Error::BadRequest(format!(
                "bad {param}: 需为 RFC3339 时间(如 2026-08-24T10:00:00Z)"
            ))
        })
}

fn clamp_limit(limit: Option<i64>, default: i64, max: i64) -> i64 {
    limit.unwrap_or(default).clamp(1, max)
}

// ---------------- 健康检查(公开) ----------------
#[utoipa::path(
    get,
    path = "/api/health",
    responses((status = 200, description = "服务与数据库状态"))
)]
async fn health(State(s): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, Error> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&s.db)
        .await?;
    Ok(Json(
        serde_json::json!({"status": "ok", "database": "connected"}),
    ))
}

// ---------------- 设备管理 ----------------
#[utoipa::path(
    get,
    path = "/api/devices",
    responses((status = 200, description = "设备列表", body = Vec<Device>)),
    security(("bearer_auth" = []))
)]
async fn list_devices(
    State(s): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<Device>>, Error> {
    auth.require(&s.db, "device:status").await?;
    let rows = sqlx::query_as::<_, Device>(
        "SELECT id, name, location, status, lamp, mode, last_seen_at, created_at \
         FROM device ORDER BY created_at",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[utoipa::path(
    post,
    path = "/api/devices",
    request_body = CreateDevice,
    responses(
        (status = 201, description = "设备已创建(已存在则为幂等成功)"),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
async fn create_device(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Json(body): Json<CreateDevice>,
) -> Result<StatusCode, Error> {
    auth.require(&s.db, "device:manage").await?;
    let id = body.id.trim().to_string();
    if id.is_empty() || id.len() > 64 {
        return Err(Error::BadRequest("device id 长度需在 1~64 之间".into()));
    }
    sqlx::query(
        "INSERT INTO device (id, name, location) VALUES ($1, $2, $3) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&id)
    .bind(body.name.unwrap_or_else(|| id.clone()).trim())
    .bind(body.location.unwrap_or_default().trim())
    .execute(&s.db)
    .await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    patch,
    path = "/api/devices/{id}",
    params(("id" = String, Path, description = "设备 ID")),
    request_body = UpdateDevice,
    responses(
        (status = 200, description = "设备资料已更新", body = Device),
        (status = 400, description = "没有可更新的字段"),
        (status = 403, description = "无权限"),
        (status = 404, description = "设备不存在")
    ),
    security(("bearer_auth" = []))
)]
async fn update_device(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<UpdateDevice>,
) -> Result<Json<Device>, Error> {
    auth.require(&s.db, "device:manage").await?;
    let mut qb = sqlx::QueryBuilder::new("UPDATE device SET ");
    let mut changed = false;
    {
        let mut sep = qb.separated(", ");
        if let Some(name) = body
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            sep.push("name = ").push_bind_unseparated(name);
            changed = true;
        }
        if let Some(location) = body
            .location
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            sep.push("location = ").push_bind_unseparated(location);
            changed = true;
        }
    }
    if !changed {
        return Err(Error::BadRequest(
            "name/location 至少提供一个非空字段".into(),
        ));
    }
    qb.push(" WHERE id = ").push_bind(&id);
    let result = qb.build().execute(&s.db).await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("设备 {id} 不存在")));
    }
    let row = sqlx::query_as::<_, Device>(
        "SELECT id, name, location, status, lamp, mode, last_seen_at, created_at \
         FROM device WHERE id = $1",
    )
    .bind(&id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

#[utoipa::path(
    delete,
    path = "/api/devices/{id}",
    params(("id" = String, Path, description = "设备 ID")),
    responses(
        (status = 204, description = "设备及其关联数据已删除"),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
async fn delete_device(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    auth.require(&s.db, "device:manage").await?;
    // 静态 SQL 白名单(sqlx 0.9 起 query() 要求 SqlSafeStr,拒收动态 String)
    futures::future::try_join_all(
        [
            "DELETE FROM device WHERE id = $1",
            "DELETE FROM config WHERE device_id = $1",
            "DELETE FROM lux_record WHERE device_id = $1",
            "DELETE FROM alarm WHERE device_id = $1",
            "DELETE FROM command_record WHERE device_id = $1",
        ]
        .into_iter()
        .map(|sql| sqlx::query(sql).bind(&id).execute(&s.db)),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------- 光照 ----------------
#[utoipa::path(
    get,
    path = "/api/devices/{id}/lux/latest",
    params(("id" = String, Path, description = "设备 ID")),
    responses((status = 200, description = "最新一条光照", body = Option<LuxRecord>)),
    security(("bearer_auth" = []))
)]
async fn lux_latest(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<Option<LuxRecord>>, Error> {
    auth.require(&s.db, "luminance:monitor").await?;
    let row = sqlx::query_as::<_, LuxRecord>(
        "SELECT id, device_id, lux, created_at FROM lux_record \
         WHERE device_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&s.db)
    .await?;
    Ok(Json(row))
}

#[utoipa::path(
    get,
    path = "/api/devices/{id}/lux/history",
    params(
        ("id" = String, Path, description = "设备 ID"),
        ("from" = Option<String>, Query, description = "RFC3339 起始时间"),
        ("to" = Option<String>, Query, description = "RFC3339 结束时间")
    ),
    responses((status = 200, description = "历史光照(倒序,最多 5000 条)", body = Vec<LuxRecord>)),
    security(("bearer_auth" = []))
)]
async fn lux_history(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<LuxRecord>>, Error> {
    auth.require(&s.db, "luminance:history").await?;
    let from = q.from.as_deref().map(|v| parse_ts("from", v)).transpose()?;
    let to = q.to.as_deref().map(|v| parse_ts("to", v)).transpose()?;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, lux, created_at FROM lux_record WHERE device_id = ",
    );
    qb.push_bind(&id);
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    qb.push(" ORDER BY created_at DESC, id DESC LIMIT 5000");
    let rows = qb.build_query_as::<LuxRecord>().fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[utoipa::path(
    get,
    path = "/api/devices/{id}/lux/stats",
    params(
        ("id" = String, Path, description = "设备 ID"),
        ("from" = Option<String>, Query, description = "RFC3339 起始时间"),
        ("to" = Option<String>, Query, description = "RFC3339 结束时间")
    ),
    responses((status = 200, description = "光照统计(条数/最低/最高/平均/最新)", body = LuxStats)),
    security(("bearer_auth" = []))
)]
async fn lux_stats(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<LuxStats>, Error> {
    auth.require(&s.db, "luminance:history").await?;
    let from = q.from.as_deref().map(|v| parse_ts("from", v)).transpose()?;
    let to = q.to.as_deref().map(|v| parse_ts("to", v)).transpose()?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(*)::bigint AS count, MIN(lux)::int AS min, \
                MAX(lux)::int AS max, AVG(lux)::float8 AS avg \
         FROM lux_record WHERE device_id = ",
    );
    qb.push_bind(&id);
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    let agg = qb.build_query_as::<LuxAgg>().fetch_one(&s.db).await?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, lux, created_at FROM lux_record WHERE device_id = ",
    );
    qb.push_bind(&id);
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    qb.push(" ORDER BY created_at DESC, id DESC LIMIT 1");
    let latest = qb
        .build_query_as::<LuxRecord>()
        .fetch_optional(&s.db)
        .await?;

    Ok(Json(LuxStats {
        device_id: id,
        from,
        to,
        count: agg.count,
        min: agg.min,
        max: agg.max,
        avg: agg.avg.map(|v| (v * 10.0).round() / 10.0),
        latest,
    }))
}

#[utoipa::path(
    get,
    path = "/api/lux/latest",
    responses((status = 200, description = "所有设备的最新光照", body = Vec<DeviceLuxLatest>)),
    security(("bearer_auth" = []))
)]
async fn global_lux_latest(
    State(s): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<DeviceLuxLatest>>, Error> {
    auth.require(&s.db, "luminance:monitor").await?;
    let rows = sqlx::query_as::<_, DeviceLuxLatest>(
        "SELECT d.id AS device_id, l.id, l.lux, l.created_at \
         FROM device d \
         LEFT JOIN LATERAL (\
           SELECT id, lux, created_at FROM lux_record \
           WHERE device_id = d.id ORDER BY created_at DESC, id DESC LIMIT 1\
         ) l ON true \
         ORDER BY d.created_at DESC",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

// ---------------- 控灯 / 阈值 ----------------
#[utoipa::path(
    post,
    path = "/api/devices/{id}/lamp",
    params(("id" = String, Path, description = "设备 ID")),
    request_body = LampBody,
    responses(
        (status = 202, description = "IoTDA 北向已受理"),
        (status = 403, description = "无权限"),
        (status = 502, description = "IoTDA 调用失败"),
        (status = 503, description = "IoTDA 未配置")
    ),
    security(("bearer_auth" = []))
)]
async fn set_lamp(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<LampBody>,
) -> Result<StatusCode, Error> {
    auth.require(&s.db, "control:manual").await?;
    let hub = s.iothub.as_ref().ok_or(Error::IothubUnavailable)?;
    // 指令留痕:北向接受记 sent,失败记 failed(固件执行结果不回传,无法追踪)
    let result = hub.control_led(&id, body.action).await;
    let (status, message) = result
        .as_ref()
        .map_or_else(|e| ("failed", e.to_string()), |()| ("sent", String::new()));
    sqlx::query(
        "INSERT INTO command_record (device_id, action, source, status, message) \
         VALUES ($1, $2, 'manual', $3, $4)",
    )
    .bind(&id)
    .bind(body.action.to_string())
    .bind(status)
    .bind(message)
    .execute(&s.db)
    .await?;
    result?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    get,
    path = "/api/devices/{id}/threshold",
    params(("id" = String, Path, description = "设备 ID")),
    responses((status = 200, description = "当前阈值(未配置默认 40)", body = ThresholdResponse)),
    security(("bearer_auth" = []))
)]
async fn get_threshold(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<ThresholdResponse>, Error> {
    auth.require(&s.db, "config:threshold").await?;
    let threshold =
        sqlx::query_scalar::<_, i32>("SELECT threshold FROM config WHERE device_id = $1")
            .bind(&id)
            .fetch_optional(&s.db)
            .await?
            .unwrap_or(40);
    Ok(Json(ThresholdResponse {
        device_id: id,
        threshold,
    }))
}

#[utoipa::path(
    put,
    path = "/api/devices/{id}/threshold",
    params(("id" = String, Path, description = "设备 ID")),
    request_body = ThresholdBody,
    responses(
        (status = 204, description = "阈值已入库并下发 IoTDA"),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权限"),
        (status = 502, description = "IoTDA 调用失败"),
        (status = 503, description = "IoTDA 未配置")
    ),
    security(("bearer_auth" = []))
)]
async fn put_threshold(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<ThresholdBody>,
) -> Result<StatusCode, Error> {
    auth.require(&s.db, "config:threshold").await?;
    if !(0..=10_000).contains(&body.threshold) {
        return Err(Error::BadRequest("threshold 需在 0~10000 之间".into()));
    }
    sqlx::query(
        "INSERT INTO config (device_id, threshold) VALUES ($1, $2) \
         ON CONFLICT (device_id) DO UPDATE SET threshold = $2",
    )
    .bind(&id)
    .bind(body.threshold)
    .execute(&s.db)
    .await?;
    let hub = s.iothub.as_ref().ok_or(Error::IothubUnavailable)?;
    hub.set_threshold(&id, body.threshold).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------- 指令留痕 ----------------
#[utoipa::path(
    get,
    path = "/api/devices/{id}/commands",
    params(
        ("id" = String, Path, description = "设备 ID"),
        ("from" = Option<String>, Query, description = "RFC3339 起始时间"),
        ("to" = Option<String>, Query, description = "RFC3339 结束时间"),
        ("limit" = Option<i64>, Query, description = "返回条数(默认 500,最大 5000)")
    ),
    responses((status = 200, description = "设备指令记录(倒序)", body = Vec<CommandRecord>)),
    security(("bearer_auth" = []))
)]
async fn list_device_commands(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<CommandQuery>,
) -> Result<Json<Vec<CommandRecord>>, Error> {
    auth.require(&s.db, "command:log").await?;
    let rows = query_commands(&s.db, Some(&id), &q).await?;
    Ok(Json(rows))
}

#[utoipa::path(
    get,
    path = "/api/commands",
    params(
        ("device_id" = Option<String>, Query, description = "按设备过滤"),
        ("from" = Option<String>, Query, description = "RFC3339 起始时间"),
        ("to" = Option<String>, Query, description = "RFC3339 结束时间"),
        ("limit" = Option<i64>, Query, description = "返回条数(默认 500,最大 5000)")
    ),
    responses((status = 200, description = "全局指令记录(倒序)", body = Vec<CommandRecord>)),
    security(("bearer_auth" = []))
)]
async fn list_global_commands(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Query(q): Query<CommandQuery>,
) -> Result<Json<Vec<CommandRecord>>, Error> {
    auth.require(&s.db, "command:log").await?;
    let rows = query_commands(&s.db, None, &q).await?;
    Ok(Json(rows))
}

async fn query_commands(
    db: &sqlx::PgPool,
    device_id: Option<&str>,
    q: &CommandQuery,
) -> Result<Vec<CommandRecord>, Error> {
    let from = q.from.as_deref().map(|v| parse_ts("from", v)).transpose()?;
    let to = q.to.as_deref().map(|v| parse_ts("to", v)).transpose()?;
    let limit = clamp_limit(q.limit, 500, 5000);
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, action, source, status, message, created_at \
         FROM command_record WHERE 1=1",
    );
    if let Some(id) = device_id {
        qb.push(" AND device_id = ").push_bind(id);
    } else if let Some(id) = q.device_id.as_deref() {
        qb.push(" AND device_id = ").push_bind(id);
    }
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push(limit);
    Ok(qb.build_query_as::<CommandRecord>().fetch_all(db).await?)
}

// ---------------- 告警 ----------------
#[utoipa::path(
    get,
    path = "/api/alarms",
    params(
        ("device_id" = Option<String>, Query, description = "按设备过滤"),
        ("resolved" = Option<bool>, Query, description = "true 已处理 / false 未处理"),
        ("from" = Option<String>, Query, description = "RFC3339 起始时间"),
        ("to" = Option<String>, Query, description = "RFC3339 结束时间"),
        ("type" = Option<String>, Query, description = "告警类型"),
        ("limit" = Option<i64>, Query, description = "返回条数(默认 500,最大 5000)")
    ),
    responses((status = 200, description = "告警列表(倒序)", body = Vec<Alarm>)),
    security(("bearer_auth" = []))
)]
async fn list_alarms(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Query(q): Query<AlarmQuery>,
) -> Result<Json<Vec<Alarm>>, Error> {
    auth.require(&s.db, "alarm:log").await?;
    let from = q.from.as_deref().map(|v| parse_ts("from", v)).transpose()?;
    let to = q.to.as_deref().map(|v| parse_ts("to", v)).transpose()?;
    let limit = clamp_limit(q.limit, 500, 5000);
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, type, message, created_at, resolved_at FROM alarm WHERE 1=1",
    );
    if let Some(d) = q.device_id.as_deref() {
        qb.push(" AND device_id = ").push_bind(d);
    }
    if let Some(resolved) = q.resolved {
        qb.push(if resolved {
            " AND resolved_at IS NOT NULL"
        } else {
            " AND resolved_at IS NULL"
        });
    }
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    if let Some(t) = q.r#type.as_deref() {
        qb.push(" AND type = ").push_bind(t);
    }
    qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push(limit);
    let rows = qb.build_query_as::<Alarm>().fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[utoipa::path(
    patch,
    path = "/api/alarms/{id}",
    params(("id" = i64, Path, description = "告警 ID")),
    request_body = AlarmPatch,
    responses(
        (status = 200, description = "处理状态已更新", body = Alarm),
        (status = 403, description = "无权限"),
        (status = 404, description = "告警不存在")
    ),
    security(("bearer_auth" = []))
)]
async fn patch_alarm(
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<i64>,
    Json(body): Json<AlarmPatch>,
) -> Result<Json<Alarm>, Error> {
    auth.require(&s.db, "alarm:log").await?;
    let row = if body.resolved {
        sqlx::query_as::<_, Alarm>(
            "UPDATE alarm SET resolved_at = COALESCE(resolved_at, now()) WHERE id = $1 \
             RETURNING id, device_id, type, message, created_at, resolved_at",
        )
    } else {
        sqlx::query_as::<_, Alarm>(
            "UPDATE alarm SET resolved_at = NULL WHERE id = $1 \
             RETURNING id, device_id, type, message, created_at, resolved_at",
        )
    }
    .bind(id)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| Error::NotFound(format!("告警 {id} 不存在")))?;
    Ok(Json(row))
}

// ---------------- 仪表盘聚合 ----------------
#[utoipa::path(
    get,
    path = "/api/dashboard",
    responses((status = 200, description = "首页聚合数据", body = Dashboard)),
    security(("bearer_auth" = []))
)]
async fn dashboard(State(s): State<Arc<AppState>>, auth: Auth) -> Result<Json<Dashboard>, Error> {
    auth.require(&s.db, "device:status").await?;
    let devices = sqlx::query_as::<_, DeviceCounts>(
        "SELECT COUNT(*)::bigint AS total, \
                COUNT(*) FILTER (WHERE status = 'online')::bigint AS online, \
                COUNT(*) FILTER (WHERE lamp = 'on')::bigint AS lamp_on \
         FROM device",
    )
    .fetch_one(&s.db)
    .await?;
    let alarms = sqlx::query_as::<_, AlarmCounts>(
        "SELECT COUNT(*) FILTER (WHERE resolved_at IS NULL)::bigint AS open, \
                COUNT(*) FILTER (WHERE created_at >= now() - interval '24 hours')::bigint AS last_24h \
         FROM alarm",
    )
    .fetch_one(&s.db)
    .await?;
    let lux = sqlx::query_as::<_, LuxCounts>(
        "SELECT COUNT(*)::bigint AS reports_24h, AVG(lux)::float8 AS avg_lux_24h \
         FROM lux_record WHERE created_at >= now() - interval '24 hours'",
    )
    .fetch_one(&s.db)
    .await?;
    let commands = sqlx::query_as::<_, CommandCounts>(
        "SELECT COUNT(*) FILTER (WHERE source = 'manual')::bigint AS manual_24h, \
                COUNT(*) FILTER (WHERE source = 'auto')::bigint AS auto_24h \
         FROM command_record WHERE created_at >= now() - interval '24 hours'",
    )
    .fetch_one(&s.db)
    .await?;
    Ok(Json(Dashboard {
        devices: DashboardDevices {
            total: devices.total,
            online: devices.online,
            lamp_on: devices.lamp_on,
        },
        alarms: DashboardAlarms {
            open: alarms.open,
            last_24h: alarms.last_24h,
        },
        lux_24h: DashboardLux {
            reports_24h: lux.reports_24h,
            avg_lux_24h: lux.avg_lux_24h.map(|v| (v * 10.0).round() / 10.0),
        },
        commands_24h: DashboardCommands {
            manual_24h: commands.manual_24h,
            auto_24h: commands.auto_24h,
        },
    }))
}
