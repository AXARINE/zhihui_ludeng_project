//! 账号 / 登录 / RBAC:JWT + Argon2id + role/permission 权限检查。
//!
//! 中间件 `auth_middleware` 只做"你是谁"(认证),路由级的"你能不能做"由各 handler
//! 调用 `Auth::require()` 完成(权限码与种子数据见 `migrations/0002_rbac.sql`)。

use crate::AppState;
use crate::api::Error;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::extract::Request;
use axum::extract::{ConnectInfo, FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use jsonwebtoken::{
    DecodingKey, EncodingKey, Header, Validation, decode, encode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use utoipa::ToSchema;

const TOKEN_TTL_SECS: i64 = 24 * 60 * 60;

/// 系统管理员角色代码(唯一硬编码的特殊角色:账号操作的越权守卫都围绕它,
/// 见 `guard_super_admin`;其角色权限固定不可改的保护在 `update_role_permissions`)
pub const SUPER_ADMIN: &str = "super_admin";

/// 用户状态缓存 TTL:JWT 验签后的账号活性(存在/启用/当前角色)校验缓存,
/// 禁用/删除/降权最迟在这个窗口内对已签发 token 生效
const USER_CACHE_TTL: Duration = Duration::from_secs(30);

/// 角色权限缓存 TTL:命中未过期免 SQL;过期重查回填,
/// 兜底"经 nocodb/直接改库而不走 API"导致的缓存失效盲区(单实例部署前提不变)
const PERM_CACHE_TTL: Duration = Duration::from_secs(60);

/// 进程内缓存条目新鲜度判断(`user_cache` / `perm_cache` 共用)
pub fn cache_entry_fresh(fetched_at: Instant, ttl: Duration) -> bool {
    fetched_at.elapsed() < ttl
}

/// 用户状态缓存条目:JWT 验签通过后,授权以这里的**数据库当前值**为准,
/// 而不是 token 签发时刻的 role 声明
#[derive(Debug, Clone)]
pub struct UserCacheEntry {
    pub status: i16,
    pub role_id: i64,
    pub role_code: String,
    pub fetched_at: Instant,
}

/// 用户状态缓存:`user_id` → 条目(30s TTL;`update_user`/`delete_user` 提交后主动失效)
pub type UserCache = Arc<std::sync::RwLock<HashMap<i64, UserCacheEntry>>>;

/// 登录时序抹平用的伪哈希:用户不存在时也拿它跑一次 Argon2 校验,
/// 避免"用户名是否存在"从响应时间差泄露。首次登录请求时生成(一次性 ~几十 ms)
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("dummy-password-for-timing-equalization")
        .expect("生成 dummy hash 失败")
});

/// Argon2id 默认参数(OWASP 推荐档):m=19456KiB, t=2, p=1。
/// 部署时可用 `ARGON2_M_COST_KIB` / `ARGON2_T` / `ARGON2_P` 下调(低配机权衡),
/// **已有密码哈希不受影响** —— 校验永远按 hash 串内嵌参数执行。
fn argon2_params() -> Params {
    let env_u32 = |key: &str, default: u32| {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let m = env_u32("ARGON2_M_COST_KIB", 19_456);
    let t = env_u32("ARGON2_T", 2);
    let p = env_u32("ARGON2_P", 1);
    Params::new(m, t, p, None).expect("合法 Argon2 参数")
}

/// 登录限流:滑动窗口,同一来源 IP 每 60 秒最多 `per_minute` 次尝试。
/// `per_minute <= 0` 表示不限流。防登录风暴打爆 Argon2 内存(见 perf 报告 F2)。
pub struct LoginLimiter {
    inner: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    per_minute: usize,
}

impl LoginLimiter {
    pub fn new(per_minute: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            per_minute,
        }
    }

    /// 记录一次尝试并裁决:窗口内次数未超限放行并计数,超限拒绝(不计数)
    pub async fn try_acquire(&self, ip: IpAddr) -> bool {
        if self.per_minute == 0 {
            return true;
        }
        let now = Instant::now();
        {
            // 锁只覆盖计数区段,计数完成后立刻释放(锁守卫带显著 Drop)
            let mut guard = self.inner.lock().await;
            let window = guard.entry(ip).or_default();
            while window.front().is_some_and(|t| {
                now.duration_since(*t) > Duration::from_secs(60)
            }) {
                window.pop_front();
            }
            if window.len() >= self.per_minute {
                return false;
            }
            window.push_back(now);
            drop(guard);
        }
        true
    }
}

