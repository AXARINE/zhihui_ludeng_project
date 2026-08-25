mod api;
mod assistant;
mod auth;
mod iothub;
mod openapi;
#[cfg(test)]
mod tests;

use axum::Router;
use iothub::IothubClient;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    /// `IoTDA` 北向客户端,未配置 HUAWEI_* 环境变量时为 None
    pub iothub: Option<Arc<IothubClient>>,
    /// JWT 签名密钥,必须通过环境变量 `JWT_SECRET` 覆盖开发默认值
    pub jwt_secret: Arc<str>,
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

    let iothub = IothubClient::from_env()?;
    let state = Arc::new(AppState {
        db,
        iothub: iothub.clone(),
        jwt_secret,
    });

    if let Some(hub) = iothub {
        tokio::spawn(iothub::run(state.clone(), hub));
        tracing::info!("iothub poller started");
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app: Router = api::router(state.clone())
        .merge(auth::router(state.clone()))
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
    axum::serve(listener, app).await?;
    Ok(())
}
