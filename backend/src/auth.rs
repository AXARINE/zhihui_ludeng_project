//! 账号 / 登录 / RBAC:JWT + Argon2id + role/permission 权限检查。
//!
//! 中间件 `auth_middleware` 只做"你是谁"(认证),路由级的"你能不能做"由各 handler
//! 调用 `Auth::require()` 完成(权限码与种子数据见 `migrations/0002_rbac.sql`)。

use crate::AppState;
use crate::api::Error;
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use axum::extract::Request;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use jsonwebtoken::{
    DecodingKey, EncodingKey, Header, Validation, decode, encode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use utoipa::ToSchema;

const TOKEN_TTL_SECS: i64 = 24 * 60 * 60;

/// 中间件认证成功后塞进 `Request` extensions 的身份,handler 通过 extractor 取用
#[derive(Debug, Clone)]
pub struct Auth {
    pub user_id: i64,
    pub role_id: i64,
    pub role_code: String,
}

impl Auth {
    /// 检查当前角色是否拥有指定权限码(查 `role_permission`,尊重运行时可改的 RBAC 映射)
    pub async fn require(&self, db: &PgPool, perm: &str) -> Result<(), Error> {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
             SELECT 1 FROM role_permission rp \
             JOIN permission p ON p.id = rp.permission_id \
             WHERE rp.role_id = $1 AND p.perm_code = $2)",
        )
        .bind(self.role_id)
        .bind(perm)
        .fetch_one(db)
        .await?;
        if ok {
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

/// 全局认证中间件:非公开路径必须带 `Authorization: Bearer <token>`
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
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
    req.extensions_mut().insert(Auth {
        user_id: claims.user_id,
        role_id: claims.role_id,
        role_code: claims.role_code,
    });
    next.run(req).await
}

fn is_public(path: &str, method: &Method) -> bool {
    method == Method::OPTIONS
        || path == "/api/health"
        || path == "/api/auth/login"
        || path == "/docs"
        || path.starts_with("/docs/")
        || path == "/api/openapi.json"
}

// ---------------- 密码哈希 ----------------
pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::Internal(format!("密码哈希失败: {e}")))
}

fn verify_password(password: &str, hashed: &str) -> bool {
    PasswordHash::new(hashed).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

// ---------------- 输出模型 ----------------
#[derive(Serialize, ToSchema)]
pub struct RoleOut {
    pub id: i64,
    pub role_code: String,
    pub role_name: String,
    pub description: String,
}

#[derive(Serialize, ToSchema)]
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

// ---------------- 路由 ----------------
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", get(me))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", delete(delete_user))
        .route("/api/roles", get(list_roles))
        .route("/api/permissions", get(list_permissions))
        .route("/api/roles/{id}/permissions", put(update_role_permissions))
        .with_state(state)
}

