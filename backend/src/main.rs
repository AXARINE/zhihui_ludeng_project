mod api;
mod assistant;
mod auth;
mod iothub;
mod openapi;
#[cfg(test)]
mod tests;
mod webhook;

use axum::Router;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Method};
use iothub::IothubClient;
use sqlx::postgres::PgPoolOptions;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use utoipa_swagger_ui::SwaggerUi;

/// 角色权限缓存:`role_id` → 权限码集合(命中即免一次 SQL;
/// 变更只经由 `update_role_permissions`,该接口提交后失效对应条目)
pub type PermCache = Arc<RwLock<HashMap<i64, Arc<HashSet<String>>>>>;

/// 应用状态。所有字段均为廉价 Clone(PgPool/Arc 内部共享),
/// 直接作为 axum 状态类型按值克隆,无需再套一层 `Arc<AppState>`
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    /// `IoTDA` 北向客户端,未配置 HUAWEI_* 环境变量时为 None
    pub iothub: Option<Arc<IothubClient>>,
    /// JWT 签名密钥,必须通过环境变量 `JWT_SECRET` 覆盖开发默认值
    pub jwt_secret: Arc<str>,
    pub perm_cache: PermCache,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://streetlight:streetlight@127.0.0.1:5432/streetlight".into()
    });

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("database migrated");

    auth::bootstrap_admin(&db).await?;

    let jwt_secret: Arc<str> = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| {
            tracing::warn!(
                "JWT_SECRET 未设置,使用开发默认值 dev-secret-change-me(生产环境必须覆盖)"
            );
            "dev-secret-change-me".into()
        })
        .into();

    let state = AppState {
        db,
        iothub: IothubClient::from_env()?,
        jwt_secret,
        perm_cache: PermCache::default(),
    };

    if let Some(hub) = state.iothub.clone() {
        tokio::spawn(iothub::run(state.clone(), hub));
        tracing::info!("iothub poller started");
    }

    // 跨域白名单:ALLOWED_ORIGINS 逗号分隔(各项 trim、空项忽略);
    // 未设置或解析后为空则保持 Any 全开(开发默认值)
    let cors = std::env::var("ALLOWED_ORIGINS").map_or_else(
        |_| permissive_cors(),
        |raw| {
            let origins: Vec<HeaderValue> = raw
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .filter_map(|s| HeaderValue::from_str(s).ok())
                .collect();
            if origins.is_empty() {
                permissive_cors()
            } else {
                tracing::info!("CORS 白名单已启用: {origins:?}");
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(origins))
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::PATCH,
                        Method::DELETE,
                        Method::OPTIONS,
                    ])
                    .allow_headers([AUTHORIZATION, CONTENT_TYPE])
            }
        },
    );

    let app: Router = api::router(state.clone())
        .merge(auth::router(state.clone()))
        .merge(webhook::router(state.clone()))
        .merge(
            SwaggerUi::new("/docs")
                .url("/api/openapi.json", openapi::openapi()),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("http listening on 0.0.0.0:8080 (Swagger UI: /docs)");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// 全放开 CORS(`ALLOWED_ORIGINS` 未设置或解析后为空时的开发默认值)
fn permissive_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}

/// 优雅停机信号:SIGINT(Ctrl+C)与 SIGTERM(`docker stop` 默认发送)任一到达即返回
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::SignalKind;
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("安装 SIGTERM 信号处理器失败");
    tokio::select! {
        res = tokio::signal::ctrl_c() => {
            if let Err(err) = res {
                tracing::error!(%err, "监听 SIGINT 失败");
            }
            tracing::info!("收到 SIGINT(Ctrl+C),开始优雅停机");
        }
        _ = sigterm.recv() => {
            tracing::info!("收到 SIGTERM,开始优雅停机");
        }
    }
}

/// Windows 优雅停机:仅监听 Ctrl+C
#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("监听 Ctrl+C 失败");
    tracing::info!("收到 Ctrl+C,开始优雅停机");
}
