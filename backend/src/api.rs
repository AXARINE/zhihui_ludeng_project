use crate::AppState;
use crate::assistant;
use crate::auth::Auth;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
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
    RateLimited(String),
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
            Self::Db(_) | Self::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Iothub(_) => StatusCode::BAD_GATEWAY,
            Self::IothubUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
        };
        (status, self.to_string()).into_response()
    }
}

/// 灯控动作:serde 按小写反序列化(on/off/auto),非法值由 axum 直接拒收
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
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

/// 从 DB 文本还原(`command_record.action` 只由本后端写入,非法值视为数据损坏)
impl TryFrom<String> for LampAction {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "auto" => Ok(Self::Auto),
            _ => Err(format!("非法灯控动作: {s}")),
        }
    }
}

// ---- 设备/指令表的封闭取值枚举:serde 小写(对外 JSON 与原字符串逐字节一致),
// ---- sqlx 经 `try_from = "String"` 解码;DB 里出现未知值时兜底 Unknown,不让查询失败
macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
        #[serde(rename_all = "lowercase")]
        pub enum $name {
            $($variant),+,
            #[serde(other)]
            Unknown,
        }

        // sqlx `try_from` 属性要求 TryFrom;未知值兜底 Unknown,永不失败
        #[allow(clippy::infallible_try_from)]
        impl TryFrom<String> for $name {
            type Error = std::convert::Infallible;
            fn try_from(s: String) -> Result<Self, Self::Error> {
                Ok(match s.as_str() {
                    $($text => Self::$variant,)+
                    _ => Self::Unknown,
                })
            }
        }
    };
}

text_enum! {
    /// 设备在线状态(`device.status`,由轮询器写入 online/offline)
    DeviceStatus { Online => "online", Offline => "offline" }
}
text_enum! {
    /// 灯态(`device.lamp`,影子 `LightStatus` 小写化)
    LampState { On => "on", Off => "off" }
}
text_enum! {
    /// 控制模式(`device.mode`)
    ControlMode { Auto => "auto", Manual => "manual" }
}
text_enum! {
    /// 指令来源(`command_record.source`)
    CommandSource { Manual => "manual", Auto => "auto" }
}
text_enum! {
    /// 指令状态(`command_record.status`:北向是否受理)
    CommandStatus { Sent => "sent", Failed => "failed" }
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub location: String,
    #[sqlx(try_from = "String")]
    pub status: DeviceStatus,
    #[sqlx(try_from = "String")]
    pub lamp: LampState,
    #[sqlx(try_from = "String")]
    pub mode: ControlMode,
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
    #[sqlx(try_from = "String")]
    pub action: LampAction,
    #[sqlx(try_from = "String")]
    pub source: CommandSource,
    #[sqlx(try_from = "String")]
    pub status: CommandStatus,
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
    /// keyset 游标:只返回 `created_at` 严格早于该时刻的记录(配合 `limit` 翻页)
    pub before: Option<String>,
    /// 每页条数,默认 1000,上限 5000
    pub limit: Option<i64>,
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

pub fn router(state: AppState) -> Router {
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
        .route("/api/assistant/ask", post(assistant_ask))
        .with_state(state)
}

pub fn parse_ts(param: &str, raw: &str) -> Result<DateTime<Utc>, Error> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            Error::BadRequest(format!(
                "bad {param}: 需为 RFC3339 时间(如 2026-08-24T10:00:00Z)"
            ))
        })
}

/// 解析后的时间区间(from, to;任一侧可缺省)
pub type TimeRange = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);