// ---------------- 登录 ----------------
#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginIn,
    responses(
        (status = 200, description = "登录成功", body = LoginOut),
        (status = 401, description = "用户名或密码错误")
    )
)]
async fn login(
    State(s): State<Arc<AppState>>,
    Json(body): Json<LoginIn>,
) -> Result<Json<LoginOut>, Error> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.username, u.password_hash, u.real_name, u.role_id, u.status, \
                u.created_at, u.updated_at, r.role_code, r.role_name \
         FROM app_user u JOIN role r ON r.id = u.role_id \
         WHERE u.username = $1",
    )
    .bind(body.username.trim())
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| Error::Unauthorized("用户名或密码错误".into()))?;

    if user.status != 1 || !verify_password(&body.password, &user.password_hash)
    {
        return Err(Error::Unauthorized("用户名或密码错误".into()));
    }

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
    State(s): State<Arc<AppState>>,
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
    State(s): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<UserOut>>, Error> {
    auth.require(&s.db, "user:manage").await?;
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
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Json(body): Json<CreateUserIn>,
) -> Result<(StatusCode, Json<UserOut>), Error> {
    auth.require(&s.db, "user:manage").await?;
    let username = body.username.trim().to_string();
    if username.is_empty() || username.len() > 64 {
        return Err(Error::BadRequest("用户名长度需在 1~64 之间".into()));
    }
    if body.password.len() < 6 || body.password.len() > 64 {
        return Err(Error::BadRequest("密码长度需在 6~64 之间".into()));
    }
    let role_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM role WHERE id = $1)")
            .bind(body.role_id)
            .fetch_one(&s.db)
            .await?;
    if !role_exists {
        return Err(Error::BadRequest("角色不存在".into()));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_user WHERE username = $1)",
    )
    .bind(&username)
    .fetch_one(&s.db)
    .await?;
    if exists {
        return Err(Error::Conflict("用户名已存在".into()));
    }
    let hash = hash_password(&body.password)?;
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO app_user (username, password_hash, real_name, role_id, status) \
         VALUES ($1, $2, $3, $4, 1) RETURNING id",
    )
    .bind(&username)
    .bind(hash)
    .bind(body.real_name.trim())
    .bind(body.role_id)
    .fetch_one(&s.db)
    .await?;
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
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<i64>,
) -> Result<Json<UserOut>, Error> {
    auth.require(&s.db, "user:manage").await?;
    if auth.user_id == id {
        return Err(Error::Forbidden("不能删除当前登录账号".into()));
    }
    let user = fetch_user_by_id(&s.db, id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("账号 {id} 不存在")))?;
    sqlx::query("DELETE FROM app_user WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
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
    State(s): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<RoleOut>>, Error> {
    auth.require(&s.db, "user:manage").await?;
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, role_code, role_name, description FROM role ORDER BY id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, role_code, role_name, description)| RoleOut {
                id,
                role_code,
                role_name,
                description,
            })
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/permissions",
    responses((status = 200, description = "权限列表", body = Vec<PermissionOut>)),
    security(("bearer_auth" = []))
)]
async fn list_permissions(
    State(s): State<Arc<AppState>>,
    auth: Auth,
) -> Result<Json<Vec<PermissionOut>>, Error> {
    auth.require(&s.db, "user:manage").await?;
    let rows = sqlx::query_as::<_, (i64, String, String, String, String)>(
        "SELECT id, perm_code, perm_name, module, description FROM permission ORDER BY id",
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, perm_code, perm_name, module, description)| {
                PermissionOut {
                    id,
                    perm_code,
                    perm_name,
                    module,
                    description,
                }
            })
            .collect(),
    ))
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
    State(s): State<Arc<AppState>>,
    auth: Auth,
    Path(id): Path<i64>,
    Json(body): Json<RolePermissionsIn>,
) -> Result<StatusCode, Error> {
    auth.require(&s.db, "user:manage").await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM role WHERE id = $1)")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
    if !exists {
        return Err(Error::NotFound(format!("角色 {id} 不存在")));
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
    for pid in &body.permission_ids {
        sqlx::query(
            "INSERT INTO role_permission (role_id, permission_id) VALUES ($1, $2) \
             ON CONFLICT (role_id, permission_id) DO NOTHING",
        )
        .bind(id)
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------- 首次启动的引导管理员 ----------------
/// 首次启动且 `app_user` 为空时创建管理员,账号密码可用环境变量覆盖(默认 `admin`/`admin123`)
pub async fn bootstrap_admin(db: &PgPool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_user")
        .fetch_one(db)
        .await?;
    if count > 0 {
        return Ok(());
    }
    let username = std::env::var("BOOTSTRAP_ADMIN_USERNAME")
        .unwrap_or_else(|_| "admin".into());
    let password = std::env::var("BOOTSTRAP_ADMIN_PASSWORD").unwrap_or_else(|_| {
        tracing::warn!(
            "BOOTSTRAP_ADMIN_PASSWORD 未设置,使用默认账号 admin/admin123(生产环境请覆盖)"
        );
        "admin123".into()
    });
    let hash = hash_password(&password).map_err(|e| anyhow::anyhow!("{e}"))?;
    sqlx::query(
        "INSERT INTO app_user (username, password_hash, real_name, role_id, status) \
         VALUES ($1, $2, '系统管理员', (SELECT id FROM role WHERE role_code = 'admin'), 1)",
    )
    .bind(&username)
    .bind(hash)
    .execute(db)
    .await?;
    tracing::info!("已创建引导管理员账号 {username}");
    Ok(())
}
