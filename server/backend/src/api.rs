use crate::auth;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
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
    lux: f32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct Alarm {
    id: i64,
    device_id: String,
    r#type: String,
    created_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // 设备相关
        .route("/api/devices", get(list_devices).post(create_device))
        .route("/api/devices/{id}", delete(delete_device))
        .route("/api/devices/{id}/lux/latest", get(lux_latest))
        .route("/api/devices/{id}/lux/history", get(lux_history))
        .route("/api/devices/{id}/lamp", post(set_lamp))
        .route("/api/devices/{id}/threshold", get(get_threshold).put(put_threshold))
        // 告警
        .route("/api/alarms", get(list_alarms))
        .route("/api/alarms/{id}/resolve", post(resolve_alarm))
        .route("/api/alarms/{id}/unresolve", post(unresolve_alarm))
        // 认证
        .route("/api/auth/login", post(login))
        // 账号管理
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", delete(delete_user))
        .route("/api/roles", get(list_roles))
        // 审计日志
        .route("/api/commands", get(list_commands))
        // 智能问答
        .route("/api/assistant/ask", post(assistant_ask))
        // 健康检查
        .route("/api/health", get(health))
        .with_state(state)
}

async fn list_devices(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<Device>>> {
    let rows = sqlx::query_as::<_, Device>(
        "SELECT id, name, status, lamp, mode, last_seen_at, created_at FROM device ORDER BY created_at",
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
}

async fn create_device(
    State(s): State<Arc<AppState>>,
    Json(body): Json<CreateDevice>,
) -> ApiResult<StatusCode> {
    sqlx::query("INSERT INTO device (id, name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(&body.id)
        .bind(body.name.unwrap_or_else(|| body.id.clone()))
        .execute(&s.db)
        .await
        .map_err(err500)?;
    Ok(StatusCode::CREATED)
}

async fn delete_device(State(s): State<Arc<AppState>>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    for table in ["device", "config", "lux_record", "alarm", "command_record"] {
        let col = if table == "device" { "id" } else { "device_id" };
        sqlx::query(&format!("DELETE FROM {table} WHERE {col} = $1"))
            .bind(&id)
            .execute(&s.db)
            .await
            .map_err(err500)?;
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
    hub.control_led(&id, led).await.map_err(err500)?;

    // 写审计日志
    let _ = sqlx::query(
        "INSERT INTO command_record (device_id, command_type, source, status, message) \
         VALUES ($1, $2, 'manual', 'sent', $3)",
    )
    .bind(&id)
    .bind(body.action.as_str())
    .bind(format!("IoTDA command: {led}"))
    .execute(&s.db)
    .await;

    Ok(StatusCode::ACCEPTED)
}

async fn get_threshold(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let row: Option<(f32,)> = sqlx::query_as("SELECT threshold FROM config WHERE device_id = $1")
        .bind(&id)
        .fetch_optional(&s.db)
        .await
        .map_err(err500)?;
    let threshold = row.map(|r| r.0).unwrap_or(40.0);
    Ok(Json(serde_json::json!({ "device_id": id, "threshold": threshold })))
}

#[derive(Deserialize)]
struct ThresholdBody {
    threshold: f32,
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
        "SELECT id, device_id, type, created_at, resolved_at FROM alarm WHERE 1=1",
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

// ============================================================
// 健康检查
// ============================================================
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

// ============================================================
// 登录
// ============================================================
async fn login(
    State(s): State<Arc<AppState>>,
    Json(body): Json<auth::LoginRequest>,
) -> ApiResult<Json<auth::LoginResponse>> {
    let row: Option<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.username, u.real_name, u.password_hash, r.role_code, r.role_name \
         FROM users u JOIN role r ON u.role_id = r.id WHERE u.username = $1 AND u.status = 1",
    )
    .bind(&body.username)
    .fetch_optional(&s.db)
    .await
    .map_err(err500)?;

    let (id, username, real_name, password_hash, role_code, role_name) =
        row.ok_or_else(|| (StatusCode::UNAUTHORIZED, "用户名或密码错误".to_string()))?;

    let valid = auth::verify_password(&body.password, &password_hash)
        .map_err(err500)?;
    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "用户名或密码错误".to_string()));
    }

    let claims = auth::create_claims(id, username.clone(), role_code.clone());
    let token = auth::generate_token(&claims, &s.jwt_secret).map_err(err500)?;

    Ok(Json(auth::LoginResponse {
        token,
        user: auth::UserInfo {
            id,
            username,
            real_name,
            role_code,
            role_name,
        },
    }))
}

// ============================================================
// 从请求头提取当前用户
// ============================================================
fn extract_user(headers: &HeaderMap, secret: &str) -> Result<auth::Claims, (StatusCode, String)> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "未登录".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "token 格式错误".to_string()))?;

    auth::validate_token(token, secret)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))
}

