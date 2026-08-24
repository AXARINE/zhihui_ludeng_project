use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use std::sync::Arc;

type ApiResult<T> = Result<T, (StatusCode, String)>;

fn err500<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn bad_req(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

fn no_iothub() -> (StatusCode, String) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "IoTDA 北向未配置(HUAWEI_* 环境变量缺失)".to_string(),
    )
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct Device {
    id: String,
    name: String,
    location: String,
    status: String,
    lamp: String,
    mode: String,
    last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct LuxRecord {
    id: i64,
    device_id: String,
    lux: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct Alarm {
    id: i64,
    device_id: String,
    r#type: String,
    message: String,
    created_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct CommandRecord {
    id: i64,
    device_id: String,
    action: String,
    source: String,
    status: String,
    message: String,
    created_at: chrono::DateTime<chrono::Utc>,
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

async fn list_devices(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<Device>>> {
    let rows = sqlx::query_as::<_, Device>(
        "SELECT id, name, location, status, lamp, mode, last_seen_at, created_at FROM device ORDER BY created_at",
    )
    .fetch_all(&s.db)
    .await
    .map_err(err500)?;
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
) -> ApiResult<StatusCode> {
    sqlx::query("INSERT INTO device (id, name, location) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING")
        .bind(&body.id)
        .bind(body.name.unwrap_or_else(|| body.id.clone()))
        .bind(body.location.unwrap_or_default())
        .execute(&s.db)
        .await
        .map_err(err500)?;
    Ok(StatusCode::CREATED)
}

async fn delete_device(State(s): State<Arc<AppState>>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    // sqlx 0.9 起 query() 要求 SqlSafeStr(拒收动态 String);表名/列名来自这里的静态白名单
    for (table, col) in [
        ("device", "id"),
        ("config", "device_id"),
        ("lux_record", "device_id"),
        ("alarm", "device_id"),
    ] {
        let mut qb = sqlx::QueryBuilder::new("DELETE FROM ");
        qb.push(table).push(" WHERE ").push(col).push(" = ").push_bind(&id);
        qb.build().execute(&s.db).await.map_err(err500)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn lux_latest(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Option<LuxRecord>>> {
    let row = sqlx::query_as::<_, LuxRecord>(
        "SELECT id, device_id, lux, created_at FROM lux_record \
         WHERE device_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&id)
    .fetch_optional(&s.db)
    .await
    .map_err(err500)?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct HistoryQuery {
    from: Option<String>,
    to: Option<String>,
}

async fn lux_history(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<LuxRecord>>> {
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, lux, created_at FROM lux_record WHERE device_id = ",
    );
    qb.push_bind(id);
    if let Some(from) = q.from {
        let from = chrono::DateTime::parse_from_rfc3339(&from).map_err(|_| bad_req("bad from"))?;
        qb.push(" AND created_at >= ").push_bind(from.with_timezone(&chrono::Utc));
    }
    if let Some(to) = q.to {
        let to = chrono::DateTime::parse_from_rfc3339(&to).map_err(|_| bad_req("bad to"))?;
        qb.push(" AND created_at <= ").push_bind(to.with_timezone(&chrono::Utc));
    }
    qb.push(" ORDER BY created_at DESC LIMIT 5000");
    let rows = qb
        .build_query_as::<LuxRecord>()
        .fetch_all(&s.db)
        .await
        .map_err(err500)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct LampAction {
    action: String,
}

async fn set_lamp(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<LampAction>,
) -> ApiResult<StatusCode> {
    let led = match body.action.as_str() {
        "on" => "ON",
        "off" => "OFF",
        "auto" => "AUTO",
        _ => return Err(bad_req("action must be on|off|auto")),
    };
    let hub = s.iothub.as_ref().ok_or_else(no_iothub)?;
    // 指令留痕:北向接受记 sent,失败记 failed(固件执行结果不回传,无法追踪)
    let result = hub.control_led(&id, led).await;
    let (status, message) = match &result {
        Ok(()) => ("sent", String::new()),
        Err(e) => ("failed", e.to_string()),
    };
    sqlx::query(
        "INSERT INTO command_record (device_id, action, source, status, message) \
         VALUES ($1, $2, 'manual', $3, $4)",
    )
    .bind(&id)
    .bind(&body.action)
    .bind(status)
    .bind(message)
    .execute(&s.db)
    .await
    .map_err(err500)?;
    result.map_err(err500)?;
    Ok(StatusCode::ACCEPTED)
}

async fn list_commands(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<CommandRecord>>> {
    let rows = sqlx::query_as::<_, CommandRecord>(
        "SELECT id, device_id, action, source, status, message, created_at \
         FROM command_record WHERE device_id = $1 ORDER BY created_at DESC LIMIT 500",
    )
    .bind(&id)
    .fetch_all(&s.db)
    .await
    .map_err(err500)?;
    Ok(Json(rows))
}

async fn get_threshold(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let row: Option<(i32,)> = sqlx::query_as("SELECT threshold FROM config WHERE device_id = $1")
        .bind(&id)
        .fetch_optional(&s.db)
        .await
        .map_err(err500)?;
    let threshold = row.map_or(40, |r| r.0);
    Ok(Json(serde_json::json!({ "device_id": id, "threshold": threshold })))
}

#[derive(Deserialize)]
struct ThresholdBody {
    threshold: i32,
}

async fn put_threshold(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ThresholdBody>,
) -> ApiResult<StatusCode> {
    sqlx::query(
        "INSERT INTO config (device_id, threshold) VALUES ($1, $2) \
         ON CONFLICT (device_id) DO UPDATE SET threshold = $2",
    )
    .bind(&id)
    .bind(body.threshold)
    .execute(&s.db)
    .await
    .map_err(err500)?;
    let hub = s.iothub.as_ref().ok_or_else(no_iothub)?;
    hub.set_threshold(&id, body.threshold).await.map_err(err500)?;
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
) -> ApiResult<Json<Vec<Alarm>>> {
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
    let rows = qb
        .build_query_as::<Alarm>()
        .fetch_all(&s.db)
        .await
        .map_err(err500)?;
    Ok(Json(rows))
}
