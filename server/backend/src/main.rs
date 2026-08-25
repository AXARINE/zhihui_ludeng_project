mod api;
mod auth;
mod iothub;

use iothub::IothubClient;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    /// IoTDA 北向客户端,未配置 HUAWEI_* 环境变量时为 None
    pub iothub: Option<Arc<IothubClient>>,
    /// JWT 签名密钥
    pub jwt_secret: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 自动加载同目录下的 .env 文件,把里面的 DATABASE_URL / HUAWEI_* 读进环境变量
    // (原来只写了 .env 却没读,得手动 export 才生效,这里补上)
    let _ = dotenvy::dotenv();

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

    // 如果没有用户，自动创建默认管理员
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&db)
        .await?;
    if user_count.0 == 0 {
        let hash = auth::hash_password("admin123")
            .map_err(|e| anyhow::anyhow!("hash error: {e}"))?;
        sqlx::query(
            "INSERT INTO users (username, password_hash, real_name, role_id) \
             VALUES ('admin', $1, '系统管理员', 2)",
        )
        .bind(&hash)
        .execute(&db)
        .await?;
        tracing::info!("默认管理员已创建: admin / admin123");
    }

    let iothub = IothubClient::from_env()?;
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "streetlight-jwt-secret-change-me".to_string());
    let state = Arc::new(AppState {
        db,
        iothub: iothub.clone(),
        jwt_secret,
    });

    if let Some(hub) = iothub {
        tokio::spawn(iothub::run(state.clone(), hub));
        tracing::info!("iothub poller started");
    }

    let app = api::router(state)
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        );
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("http listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