// ============================================================
// 账号列表
// ============================================================
#[derive(serde::Serialize, sqlx::FromRow)]
struct UserRow {
    id: i64,
    username: String,
    real_name: String,
    role_id: i64,
    role_code: String,
    role_name: String,
    status: i16,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_users(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<UserRow>>> {
    let _claims = extract_user(&headers, &s.jwt_secret)?;
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.username, u.real_name, u.role_id, r.role_code, r.role_name, u.status, u.created_at \
         FROM users u JOIN role r ON u.role_id = r.id ORDER BY u.id",
    )
    .fetch_all(&s.db)
    .await
    .map_err(err500)?;
    Ok(Json(rows))
}

// ============================================================
// 创建账号
// ============================================================
#[derive(Deserialize)]
struct CreateUser {
    username: String,
    password: String,
    real_name: Option<String>,
    role_id: i64,
}

async fn create_user(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateUser>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let _claims = extract_user(&headers, &s.jwt_secret)?;
    if body.password.len() < 6 {
        return Err(bad_req("密码至少6位"));
    }
    let hash = auth::hash_password(&body.password).map_err(err500)?;
    let real_name = body.real_name.unwrap_or_default();
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO users (username, password_hash, real_name, role_id) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(&body.username)
    .bind(&hash)
    .bind(&real_name)
    .bind(body.role_id)
    .fetch_one(&s.db)
    .await
    .map_err(err500)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": row.0, "username": body.username }))))
}

// ============================================================
// 删除账号
// ============================================================
async fn delete_user(
    State(s): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let _claims = extract_user(&headers, &s.jwt_secret)?;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await
        .map_err(err500)?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// 角色列表
// ============================================================
#[derive(serde::Serialize, sqlx::FromRow)]
struct RoleRow {
    id: i64,
    role_code: String,
    role_name: String,
    description: String,
}

async fn list_roles(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<RoleRow>>> {
    let rows = sqlx::query_as::<_, RoleRow>(
        "SELECT id, role_code, role_name, description FROM role ORDER BY id",
    )
    .fetch_all(&s.db)
    .await
    .map_err(err500)?;
    Ok(Json(rows))
}

// ============================================================
// 告警处理
// ============================================================
async fn resolve_alarm(
    State(s): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        "UPDATE alarm SET resolved_at = now() WHERE id = $1 AND resolved_at IS NULL",
    )
    .bind(id)
    .execute(&s.db)
    .await
    .map_err(err500)?;
    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "告警不存在或已处理".to_string()));
    }
    Ok(Json(serde_json::json!({ "resolved": id })))
}

async fn unresolve_alarm(
    State(s): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let result = sqlx::query(
        "UPDATE alarm SET resolved_at = NULL WHERE id = $1 AND resolved_at IS NOT NULL",
    )
    .bind(id)
    .execute(&s.db)
    .await
    .map_err(err500)?;
    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "告警不存在或未处理".to_string()));
    }
    Ok(Json(serde_json::json!({ "unresolved": id })))
}

