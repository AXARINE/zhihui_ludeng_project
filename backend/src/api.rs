use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

/// API 统一错误:`IntoResponse` 映射为 (status, message),handler 全程 `?` 组合
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("IoTDA 北向调用失败: {0:#}")]
    Iothub(#[from] anyhow::Error),
    #[error("{0}")]
    BadRequest(String),
    #[error("IoTDA 北向未配置(HUAWEI_* 环境变量缺失)")]
    IothubUnavailable,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Iothub(_) => StatusCode::BAD_GATEWAY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::IothubUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, self.to_string()).into_response()
    }
}

/// 灯控动作:serde 按小写反序列化(on/off/auto),非法值由 axum 直接拒收
#[derive(Debug, Clone, Copy, Deserialize)]
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

#[derive(Serialize, sqlx::FromRow)]
struct Device {
    id: String,
    name: String,
    location: String,
    status: String,
    lamp: String,
    mode: String,
    last_seen_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct LuxRecord {
    id: i64,
    device_id: String,
    lux: i32,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct Alarm {
    id: i64,
    device_id: String,
    r#type: String,
    message: String,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, sqlx::FromRow)]
struct CommandRecord {
    id: i64,
    device_id: String,
    action: String,
    source: String,
    status: String,
    message: String,
    created_at: DateTime<Utc>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/devices", get(list_devices).post(create_device))
        .route("/api/devices/{id}", delete(delete_device))
        .route("/api/devices/{id}/lux/latest", get(lux_latest))
        .route("/api/devices/{id}/lux/history", get(lux_history))
        .route("/api/devices/{id}/lamp", post(set_lamp))
        .route("/api/devices/{id}/commands", get(list_commands))
        .route(
            "/api/devices/{id}/threshold",
            get(get_threshold).put(put_threshold),
        )
        .route("/api/alarms", get(list_alarms))
        .with_state(state)
}

async fn list_devices(State(s): State<Arc<AppState>>) -> Result<Json<Vec<Device>>, Error> {
    let rows = sqlx::query_as::<_, Device>(
        "SELECT id, name, location, status, lamp, mode, last_seen_at, created_at FROM device ORDER BY created_at",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateDevice {
    id: String,
    name: Option<String>,
    location: Option<String>,
}

async fn create_device(
    State(s): State<Arc<AppState>>,
    Json(body): Json<CreateDevice>,
) -> Result<StatusCode, Error> {
    sqlx::query("INSERT INTO device (id, name, location) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING")
        .bind(&body.id)
        .bind(body.name.unwrap_or_else(|| body.id.clone()))
        .bind(body.location.unwrap_or_default())
        .execute(&s.db)
        .await?;
    Ok(StatusCode::CREATED)
}

async fn delete_device(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    // 静态 SQL 白名单(sqlx 0.9 起 query() 要求 SqlSafeStr,拒收动态 String)
    futures::future::try_join_all(
        [
            "DELETE FROM device WHERE id = $1",
            "DELETE FROM config WHERE device_id = $1",
            "DELETE FROM lux_record WHERE device_id = $1",
            "DELETE FROM alarm WHERE device_id = $1",
        ]
        .into_iter()
        .map(|sql| sqlx::query(sql).bind(&id).execute(&s.db)),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn lux_latest(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<LuxRecord>>, Error> {
    let row = sqlx::query_as::<_, LuxRecord>(
        "SELECT id, device_id, lux, created_at FROM lux_record \
         WHERE device_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&s.db)
    .await?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct HistoryQuery {
    from: Option<String>,
    to: Option<String>,
}

fn parse_ts(param: &str, raw: &str) -> Result<DateTime<Utc>, Error> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| Error::BadRequest(format!("bad {param}")))
}

async fn lux_history(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<LuxRecord>>, Error> {
    let from = q.from.as_deref().map(|v| parse_ts("from", v)).transpose()?;
    let to = q.to.as_deref().map(|v| parse_ts("to", v)).transpose()?;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, lux, created_at FROM lux_record WHERE device_id = ",
    );
    qb.push_bind(id);
    if let Some(from) = from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    qb.push(" ORDER BY created_at DESC LIMIT 5000");
    let rows = qb.build_query_as::<LuxRecord>().fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct LampBody {
    action: LampAction,
}

async fn set_lamp(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<LampBody>,
) -> Result<StatusCode, Error> {
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

async fn list_commands(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<CommandRecord>>, Error> {
    let rows = sqlx::query_as::<_, CommandRecord>(
        "SELECT id, device_id, action, source, status, message, created_at \
         FROM command_record WHERE device_id = $1 ORDER BY created_at DESC LIMIT 500",
    )
    .bind(&id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize)]
struct ThresholdResponse {
    device_id: String,
    threshold: i32,
}

async fn get_threshold(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ThresholdResponse>, Error> {
    let threshold = sqlx::query_scalar::<_, i32>("SELECT threshold FROM config WHERE device_id = $1")
        .bind(&id)
        .fetch_optional(&s.db)
        .await?
        .unwrap_or(40);
    Ok(Json(ThresholdResponse {
        device_id: id,
        threshold,
    }))
}

#[derive(Deserialize)]
struct ThresholdBody {
    threshold: i32,
}

async fn put_threshold(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ThresholdBody>,
) -> Result<StatusCode, Error> {
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

#[derive(Deserialize)]
struct AlarmQuery {
    device_id: Option<String>,
    resolved: Option<bool>,
}

async fn list_alarms(
    State(s): State<Arc<AppState>>,
    Query(q): Query<AlarmQuery>,
) -> Result<Json<Vec<Alarm>>, Error> {
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, type, message, created_at, resolved_at FROM alarm WHERE 1=1",
    );
    if let Some(d) = q.device_id {
        qb.push(" AND device_id = ").push_bind(d);
    }
    if let Some(resolved) = q.resolved {
        qb.push(if resolved {
            " AND resolved_at IS NOT NULL"
        } else {
            " AND resolved_at IS NULL"
        });
    }
    qb.push(" ORDER BY created_at DESC LIMIT 500");
    let rows = qb.build_query_as::<Alarm>().fetch_all(&s.db).await?;
    Ok(Json(rows))
}