/// 中间件认证成功后塞进 `Request` extensions 的身份,handler 通过 extractor 取用
#[derive(Debug, Clone)]
pub struct Auth {
    pub user_id: i64,
    pub role_id: i64,
    pub role_code: String,
}

impl Auth {
    /// 检查当前角色是否拥有指定权限码。
    ///
    /// RBAC 映射运行时可改,变更只经由 `update_role_permissions`(提交后失效对应条目),
    /// 故按 `role_id` 做进程内缓存,命中且未过 `PERM_CACHE_TTL`(60s)即免一次 SQL;
    /// TTL 兜底"绕过 API 直接改库"的场景(单实例部署前提,多副本需改 TTL 方案)。
    pub async fn require(
        &self,
        state: &AppState,
        perm: &str,
    ) -> Result<(), Error> {
        // 锁内不 await:取到 Arc 即放锁,std::RwLock 不会跨异步点持有
        let cached = state
            .perm_cache
            .read()
            .expect("perm cache lock poisoned")
            .get(&self.role_id)
            .cloned();
        let perms = match cached {
            Some((perms, fetched_at))
                if cache_entry_fresh(fetched_at, PERM_CACHE_TTL) =>
            {
                perms
            }
            _ => {
                let fresh: Arc<HashSet<String>> = Arc::new(
                    sqlx::query_scalar::<_, String>(
                        "SELECT p.perm_code FROM permission p \
                         JOIN role_permission rp ON rp.permission_id = p.id \
                         WHERE rp.role_id = $1",
                    )
                    .bind(self.role_id)
                    .fetch_all(&state.db)
                    .await?
                    .into_iter()
                    .collect(),
                );
                state
                    .perm_cache
                    .write()
                    .expect("perm cache lock poisoned")
                    .insert(self.role_id, (Arc::clone(&fresh), Instant::now()));
                fresh
            }
        };
        if perms.contains(perm) {
            Ok(())
        } else {
            Err(Error::Forbidden(format!(
                "当前角色({})没有权限 {perm}",
                self.role_code
            )))
        }
    }
}

impl<S> FromRequestParts<S> for Auth
where
    S: Send + Sync,
{
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(parts.extensions.get::<Self>().cloned().ok_or_else(
            || (StatusCode::UNAUTHORIZED, "未登录或登录已过期").into_response(),
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    user_id: i64,
    username: String,
    // 兼容已签发 token 的字段;授权不再以它们为准——中间件验签后会从
    // 数据库(经 30s 缓存)取当前角色构造 `Auth`,降权/禁用对已签发 token 生效
    role_id: i64,
    role_code: String,
    exp: usize,
}

fn decode_token(
    state: &AppState,
    token: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
}

/// 全局认证中间件:非公开路径必须带 `Authorization: Bearer <token>`。
/// 验签通过后还要查账号活性(30s 进程内缓存):被禁用/删除的账号最迟 30s 内
/// 被踢下线;`Auth` 里的角色用数据库当前值,不用 token 签发时的声明——
/// 降权/改角色对已签发 token 同样生效。
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if is_public(req.uri().path(), req.method()) {
        return next.run(req).await;
    }
    let Some(token) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty())
    else {
        return (
            StatusCode::UNAUTHORIZED,
            "缺少 Authorization: Bearer <token>",
        )
            .into_response();
    };
    let Ok(claims) = decode_token(&state, token) else {
        return (StatusCode::UNAUTHORIZED, "token 无效或已过期")
            .into_response();
    };
    match load_user_state(&state, claims.user_id).await {
        Ok(Some(u)) if u.status == 1 => {
            req.extensions_mut().insert(Auth {
                user_id: claims.user_id,
                role_id: u.role_id,
                role_code: u.role_code,
            });
            next.run(req).await
        }
        Ok(_) => {
            (StatusCode::UNAUTHORIZED, "账号不存在或已被禁用").into_response()
        }
        Err(e) => {
            tracing::error!("用户状态查询失败: {e}");
            (StatusCode::SERVICE_UNAVAILABLE, "服务暂时不可用,请稍后重试")
                .into_response()
        }
    }
}

