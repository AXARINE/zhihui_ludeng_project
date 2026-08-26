mod api;
mod assistant;
mod auth;
mod iothub;
mod openapi;
#[cfg(test)]
mod tests;
mod webhook;

use auth::LoginLimiter;
use axum::Router;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Method};
use iothub::IothubClient;
use sqlx::postgres::PgPoolOptions;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::signal::unix::SignalKind;
use tokio::sync::Semaphore;
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
    /// Argon2 校验并发闸(许可数 = `ARGON2_MAX_CONCURRENCY`,默认 32):
    /// 每次校验 19MiB 工作内存,登录风暴下无闸会被打爆 RSS(见 perf 报告 F2)
    pub argon2_sem: Arc<Semaphore>,
    /// 登录限流器(每 IP 每分钟 `LOGIN_RATE_LIMIT_PER_MIN` 次,默认 30,0 禁用)
    pub login_limiter: Arc<LoginLimiter>,
}

/// 读取正整数环境变量,缺失/非法时回落默认值
fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() -> anyhow::Result<()> {
    // 手写 runtime:`spawn_blocking` 线程上限 512 → 64。
    // login 的 Argon2 校验经 spawn_blocking 执行,默认上限允许的线程栈
    // (512×8MiB) 本身就是 4GiB 内存膨胀空间,压测实证 RSS 最高 12GiB(见 perf 报告 F2)
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .max_blocking_threads(64)
        .enable_all()
        .build()?;
    runtime.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://streetlight:streetlight@127.0.0.1:5432/streetlight".into()
    });

    // 连接池上限:`DATABASE_POOL_SIZE` 可调,默认 20。
    // 压测 A/B:5 → 20 时读接口 +77~90%、控灯 +269%(见 perf 报告 F3)
    let pool_size = u32::try_from(env_usize("DATABASE_POOL_SIZE", 20).max(1))
        .unwrap_or(u32::MAX);
    let db = PgPoolOptions::new()
        .max_connections(pool_size)
        .connect(&database_url)
        .await?;
    tracing::info!("database pool size: {pool_size}");
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

    let argon2_concurrency = env_usize("ARGON2_MAX_CONCURRENCY", 32).max(1);
    let login_limit = env_usize("LOGIN_RATE_LIMIT_PER_MIN", 30);
    let state = AppState {
        db,
        iothub: IothubClient::from_env()?,
        jwt_secret,
        perm_cache: PermCache::default(),
        argon2_sem: Arc::new(Semaphore::new(argon2_concurrency)),
        login_limiter: Arc::new(LoginLimiter::new(login_limit)),
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
    // ConnectInfo:登录限流按来源 IP 计数(直连部署;反代后需在网关侧限流)
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
async fn shutdown_signal() {
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