// ============================================================
// 审计日志查询
// ============================================================
#[derive(serde::Serialize, sqlx::FromRow)]
struct CommandRecord {
    id: i64,
    device_id: String,
    command_type: String,
    source: String,
    status: String,
    message: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct CommandQuery {
    device_id: Option<String>,
    limit: Option<i64>,
}

async fn list_commands(
    State(s): State<Arc<AppState>>,
    Query(q): Query<CommandQuery>,
) -> ApiResult<Json<Vec<CommandRecord>>> {
    let limit = q.limit.unwrap_or(50).min(1000);
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, device_id, command_type, source, status, message, created_at \
         FROM command_record WHERE 1=1",
    );
    if let Some(d) = q.device_id {
        qb.push(" AND device_id = ").push_bind(d);
    }
    qb.push(" ORDER BY created_at DESC LIMIT ").push(limit);
    let rows = qb
        .build_query_as::<CommandRecord>()
        .fetch_all(&s.db)
        .await
        .map_err(err500)?;
    Ok(Json(rows))
}

// ============================================================
// 智能问答（从 Python 后端移植：意图识别 + 实体抽取 + 业务数据查询）
// ============================================================
#[derive(Deserialize)]
struct AskRequest {
    question: String,
}

#[derive(sqlx::FromRow)]
struct Knowledge {
    keyword: String,
    category: String,
    cause: String,
    suggestion: String,
}

