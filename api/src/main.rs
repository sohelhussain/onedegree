use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use api::{
    config::Config,
    create_router,
    mailer::ConsoleMailer,
    rate_limiter::RateLimiter,
    AppState, GoogleVerifier,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,api=debug,tower_http=debug".into()),
        )
        .init();

    // Refuse to start when APP_ENV=production and mailer is ConsoleMailer
    let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());
    if app_env.eq_ignore_ascii_case("production") {
        tracing::error!(
            "FATAL: Refusing to start in production environment with ConsoleMailer. Configure a production mailer service."
        );
        return Err("Refusing to start in production with ConsoleMailer".into());
    }

    let config = Config::from_env();
    info!("Connecting to PostgreSQL database...");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(10))
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    info!("Applying pending database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    info!("Database migrations applied successfully.");

    let google_verifier = GoogleVerifier::new(config.google_client_id.clone());
    let mailer = Arc::new(ConsoleMailer::default());
    let rate_limiter = RateLimiter::new();

    let state = Arc::new(AppState {
        pool,
        config: config.clone(),
        google_verifier,
        mailer,
        rate_limiter,
    });

    let app = create_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));

    info!("1degree Axum API listening on http://{}", addr);
    info!("Swagger UI documentation available at http://{}/v1/swagger-ui", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
