pub mod auth;
pub mod config;
pub mod errors;
pub mod mailer;
pub mod models;
pub mod rate_limiter;
pub mod routes;

use axum::{
    Json, Router,
    extract::Extension,
    routing::{get, post},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_swagger_ui::SwaggerUi;

pub use auth::GoogleVerifier;
pub use config::Config;
pub use mailer::Mailer;
pub use rate_limiter::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub google_verifier: GoogleVerifier,
    pub mailer: Arc<dyn Mailer>,
    pub rate_limiter: RateLimiter,
}

pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        routes::auth::google_auth,
        routes::auth::register,
        routes::auth::login,
        routes::auth::verify_email,
        routes::auth::forgot_password,
        routes::auth::reset_password,
        routes::auth::refresh_token,
        routes::auth::logout,
        routes::me::get_me,
        routes::me::update_me,
        routes::home::get_home,
    ),
    components(
        schemas(
            models::UserRole,
            models::UserResponse,
            models::UpdateUserRequest,
            models::AuthResponse,
            models::RefreshResponse,
            models::HomeSummaryResponse,
            routes::auth::GoogleAuthRequest,
            routes::auth::RegisterRequest,
            routes::auth::LoginRequest,
            routes::auth::VerifyEmailRequest,
            routes::auth::ForgotPasswordRequest,
            routes::auth::ResetPasswordRequest,
            routes::auth::RefreshRequest,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Health", description = "Service health check endpoint"),
        (name = "Auth", description = "Authentication and credential lifecycle"),
        (name = "User", description = "User profile and settings"),
        (name = "Habits", description = "Habit tracking and daily summaries")
    ),
    info(
        title = "1degree API",
        version = "1.0.0",
        description = "REST API for 1degree (1degree.fit) habit tracking platform"
    )
)]
pub struct ApiDoc;

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = Value)
    )
)]
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "1degree-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub fn create_router(app_state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_check))
        // Authentication
        .route("/v1/auth/google", post(routes::auth::google_auth))
        .route("/v1/auth/register", post(routes::auth::register))
        .route("/v1/auth/login", post(routes::auth::login))
        .route("/v1/auth/verify-email", post(routes::auth::verify_email))
        .route(
            "/v1/auth/forgot-password",
            post(routes::auth::forgot_password),
        )
        .route(
            "/v1/auth/reset-password",
            post(routes::auth::reset_password),
        )
        .route("/v1/auth/refresh", post(routes::auth::refresh_token))
        .route("/v1/auth/logout", post(routes::auth::logout))
        // Profile
        .route(
            "/v1/me",
            get(routes::me::get_me).patch(routes::me::update_me),
        )
        // Home summary
        .route("/v1/home", get(routes::home::get_home))
        .merge(SwaggerUi::new("/v1/swagger-ui").url("/v1/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(Extension(app_state))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