/// 查用户活性(缓存 30s):None = 用户不存在。不缓存"不存在"(负缓存):
/// 构造合法签名 token 需要先拿到 JWT 密钥,被删账号 token 的偶发回源查询可接受
async fn load_user_state(
    state: &AppState,
    user_id: i64,
) -> Result<Option<UserCacheEntry>, Error> {
    let cached = state
        .user_cache
        .read()
        .expect("user cache lock poisoned")
        .get(&user_id)
        .cloned();
    if let Some(entry) =
        cached.filter(|e| cache_entry_fresh(e.fetched_at, USER_CACHE_TTL))
    {
        return Ok(Some(entry));
    }
    let row: Option<(i16, i64, String)> = sqlx::query_as(
        "SELECT u.status, u.role_id, r.role_code \
         FROM app_user u JOIN role r ON r.id = u.role_id WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    let entry = row.map(|(status, role_id, role_code)| UserCacheEntry {
        status,
        role_id,
        role_code,
        fetched_at: Instant::now(),
    });
    {
        let mut guard =
            state.user_cache.write().expect("user cache lock poisoned");
        match &entry {
            Some(e) => {
                guard.insert(user_id, e.clone());
            }
            // 用户已不存在:清掉可能残留的旧条目
            None => {
                guard.remove(&user_id);
            }
        }
    }
    Ok(entry)
}

/// 用户资料/状态变更后主动失效其缓存条目(`update_user`/`delete_user` 调用)
fn invalidate_user_cache(state: &AppState, user_id: i64) {
    state
        .user_cache
        .write()
        .expect("user cache lock poisoned")
        .remove(&user_id);
}

pub fn is_public(path: &str, method: &Method) -> bool {
    method == Method::OPTIONS
        || path == "/api/health"
        || path == "/api/auth/login"
        || path == "/docs"
        || path.starts_with("/docs/")
        || path == "/api/openapi.json"
        // IoTDA 数据转发推送入口:免 JWT 无认证(见 webhook.rs 文件头说明)
        || path == "/api/iotda/callback"
}

// ---------------- 密码哈希 ----------------
pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params())
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::Internal(format!("密码哈希失败: {e}")))
}

pub fn verify_password(password: &str, hashed: &str) -> bool {
    PasswordHash::new(hashed).is_ok_and(|parsed| {
        // argon2 0.5 约定:verify 按 hash 串内嵌的算法/版本/参数执行,
        // Argon2 实例上的配置不参与 —— 参数调整后老密码哈希依然可用
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// 异步包装:Argon2id 是 CPU/内存密集操作(默认参数约几十毫秒、19MiB 工作内存),
/// 移到阻塞线程池执行;`sem` 并发闸限制同时进行的哈希数,
/// 防止登录风暴把内存打爆(压测发现:64~256 并发登录 RSS 飙到 8~12GiB,见 perf 报告 F2)
pub async fn hash_password_async(
    password: String,
    sem: &Semaphore,
) -> Result<String, Error> {
    let _permit = sem
        .acquire()
        .await
        .map_err(|_| Error::Internal("密码哈希并发闸已关闭".into()))?;
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| Error::Internal(format!("密码哈希任务异常: {e}")))?
}

pub async fn verify_password_async(
    password: String,
    hashed: String,
    sem: &Semaphore,
) -> Result<bool, Error> {
    let _permit = sem
        .acquire()
        .await
        .map_err(|_| Error::Internal("密码校验并发闸已关闭".into()))?;
    tokio::task::spawn_blocking(move || verify_password(&password, &hashed))
        .await
        .map_err(|e| Error::Internal(format!("密码校验任务异常: {e}")))
}

// ---------------- 越权守卫 / 密码策略 ----------------
/// 越权守卫:只有 `super_admin` 本人能操作 `super_admin` 账号、或把任何人(含自己)
/// 改成 `super_admin` 角色。硬编码单一特殊角色,保持规则直白(0004 迁移的设计意图)
pub fn guard_super_admin(
    caller_role: &str,
    target_now_super: bool,
    assigning_super: bool,
) -> Result<(), Error> {
    if caller_role == SUPER_ADMIN {
        return Ok(());
    }
    if target_now_super {
        return Err(Error::Forbidden(
            "无权操作系统管理员(super_admin)账号".into(),
        ));
    }
    if assigning_super {
        return Err(Error::Forbidden(
            "无权授予系统管理员(super_admin)角色".into(),
        ));
    }
    Ok(())
}

/// 启用状态的 `super_admin` 账号数(防锁死守卫用:动最后一个之前必须还有别人)
async fn enabled_super_admin_count(db: &PgPool) -> Result<i64, Error> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM app_user u JOIN role r ON r.id = u.role_id \
         WHERE r.role_code = 'super_admin' AND u.status = 1",
    )
    .fetch_one(db)
    .await?)
}

