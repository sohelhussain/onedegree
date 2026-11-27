use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use api::{
    AppState, GoogleVerifier,
    config::Config,
    create_router,
    mailer::{build_mailer, startup_mailer_notice},
    rate_limiter::RateLimiter,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(err) = dotenvy::dotenv() {
        if !matches!(err, dotenvy::Error::Io(_)) {
            eprintln!(
                "Warning: .env contains a syntax error and was only partially loaded; relying on process environment variables."
            );
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,api=debug,tower_http=debug".into()),
        )
        .init();

    let config = Config::try_from_env().map_err(|err| {
        error!("FATAL: Configuration error: {}", err);
        err
    })?;

    let mailer = build_mailer(&config).map_err(|err| {
        error!("FATAL: Mailer configuration error: {}", err);
        err
    })?;
    info!("{}", startup_mailer_notice(&config));

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

    let google_verifier = GoogleVerifier::with_jwks_url(
        config.google_client_id.clone(),
        config.google_jwks_url.clone(),
    );
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
    info!(
        "Swagger UI documentation available at http://{}/v1/swagger-ui",
        addr
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