/// 解析查询参数里的 from/to 时间区间(任一侧可缺省)
pub fn parse_time_range(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<TimeRange, Error> {
    Ok((
        from.map(|v| parse_ts("from", v)).transpose()?,
        to.map(|v| parse_ts("to", v)).transpose()?,
    ))
}

pub fn clamp_limit(limit: Option<i64>, default: i64, max: i64) -> i64 {
    limit.unwrap_or(default).clamp(1, max)
}

// ---------------- 健康检查(公开) ----------------
#[utoipa::path(
    get,
    path = "/api/health",
    responses((status = 200, description = "服务与数据库状态"))
)]
async fn health(
    State(s): State<AppState>,
) -> Result<Json<serde_json::Value>, Error> {
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
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<Device>>, Error> {
    auth.require(&s, "device:status").await?;
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
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateDevice>,
) -> Result<StatusCode, Error> {
    auth.require(&s, "device:manage").await?;
    let id = body.id.trim().to_string();
    if id.is_empty() || id.len() > 64 {
        return Err(Error::BadRequest("device id 长度需在 1~64 之间".into()));
    }
    let name = body.name.as_deref().unwrap_or(&id).trim();
    let location = body.location.as_deref().unwrap_or_default().trim();
    sqlx::query(
        "INSERT INTO device (id, name, location) VALUES ($1, $2, $3) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(&id)
    .bind(name)
    .bind(location)
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<UpdateDevice>,
) -> Result<Json<Device>, Error> {
    auth.require(&s, "device:manage").await?;
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    auth.require(&s, "device:manage").await?;
    // 静态 SQL 白名单(sqlx 0.9 起 query() 要求 SqlSafeStr,拒收动态 String)
    // 单事务执行:并发连接池上逐条 execute 中途失败会留下孤儿数据
    let mut tx = s.db.begin().await?;
    for sql in [
        "DELETE FROM device WHERE id = $1",
        "DELETE FROM config WHERE device_id = $1",
        "DELETE FROM lux_record WHERE device_id = $1",
        "DELETE FROM alarm WHERE device_id = $1",
        "DELETE FROM command_record WHERE device_id = $1",
    ] {
        sqlx::query(sql).bind(&id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 给 `lux_record` 查询追加统一的 WHERE 条件
/// (`device_id` + 可选时间区间 + 可选 keyset 游标 `before`,严格小于)
pub fn push_lux_filters(
    qb: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    id: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) {
    qb.push_bind(id);
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    if let Some(before) = before {
        qb.push(" AND created_at < ").push_bind(before);
    }
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<Option<LuxRecord>>, Error> {
    auth.require(&s, "luminance:monitor").await?;
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
        ("to" = Option<String>, Query, description = "RFC3339 结束时间"),
        ("before" = Option<String>, Query, description = "keyset 游标:仅返回早于该时刻的记录(翻页用上一页最后一条的 created_at)"),
        ("limit" = Option<i64>, Query, description = "返回条数,默认 1000,上限 5000")
    ),
    responses((status = 200, description = "历史光照(倒序,默认最多 1000 条,配合 before+limit 翻页)", body = Vec<LuxRecord>)),
    security(("bearer_auth" = []))
)]
async fn lux_history(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<LuxRecord>>, Error> {
    auth.require(&s, "luminance:history").await?;
    let (from, to) = parse_time_range(q.from.as_deref(), q.to.as_deref())?;
    let before = q
        .before
        .as_deref()
        .map(|v| parse_ts("before", v))
        .transpose()?;
    let limit = clamp_limit(q.limit, 1_000, 5_000);
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, lux, created_at FROM lux_record WHERE device_id = ",
    );
    push_lux_filters(&mut qb, &id, from, to, before);
    qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push(limit);
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<LuxStats>, Error> {
    auth.require(&s, "luminance:history").await?;
    let (from, to) = parse_time_range(q.from.as_deref(), q.to.as_deref())?;

    let mut agg_qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(*)::bigint AS count, MIN(lux)::int AS min, \
                MAX(lux)::int AS max, AVG(lux)::float8 AS avg \
         FROM lux_record WHERE device_id = ",
    );
    push_lux_filters(&mut agg_qb, &id, from, to, None);

    let mut latest_qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, lux, created_at FROM lux_record WHERE device_id = ",
    );
    push_lux_filters(&mut latest_qb, &id, from, to, None);
    latest_qb.push(" ORDER BY created_at DESC, id DESC LIMIT 1");

    // 聚合与最新值互不依赖,并发执行
    let (agg, latest) = tokio::try_join!(
        agg_qb.build_query_as::<LuxAgg>().fetch_one(&s.db),
        latest_qb
            .build_query_as::<LuxRecord>()
            .fetch_optional(&s.db),
    )?;

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
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<DeviceLuxLatest>>, Error> {
    auth.require(&s, "luminance:monitor").await?;
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<LampBody>,
) -> Result<StatusCode, Error> {
    auth.require(&s, "control:manual").await?;
    let hub = s.iothub.as_ref().ok_or(Error::IothubUnavailable)?;
    // 指令留痕:北向接受记 sent,失败记 failed(固件执行结果不回传,无法追踪)
    let result = hub.control_led(&id, body.action).await;
    let (status, message) = result.as_ref().map_or_else(
        |e| ("failed", e.to_string()),
        |()| ("sent", String::new()),
    );
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
) -> Result<Json<ThresholdResponse>, Error> {
    auth.require(&s, "config:threshold").await?;
    let threshold = sqlx::query_scalar::<_, i32>(
        "SELECT threshold FROM config WHERE device_id = $1",
    )
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Json(body): Json<ThresholdBody>,
) -> Result<StatusCode, Error> {
    auth.require(&s, "config:threshold").await?;
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<String>,
    Query(q): Query<CommandQuery>,
) -> Result<Json<Vec<CommandRecord>>, Error> {
    auth.require(&s, "command:log").await?;
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
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<CommandQuery>,
) -> Result<Json<Vec<CommandRecord>>, Error> {
    auth.require(&s, "command:log").await?;
    let rows = query_commands(&s.db, None, &q).await?;
    Ok(Json(rows))
}

async fn query_commands(
    db: &sqlx::PgPool,
    device_id: Option<&str>,
    q: &CommandQuery,
) -> Result<Vec<CommandRecord>, Error> {
    let (from, to) = parse_time_range(q.from.as_deref(), q.to.as_deref())?;
    let limit = clamp_limit(q.limit, 500, 5000);
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, action, source, status, message, created_at \
         FROM command_record WHERE 1=1",
    );
    // 路径参数优先于查询参数
    if let Some(id) = device_id.or(q.device_id.as_deref()) {
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
    State(s): State<AppState>,
    auth: Auth,
    Query(q): Query<AlarmQuery>,
) -> Result<Json<Vec<Alarm>>, Error> {
    auth.require(&s, "alarm:log").await?;
    let (from, to) = parse_time_range(q.from.as_deref(), q.to.as_deref())?;
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
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<i64>,
    Json(body): Json<AlarmPatch>,
) -> Result<Json<Alarm>, Error> {
    auth.require(&s, "alarm:log").await?;
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
async fn dashboard(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Dashboard>, Error> {
    auth.require(&s, "device:status").await?;
    // 四类聚合并发执行(池上限 5 连接,恰好够用)
    let (devices, alarms, lux, commands) = tokio::try_join!(
        sqlx::query_as::<_, DeviceCounts>(
            "SELECT COUNT(*)::bigint AS total, \
                    COUNT(*) FILTER (WHERE status = 'online')::bigint AS online, \
                    COUNT(*) FILTER (WHERE lamp = 'on')::bigint AS lamp_on \
             FROM device",
        )
        .fetch_one(&s.db),
        sqlx::query_as::<_, AlarmCounts>(
            "SELECT COUNT(*) FILTER (WHERE resolved_at IS NULL)::bigint AS open, \
                    COUNT(*) FILTER (WHERE created_at >= now() - interval '24 hours')::bigint AS last_24h \
             FROM alarm",
        )
        .fetch_one(&s.db),
        sqlx::query_as::<_, LuxCounts>(
            "SELECT COUNT(*)::bigint AS reports_24h, AVG(lux)::float8 AS avg_lux_24h \
             FROM lux_record WHERE created_at >= now() - interval '24 hours'",
        )
        .fetch_one(&s.db),
        sqlx::query_as::<_, CommandCounts>(
            "SELECT COUNT(*) FILTER (WHERE source = 'manual')::bigint AS manual_24h, \
                    COUNT(*) FILTER (WHERE source = 'auto')::bigint AS auto_24h \
             FROM command_record WHERE created_at >= now() - interval '24 hours'",
        )
        .fetch_one(&s.db),
    )?;
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

/// POST /api/assistant/ask —— 维护智能问答（本地知识库检索，无需外部大模型）
#[derive(Deserialize, ToSchema)]
struct AssistantAskIn {
    question: String,
}

#[derive(Serialize, ToSchema)]
struct AssistantAnswer {
    answer: String,
}

#[utoipa::path(
    post,
    path = "/api/assistant/ask",
    request_body = AssistantAskIn,
    responses(
        (status = 200, description = "问答结果", body = AssistantAnswer),
        (status = 403, description = "无权限")
    ),
    security(("bearer_auth" = []))
)]
async fn assistant_ask(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<AssistantAskIn>,
) -> Result<Json<AssistantAnswer>, Error> {
    auth.require(&s, "assistant:qa").await?;
    let answer = assistant::answer(&s.db, &body.question).await?;
    Ok(Json(AssistantAnswer { answer }))
}