/// 密码策略:8~64 字符,至少一个 ASCII 字母和一个数字
pub fn validate_password(p: &str) -> Result<(), Error> {
    if p.len() < 8 || p.len() > 64 {
        return Err(Error::BadRequest("密码长度需在 8~64 之间".into()));
    }
    if !p.bytes().any(|b| b.is_ascii_alphabetic())
        || !p.bytes().any(|b| b.is_ascii_digit())
    {
        return Err(Error::BadRequest(
            "密码需至少包含一个字母和一个数字".into(),
        ));
    }
    Ok(())
}

// ---------------- 输出模型 ----------------
#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct RoleOut {
    pub id: i64,
    pub role_code: String,
    pub role_name: String,
    pub description: String,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct PermissionOut {
    pub id: i64,
    pub perm_code: String,
    pub perm_name: String,
    pub module: String,
    pub description: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserOut {
    pub id: i64,
    pub username: String,
    pub real_name: String,
    pub role_id: i64,
    pub role_code: String,
    pub role_name: String,
    pub status: i16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i64,
    username: String,
    password_hash: String,
    real_name: String,
    role_id: i64,
    status: i16,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    role_code: String,
    role_name: String,
}

impl From<UserRow> for UserOut {
    fn from(r: UserRow) -> Self {
        Self {
            id: r.id,
            username: r.username,
            real_name: r.real_name,
            role_id: r.role_id,
            role_code: r.role_code,
            role_name: r.role_name,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LoginOut {
    pub token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserOut,
    pub role: RoleOut,
    pub permissions: Vec<String>,
}

// ---------------- 请求模型 ----------------
#[derive(Deserialize, ToSchema)]
pub struct LoginIn {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateUserIn {
    pub username: String,
    pub password: String,
    pub real_name: String,
    pub role_id: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct RolePermissionsIn {
    pub permission_ids: Vec<i64>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUserIn {
    pub username: Option<String>,
    pub real_name: Option<String>,
    pub password: Option<String>,
    pub role_id: Option<i64>,
    pub status: Option<i16>,
}

// ---------------- 路由 ----------------
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", get(me))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", patch(update_user).delete(delete_user))
        .route("/api/roles", get(list_roles))
        .route("/api/permissions", get(list_permissions))
        .route(
            "/api/roles/{id}/permissions",
            get(get_role_permissions).put(update_role_permissions),
        )
        .with_state(state)
}

// ---------------- 登录 ----------------
#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginIn,
    responses(
        (status = 200, description = "登录成功", body = LoginOut),
        (status = 401, description = "用户名或密码错误"),
        (status = 429, description = "尝试过于频繁,请稍后再试")
    )
)]
async fn login(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginIn>,
) -> Result<Json<LoginOut>, Error> {
    // 限流前置在 Argon2 校验之前:登录风暴是本服务最大的内存风险(见 perf 报告 F2)
    if !s.login_limiter.try_acquire(addr.ip()).await {
        return Err(Error::RateLimited("尝试过于频繁,请稍后再试".into()));
    }
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.username, u.password_hash, u.real_name, u.role_id, u.status, \
                u.created_at, u.updated_at, r.role_code, r.role_name \
         FROM app_user u JOIN role r ON r.id = u.role_id \
         WHERE u.username = $1",
    )
    .bind(body.username.trim())
    .fetch_optional(&s.db)
    .await?;

    // 时序抹平:用户不存在/被禁用也执行一次同样的 Argon2 校验,
    // 否则"用户名是否存在"能从响应时间差(差一次 Argon2 的耗时)被探测出来
    let hash = user
        .as_ref()
        .map_or_else(|| DUMMY_HASH.clone(), |u| u.password_hash.clone());
    let verified =
        verify_password_async(body.password, hash, &s.argon2_sem).await?;
    let Some(user) = user.filter(|u| verified && u.status == 1) else {
        return Err(Error::Unauthorized("用户名或密码错误".into()));
    };

    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT p.perm_code FROM permission p \
         JOIN role_permission rp ON rp.permission_id = p.id \
         WHERE rp.role_id = $1 ORDER BY p.id",
    )
    .bind(user.role_id)
    .fetch_all(&s.db)
    .await?;

    let exp = Utc::now().timestamp() + TOKEN_TTL_SECS;
    let exp = usize::try_from(exp)
        .map_err(|_| Error::Internal("token 过期时间溢出".into()))?;
    let claims = Claims {
        user_id: user.id,
        username: user.username.clone(),
        role_id: user.role_id,
        role_code: user.role_code.clone(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(s.jwt_secret.as_bytes()),
    )
    .map_err(|e| Error::Internal(format!("token 生成失败: {e}")))?;

    let role = RoleOut {
        id: user.role_id,
        role_code: user.role_code.clone(),
        role_name: user.role_name.clone(),
        description: String::new(),
    };
    Ok(Json(LoginOut {
        token,
        token_type: "Bearer".into(),
        expires_in: TOKEN_TTL_SECS,
        user: user.into(),
        role,
        permissions,
    }))
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "当前登录用户", body = UserOut),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
async fn me(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<UserOut>, Error> {
    let user = fetch_user_by_id(&s.db, auth.user_id)
        .await?
        .ok_or_else(|| Error::Unauthorized("账号不存在或已被删除".into()))?;
    Ok(Json(user))
}

// ---------------- 用户管理 ----------------
#[utoipa::path(
    get,
    path = "/api/users",
    responses((status = 200, description = "账号列表", body = Vec<UserOut>)),
    security(("bearer_auth" = []))
)]
async fn list_users(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<UserOut>>, Error> {
    auth.require(&s, "user:manage").await?;
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.username, u.password_hash, u.real_name, u.role_id, u.status, \
                u.created_at, u.updated_at, r.role_code, r.role_name \
         FROM app_user u JOIN role r ON r.id = u.role_id ORDER BY u.id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    post,
    path = "/api/users",
    request_body = CreateUserIn,
    responses(
        (status = 201, description = "创建成功", body = UserOut),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权限"),
        (status = 409, description = "用户名已存在")
    ),
    security(("bearer_auth" = []))
)]
async fn create_user(
    State(s): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateUserIn>,
) -> Result<(StatusCode, Json<UserOut>), Error> {
    auth.require(&s, "user:manage").await?;
    let username = body.username.trim().to_string();
    if username.is_empty() || username.len() > 64 {
        return Err(Error::BadRequest("用户名长度需在 1~64 之间".into()));
    }
    validate_password(&body.password)?;
    let new_role_code: Option<String> =
        sqlx::query_scalar("SELECT role_code FROM role WHERE id = $1")
            .bind(body.role_id)
            .fetch_optional(&s.db)
            .await?;
    let Some(new_role_code) = new_role_code else {
        return Err(Error::BadRequest("角色不存在".into()));
    };
    // 越权守卫:非 super_admin 不能创建 super_admin 账号
    guard_super_admin(&auth.role_code, false, new_role_code == SUPER_ADMIN)?;
    let hash = hash_password_async(body.password, &s.argon2_sem).await?;
    // 用户名唯一性交给 UNIQUE 约束裁决:预检查 + 插入之间存在 TOCTOU 竞态,
    // 并发重名注册应得到 409 而非 500
    let new_id: i64 = match sqlx::query_scalar(
        "INSERT INTO app_user (username, password_hash, real_name, role_id, status) \
         VALUES ($1, $2, $3, $4, 1) RETURNING id",
    )
    .bind(&username)
    .bind(hash)
    .bind(body.real_name.trim())
    .bind(body.role_id)
    .fetch_one(&s.db)
    .await
    {
        Ok(id) => id,
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(Error::Conflict("用户名已存在".into()));
        }
        Err(e) => return Err(e.into()),
    };
    crate::api::audit(&s.db, Some(auth.user_id), "user.create", &username, "")
        .await;
    let user = fetch_user_by_id(&s.db, new_id)
        .await?
        .ok_or_else(|| Error::Internal("新账号创建后查询失败".into()))?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[utoipa::path(
    delete,
    path = "/api/users/{id}",
    params(("id" = i64, Path, description = "账号 ID")),
    responses(
        (status = 200, description = "删除成功", body = UserOut),
        (status = 403, description = "无权限/不能删除自己"),
        (status = 404, description = "账号不存在")
    ),
    security(("bearer_auth" = []))
)]
async fn delete_user(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<i64>,
) -> Result<Json<UserOut>, Error> {
    auth.require(&s, "user:manage").await?;
    if auth.user_id == id {
        return Err(Error::Forbidden("不能删除当前登录账号".into()));
    }
    let user = fetch_user_by_id(&s.db, id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("账号 {id} 不存在")))?;
    // 越权守卫:非 super_admin 不能删除 super_admin 账号
    guard_super_admin(&auth.role_code, user.role_code == SUPER_ADMIN, false)?;
    // 防锁死:不能删除最后一个启用状态的 super_admin 账号
    if user.role_code == SUPER_ADMIN
        && user.status == 1
        && enabled_super_admin_count(&s.db).await? <= 1
    {
        return Err(Error::Forbidden(
            "至少保留一个启用状态的系统管理员账号".into(),
        ));
    }
    sqlx::query("DELETE FROM app_user WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    invalidate_user_cache(&s, id);
    crate::api::audit(&s.db, Some(auth.user_id), "user.delete", &user.username, "")
        .await;
    Ok(Json(user))
}

/// `update_user` 的守卫链:存在性 → 越权(`super_admin` 保护)→ 防锁死。
/// 全部通过则返回目标用户名(审计用)
async fn guard_user_update(
    s: &AppState,
    auth: &Auth,
    id: i64,
    body: &UpdateUserIn,
) -> Result<String, Error> {
    // 目标当前角色/状态/用户名(越权守卫、防锁死与审计都要用)
    let target: Option<(String, i16, String)> = sqlx::query_as(
        "SELECT r.role_code, u.status, u.username FROM app_user u \
         JOIN role r ON r.id = u.role_id WHERE u.id = $1",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    let Some((target_role, target_status, target_name)) = target else {
        return Err(Error::NotFound(format!("账号 {id} 不存在")));
    };
    // 守卫 A:目标是 super_admin 账号 → 只有 super_admin 本人能改
    // (堵"admin 直接改 superadmin 密码实现接管")
    guard_super_admin(&auth.role_code, target_role == SUPER_ADMIN, false)?;
    // 如果要改角色:验证角色存在 + 守卫 B:非 super_admin 不能把任何人改成 super_admin
    if let Some(rid) = body.role_id {
        let rc: Option<String> =
            sqlx::query_scalar("SELECT role_code FROM role WHERE id = $1")
                .bind(rid)
                .fetch_optional(&s.db)
                .await?;
        let Some(rc) = rc else {
            return Err(Error::BadRequest("角色不存在".into()));
        };
        let assigning_super = rc == SUPER_ADMIN;
        guard_super_admin(&auth.role_code, false, assigning_super)?;
        // 守卫 C(防锁死)之一:把启用中的 super_admin 改走
        if target_role == SUPER_ADMIN && target_status == 1 && !assigning_super
        {
            ensure_not_last_super_admin(s).await?;
        }
    }
    // 守卫 C(防锁死)之二:禁用启用中的 super_admin
    if target_role == SUPER_ADMIN && target_status == 1 && body.status == Some(0)
    {
        ensure_not_last_super_admin(s).await?;
    }
    Ok(target_name)
}

/// 防锁死:本次操作会让一个启用中的 `super_admin` 离开 → 必须还有其他启用状态的
async fn ensure_not_last_super_admin(s: &AppState) -> Result<(), Error> {
    if enabled_super_admin_count(&s.db).await? <= 1 {
        return Err(Error::Forbidden(
            "至少保留一个启用状态的系统管理员账号".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(
    patch,
    path = "/api/users/{id}",
    params(("id" = i64, Path, description = "账号 ID")),
    request_body = UpdateUserIn,
    responses(
        (status = 200, description = "更新成功", body = UserOut),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权限"),
        (status = 404, description = "账号不存在")
    ),
    security(("bearer_auth" = []))
)]
async fn update_user(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<i64>,
    Json(body): Json<UpdateUserIn>,
) -> Result<Json<UserOut>, Error> {
    auth.require(&s, "user:manage").await?;
    let target_name = guard_user_update(&s, &auth, id, &body).await?;
    // 如果要改密码，校验密码策略
    if let Some(pwd) = &body.password {
        validate_password(pwd)?;
    }
    // 如果要改用户名，验证长度和唯一性(预查是快路径,并发兜底靠下方 23505 映射)
    if let Some(uname) = &body.username {
        let uname = uname.trim();
        if uname.is_empty() || uname.len() > 64 {
            return Err(Error::BadRequest("用户名长度需在 1~64 之间".into()));
        }
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app_user WHERE username = $1 AND id != $2)",
        )
        .bind(uname)
        .bind(id)
        .fetch_one(&s.db)
        .await?;
        if dup {
            return Err(Error::Conflict("用户名已存在".into()));
        }
    }
    // 构建动态 UPDATE
    let mut qb = sqlx::QueryBuilder::new("UPDATE app_user SET ");
    let mut changed_fields: Vec<&str> = Vec::new();
    {
        let mut sep = qb.separated(", ");
        if let Some(uname) = body
            .username
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            sep.push("username = ").push_bind_unseparated(uname);
            changed_fields.push("username");
        }
        if let Some(name) = body
            .real_name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            sep.push("real_name = ").push_bind_unseparated(name);
            changed_fields.push("real_name");
        }
        if let Some(pwd) = &body.password {
            // 走异步包装 + 并发闸:同步版会卡住 tokio worker(perf 报告 F2 结论)
            let hash = hash_password_async(pwd.clone(), &s.argon2_sem).await?;
            sep.push("password_hash = ").push_bind_unseparated(hash);
            changed_fields.push("password");
        }
        if let Some(rid) = body.role_id {
            sep.push("role_id = ").push_bind_unseparated(rid);
            changed_fields.push("role_id");
        }
        if let Some(st) = body.status {
            sep.push("status = ").push_bind_unseparated(st);
            changed_fields.push("status");
        }
    }
    if changed_fields.is_empty() {
        return Err(Error::BadRequest("没有可更新的字段".into()));
    }
    qb.push(", updated_at = now() WHERE id = ").push_bind(id);
    // 用户名唯一性由 UNIQUE 约束兜底:预查与更新之间的并发重名应得到 409 而非 500
    if let Err(e) = qb.build().execute(&s.db).await {
        if let sqlx::Error::Database(dbe) = &e
            && dbe.code().as_deref() == Some("23505")
        {
            return Err(Error::Conflict("用户名已存在".into()));
        }
        return Err(e.into());
    }
    // 角色/状态可能已变:失效目标用户的活性缓存,最迟下一请求按新值生效
    invalidate_user_cache(&s, id);
    crate::api::audit(
        &s.db,
        Some(auth.user_id),
        "user.update",
        &target_name,
        &format!("变更字段: {}", changed_fields.join(", ")),
    )
    .await;
    let user = fetch_user_by_id(&s.db, id)
        .await?
        .ok_or_else(|| Error::Internal("更新后查询失败".into()))?;
    Ok(Json(user))
}

async fn fetch_user_by_id(
    db: &PgPool,
    id: i64,
) -> Result<Option<UserOut>, Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.username, u.password_hash, u.real_name, u.role_id, u.status, \
                u.created_at, u.updated_at, r.role_code, r.role_name \
         FROM app_user u JOIN role r ON r.id = u.role_id WHERE u.id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(Into::into))
}