/// 意图词典：每种意图对应一组关键词
struct IntentDef(&'static str, &'static [&'static str]);

const INTENTS: &[IntentDef] = &[
    IntentDef("query_alarm",     &["告警", "报警", "离线", "故障", "异常"]),
    IntentDef("query_threshold", &["阈值", "光照阈值", "参数", "配置", "下限", "上限"]),
    IntentDef("query_luminance", &["光照", "亮度", "照度", "光照强度", "lux"]),
    IntentDef("query_device",    &["设备", "在线", "状态", "路灯", "灯"]),
    IntentDef("query_command",   &["指令", "开关", "控制记录", "操作记录", "记录"]),
    IntentDef("advice",          &["怎么", "如何", "为什么", "原因", "建议", "维修", "维护", "处理", "解决", "排查", "频繁", "温度", "抖"]),
];

/// 意图识别：关键词加权（长词得分高），取最高分
fn classify_intent(question: &str) -> &'static str {
    let q = question.to_lowercase();
    let mut best = "fallback";
    let mut best_score = 0usize;
    for IntentDef(intent, kws) in INTENTS {
        let score: usize = kws.iter().filter(|kw| q.contains(*kw)).map(|kw| kw.len()).sum();
        if score > best_score {
            best = intent;
            best_score = score;
        }
    }
    best
}

/// 时间窗口解析："最近3天"、"最近2小时" 等
fn parse_window(question: &str) -> (chrono::Duration, String) {
    use regex::Regex;
    let re = Regex::new(r"最近\s*(\d+)\s*(天|日|小时|分钟|周)").unwrap();
    if let Some(caps) = re.captures(question) {
        let n: i64 = caps[1].parse().unwrap_or(7);
        let unit = &caps[2];
        let (dur, label) = match unit {
            "天" | "日" => (chrono::Duration::days(n), format!("最近{n}天")),
            "小时" => (chrono::Duration::hours(n), format!("最近{n}小时")),
            "分钟" => (chrono::Duration::minutes(n), format!("最近{n}分钟")),
            "周" => (chrono::Duration::weeks(n), format!("最近{n}周")),
            _ => (chrono::Duration::days(7), "最近7天".to_string()),
        };
        (dur, label)
    } else {
        (chrono::Duration::days(7), "最近7天".to_string())
    }
}

/// 设备实体抽取：匹配 device_id / name / "N号灯" / "灯N"
async fn resolve_device(db: &sqlx::PgPool, question: &str) -> Option<String> {
    #[derive(sqlx::FromRow)]
    struct Dev { device_id: String, name: String }
    let devices = sqlx::query_as::<_, Dev>("SELECT device_id, name FROM device ORDER BY id")
        .fetch_all(db).await.ok()?;
    // 优先匹配 device_id 或 name 子串
    for d in &devices {
        if !d.device_id.is_empty() && question.contains(&d.device_id) {
            return Some(d.device_id.clone());
        }
        if !d.name.is_empty() && question.contains(&d.name) {
            return Some(d.device_id.clone());
        }
    }
    // 匹配 "灯N号" / "N号灯" / "灯N"
    use regex::Regex;
    let re = Regex::new(r"灯\s*(\d+)\s*号|(\d+)\s*号\s*灯|灯\s*(\d+)")
        .ok()?;
    if let Some(caps) = re.captures(question) {
        let num = caps.get(1).or(caps.get(2)).or(caps.get(3))?;
        let num_str = num.as_str();
        for d in &devices {
            if d.device_id.contains(num_str) || d.name.contains(num_str) {
                return Some(d.device_id.clone());
            }
        }
    }
    None
}

/// 根据告警关键词匹配知识库维护建议
async fn advice_for_keywords(db: &sqlx::PgPool, texts: &[String]) -> String {
    let rows = sqlx::query_as::<_, Knowledge>(
        "SELECT keyword, category, cause, suggestion FROM maintenance_knowledge",
    ).fetch_all(db).await.unwrap_or_default();
    for e in &rows {
        if texts.iter().any(|t| t.contains(&e.keyword)) {
            return format!("【{}】原因：{}；建议：{}", e.keyword, e.cause, e.suggestion);
        }
    }
    String::new()
}

async fn assistant_ask(
    State(s): State<Arc<AppState>>,
    Json(body): Json<AskRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let q = body.question.trim();
    if q.is_empty() {
        return Err(bad_req("问题不能为空"));
    }

    let intent = classify_intent(q);
    let device_id = resolve_device(&s.db, q).await;
    let scope = match &device_id {
        Some(d) => format!("设备 {d}"),
        None => "全部设备".to_string(),
    };
    let (dur, desc) = parse_window(q);
    let since = chrono::Utc::now() - dur;

    let answer = match intent {
        // ── 告警查询 ──
        "query_alarm" => {
            let dev_filter = device_id.as_deref().unwrap_or("");
            let rows: Vec<(String, String, String, String, chrono::DateTime<chrono::Utc>)> =
                if dev_filter.is_empty() {
                    sqlx::query_as(
                        "SELECT device_id, type, COALESCE(type,''), COALESCE(resolved_at::text,''), created_at FROM alarm WHERE created_at >= $1 ORDER BY created_at DESC LIMIT 20"
                    ).bind(since).fetch_all(&s.db).await.unwrap_or_default()
                } else {
                    sqlx::query_as(
                        "SELECT device_id, type, COALESCE(type,''), COALESCE(resolved_at::text,''), created_at FROM alarm WHERE created_at >= $1 AND device_id = $2 ORDER BY created_at DESC LIMIT 20"
                    ).bind(since).bind(dev_filter).fetch_all(&s.db).await.unwrap_or_default()
                };
            if rows.is_empty() {
                format!("{desc}，{scope}没有告警记录。")
            } else {
                let unhandled = rows.iter().filter(|r| r.3.is_empty()).count();
                let mut lines = vec![format!("{desc}，{}共 {} 条告警，未处理 {} 条：", scope, rows.len(), unhandled)];
                for r in rows.iter().take(5) {
                    let tag = if r.3.is_empty() { "未处理" } else { "已处理" };
                    lines.push(format!("· {} {}（{}）{}", r.0, r.1, tag, r.4.format("%m-%d %H:%M")));
                }
                // 匹配维护建议
                let texts: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
                let adv = advice_for_keywords(&s.db, &texts).await;
                if !adv.is_empty() {
                    lines.push(format!("维护建议：{adv}"));
                }
                lines.join("\n")
            }
        }

        // ── 光照查询 ──
        "query_luminance" => {
            let (dur2, desc2) = if desc == "最近7天" {
                (chrono::Duration::days(1), "最近1天".to_string())
            } else {
                (dur, desc.clone())
            };
            let since2 = chrono::Utc::now() - dur2;
            let dev_filter = device_id.as_deref().unwrap_or("");
            #[derive(sqlx::FromRow)]
            struct LuxStats { c: i64, mn: f32, mx: f32, av: f64 }
            let stats = if dev_filter.is_empty() {
                sqlx::query_as::<_, LuxStats>(
                    "SELECT COUNT(*) as c, MIN(lux) as mn, MAX(lux) as mx, AVG(lux) as av FROM lux_record WHERE created_at >= $1"
                ).bind(since2).fetch_optional(&s.db).await.unwrap_or(None)
            } else {
                sqlx::query_as::<_, LuxStats>(
                    "SELECT COUNT(*) as c, MIN(lux) as mn, MAX(lux) as mx, AVG(lux) as av FROM lux_record WHERE created_at >= $1 AND device_id = $2"
                ).bind(since2).bind(dev_filter).fetch_optional(&s.db).await.unwrap_or(None)
            };
            match stats {
                Some(st) if st.c > 0 => {
                    // 最新一条
                    let latest: Option<(f32,)> = if dev_filter.is_empty() {
                        sqlx::query_as("SELECT lux FROM lux_record WHERE created_at >= $1 ORDER BY id DESC LIMIT 1")
                            .bind(since2).fetch_optional(&s.db).await.unwrap_or(None)
                    } else {
                        sqlx::query_as("SELECT lux FROM lux_record WHERE created_at >= $1 AND device_id = $2 ORDER BY id DESC LIMIT 1")
                            .bind(since2).bind(dev_filter).fetch_optional(&s.db).await.unwrap_or(None)
                    };
                    let cur = latest.map(|l| l.0).unwrap_or(0.0);
                    format!(
                        "{}，{}共上报 {} 条光照数据：当前 {:.0} lux，最低 {:.0}，最高 {:.0}，平均 {:.1}。",
                        desc2, scope, st.c, cur, st.mn, st.mx, st.av
                    )
                }
                _ => format!("{}，{}没有光照数据。", desc2, scope),
            }
        }

        // ── 设备查询 ──
        "query_device" => {
            #[derive(sqlx::FromRow)]
            struct DevInfo {
                device_id: String,
                name: String,
                status: String,
                lamp: String,
                last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
            }
            let rows = sqlx::query_as::<_, DevInfo>(
                "SELECT device_id, name, status, lamp, last_seen_at FROM device ORDER BY id"
            ).fetch_all(&s.db).await.unwrap_or_default();
            if rows.is_empty() {
                "目前还没有接入任何路灯设备。".to_string()
            } else {
                let on = rows.iter().filter(|r| r.status == "online").count();
                let mut lines = vec![format!("共 {} 台设备，在线 {} 台：", rows.len(), on)];
                for r in &rows {
                    let hb = r.last_seen_at.map(|t| t.format("%m-%d %H:%M").to_string()).unwrap_or_else(|| "从未心跳".to_string());
                    let st = if r.status == "online" { "在线" } else { "离线" };
                    let lp = if r.lamp == "on" { "开" } else { "关" };
                    lines.push(format!("· {}（{}）：{}，灯{}，最后心跳 {}", r.device_id, r.name, st, lp, hb));
                }
                lines.join("\n")
            }
        }

        // ── 阈值查询 ──
        "query_threshold" => {
            #[derive(sqlx::FromRow)]
            struct ThresholdRow { device_id: String, threshold: f32 }
            let rows = if let Some(ref did) = device_id {
                sqlx::query_as::<_, ThresholdRow>(
                    "SELECT device_id, threshold FROM config WHERE device_id = $1"
                ).bind(did).fetch_all(&s.db).await.unwrap_or_default()
            } else {
                sqlx::query_as::<_, ThresholdRow>(
                    "SELECT device_id, threshold FROM config"
                ).fetch_all(&s.db).await.unwrap_or_default()
            };
            if rows.is_empty() {
                "还没有为设备配置光照阈值（默认 40 lux）。".to_string()
            } else {
                rows.iter()
                    .map(|r| format!("· {}：阈值 {:.0} lux", r.device_id, r.threshold))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }

        // ── 指令查询 ──
        "query_command" => {
            let dev_filter = device_id.as_deref().unwrap_or("");
            #[derive(sqlx::FromRow)]
            struct CmdRow {
                device_id: String,
                command_type: String,
                source: String,
                created_at: chrono::DateTime<chrono::Utc>,
            }
            let rows = if dev_filter.is_empty() {
                sqlx::query_as::<_, CmdRow>(
                    "SELECT device_id, command_type, source, created_at FROM command_record WHERE created_at >= $1 ORDER BY created_at DESC LIMIT 20"
                ).bind(since).fetch_all(&s.db).await.unwrap_or_default()
            } else {
                sqlx::query_as::<_, CmdRow>(
                    "SELECT device_id, command_type, source, created_at FROM command_record WHERE created_at >= $1 AND device_id = $2 ORDER BY created_at DESC LIMIT 20"
                ).bind(since).bind(dev_filter).fetch_all(&s.db).await.unwrap_or_default()
            };
            if rows.is_empty() {
                format!("{desc}，{scope}没有控制指令记录。")
            } else {
                let auto = rows.iter().filter(|r| r.source == "auto").count();
                let mut lines = vec![format!("{}，{}共 {} 条指令（自动 {} / 手动 {}）：",
                    desc, scope, rows.len(), auto, rows.len() - auto)];
                for r in rows.iter().take(5) {
                    let act = if r.command_type == "on" { "开灯" } else { "关灯" };
                    lines.push(format!("· {} {}（{}）{}", r.device_id, act, r.source, r.created_at.format("%m-%d %H:%M")));
                }
                lines.join("\n")
            }
        }

        // ── 维护建议 ──
        "advice" => {
            // 先从知识库匹配
            let rows = sqlx::query_as::<_, Knowledge>(
                "SELECT keyword, category, cause, suggestion FROM maintenance_knowledge",
            ).fetch_all(&s.db).await.unwrap_or_default();
            for e in &rows {
                if q.contains(&e.keyword) {
                    return Ok(Json(serde_json::json!({
                        "question": q,
                        "answer": format!("【{}】原因：{}；建议：{}", e.keyword, e.cause, e.suggestion)
                    })));
                }
            }
            // 没直接命中，结合最近告警给建议
            #[derive(sqlx::FromRow)]
            struct AlarmMsg { alarm_type: String, message: Option<String> }
            let recent = sqlx::query_as::<_, AlarmMsg>(
                "SELECT type as alarm_type, type as message FROM alarm ORDER BY id DESC LIMIT 20"
            ).fetch_all(&s.db).await.unwrap_or_default();
            if !recent.is_empty() {
                let texts: Vec<String> = recent.iter().map(|r| r.alarm_type.clone()).collect();
                let adv = advice_for_keywords(&s.db, &texts).await;
                if !adv.is_empty() {
                    return Ok(Json(serde_json::json!({
                        "question": q,
                        "answer": format!("结合最近的告警：{adv}")
                    })));
                }
            }
            "请告诉我具体故障现象。知识库覆盖：设备离线、光照异常、灯不亮、灯闪烁、传感器异常。".to_string()
        }

        // ── 兜底 ──
        _ => {
            "我还不太明白你的问题。你可以这样问我：\n\
             · 最近7天有哪些告警？\n\
             · 设备现在在线还是离线？\n\
             · 光照阈值是多少？\n\
             · 最近的光照数据怎么样？\n\
             · 路灯频繁开关怎么办？".to_string()
        }
    };

    Ok(Json(serde_json::json!({
        "question": q,
        "answer": answer
    })))
}
