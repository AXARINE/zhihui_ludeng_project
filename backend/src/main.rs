mod api;
mod iothub;

use iothub::IothubClient;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    /// IoTDA 北向客户端,未配置 HUAWEI_* 环境变量时为 None
    pub iothub: Option<Arc<IothubClient>>,
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

    let iothub = IothubClient::from_env()?;
    let state = Arc::new(AppState {
        db,
        iothub: iothub.clone(),
    });

    if let Some(hub) = iothub {
        tokio::spawn(iothub::run(state.clone(), hub));
        tracing::info!("iothub poller started");
    }

    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("http listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;
    Ok(())
}