// ---------------- 角色 / 权限 ----------------
#[utoipa::path(
    get,
    path = "/api/roles",
    responses((status = 200, description = "角色列表", body = Vec<RoleOut>)),
    security(("bearer_auth" = []))
)]
async fn list_roles(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<RoleOut>>, Error> {
    auth.require(&s, "user:manage").await?;
    let rows = sqlx::query_as::<_, RoleOut>(
        "SELECT id, role_code, role_name, description FROM role ORDER BY id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[utoipa::path(
    get,
    path = "/api/permissions",
    responses((status = 200, description = "权限列表", body = Vec<PermissionOut>)),
    security(("bearer_auth" = []))
)]
async fn list_permissions(
    State(s): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<PermissionOut>>, Error> {
    auth.require(&s, "user:manage").await?;
    let rows = sqlx::query_as::<_, PermissionOut>(
        "SELECT id, perm_code, perm_name, module, description FROM permission ORDER BY id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[utoipa::path(
    put,
    path = "/api/roles/{id}/permissions",
    params(("id" = i64, Path, description = "角色 ID")),
    request_body = RolePermissionsIn,
    responses(
        (status = 204, description = "权限映射已更新"),
        (status = 400, description = "参数错误"),
        (status = 403, description = "无权限"),
        (status = 404, description = "角色不存在")
    ),
    security(("bearer_auth" = []))
)]
async fn update_role_permissions(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<i64>,
    Json(body): Json<RolePermissionsIn>,
) -> Result<StatusCode, Error> {
    auth.require(&s, "role:manage").await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM role WHERE id = $1)")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
    if !exists {
        return Err(Error::NotFound(format!("角色 {id} 不存在")));
    }
    // 保护 1：系统管理员角色的权限不可修改（防止权限管理被锁死）
    let role_code: Option<String> =
        sqlx::query_scalar("SELECT role_code FROM role WHERE id = $1")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
    if role_code.as_deref() == Some("super_admin") {
        return Err(Error::Forbidden(
            "系统管理员角色的权限固定，不可修改".into(),
        ));
    }
    // 保护 2：不能移除"本角色"的角色权限管理权限（防止把自己锁死）
    if auth.role_id == id {
        let manage_id: i64 = sqlx::query_scalar(
            "SELECT id FROM permission WHERE perm_code = 'role:manage'",
        )
        .fetch_one(&s.db)
        .await?;
        if !body.permission_ids.contains(&manage_id) {
            return Err(Error::Forbidden(
                "不能移除本角色的「角色权限管理」权限（防止权限管理锁死）"
                    .into(),
            ));
        }
    }
    let valid = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM permission WHERE id = ANY($1)",
    )
    .bind(&body.permission_ids)
    .fetch_one(&s.db)
    .await?;
    let expected = i64::try_from(body.permission_ids.len())
        .map_err(|_| Error::BadRequest("权限 ID 列表过长".into()))?;
    if valid != expected {
        return Err(Error::BadRequest("包含不存在的权限 ID".into()));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM role_permission WHERE role_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // 批量插入:unnest 一次完成,避免 N 次 roundtrip
    sqlx::query(
        "INSERT INTO role_permission (role_id, permission_id) \
         SELECT $1, pid FROM unnest($2::bigint[]) AS pid \
         ON CONFLICT (role_id, permission_id) DO NOTHING",
    )
    .bind(id)
    .bind(&body.permission_ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    // RBAC 映射已变:失效该角色的权限缓存,下一请求重新加载
    s.perm_cache
        .write()
        .expect("perm cache lock poisoned")
        .remove(&id);
    crate::api::audit(
        &s.db,
        Some(auth.user_id),
        "role.perms_update",
        role_code.as_deref().unwrap_or("?"),
        &format!("permission_ids={:?}", body.permission_ids),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/roles/{id}/permissions —— 查询角色当前拥有的权限 ID 列表
#[utoipa::path(
    get,
    path = "/api/roles/{id}/permissions",
    params(("id" = i64, Path, description = "角色 ID")),
    responses(
        (status = 200, description = "当前权限 ID 列表", body = Vec<i64>),
        (status = 403, description = "无权限"),
        (status = 404, description = "角色不存在")
    ),
    security(("bearer_auth" = []))
)]
async fn get_role_permissions(
    State(s): State<AppState>,
    auth: Auth,
    Path(id): Path<i64>,
) -> Result<Json<Vec<i64>>, Error> {
    auth.require(&s, "role:manage").await?;
    let ids = sqlx::query_scalar::<_, i64>(
        "SELECT permission_id FROM role_permission WHERE role_id = $1",
    )
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(ids))
}

// ---------------- 首次启动的引导管理员 ----------------
/// 确保"系统管理员"(`super_admin`)与"路灯管理员"(`admin`)各有引导账号，账号密码可用环境变量覆盖。
/// 默认: superadmin / superadmin123（系统管理员，权限固定）与 admin / admin123（路灯管理员）
pub async fn bootstrap_admin(db: &PgPool) -> anyhow::Result<()> {
    ensure_account(
        db,
        "super_admin",
        "superadmin",
        "BOOTSTRAP_SUPER_ADMIN_USERNAME",
        "BOOTSTRAP_SUPER_ADMIN_PASSWORD",
        "superadmin123",
        "系统管理员",
    )
    .await?;
    ensure_account(
        db,
        "admin",
        "admin",
        "BOOTSTRAP_ADMIN_USERNAME",
        "BOOTSTRAP_ADMIN_PASSWORD",
        "admin123",
        "路灯管理员",
    )
    .await?;
    Ok(())
}

/// 若指定角色还没有账号，则按角色创建一个引导账号
async fn ensure_account(
    db: &PgPool,
    role_code: &str,
    default_user: &str,
    user_env: &str,
    pwd_env: &str,
    default_pwd: &str,
    real_name: &str,
) -> anyhow::Result<()> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(\
         SELECT 1 FROM app_user u JOIN role r ON r.id = u.role_id WHERE r.role_code = $1)",
    )
    .bind(role_code)
    .fetch_one(db)
    .await?;
    if exists {
        return Ok(());
    }
    let username =
        std::env::var(user_env).unwrap_or_else(|_| default_user.to_string());
    let password = std::env::var(pwd_env).unwrap_or_else(|_| {
        tracing::warn!(
            "{pwd_env} 未设置,使用默认账号 {username}/{default_pwd}(生产环境请覆盖)"
        );
        default_pwd.to_string()
    });
    let hash = hash_password(&password).map_err(|e| anyhow::anyhow!("{e}"))?;
    sqlx::query(
        "INSERT INTO app_user (username, password_hash, real_name, role_id, status) \
         VALUES ($1, $2, $3, (SELECT id FROM role WHERE role_code = $4), 1)",
    )
    .bind(&username)
    .bind(hash)
    .bind(real_name)
    .bind(role_code)
    .execute(db)
    .await?;
    tracing::info!("已创建引导账号 {username}（{role_code}）");
    Ok(())
}
