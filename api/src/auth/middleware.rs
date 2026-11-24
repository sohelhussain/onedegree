use std::sync::Arc;
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use crate::{
    auth::jwt,
    errors::ApiError,
    models::{User, UserRole},
    AppState,
};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let app_state = parts
            .extensions
            .get::<Arc<AppState>>()
            .ok_or_else(|| ApiError::Internal("AppState extension missing".into()))?;

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("Missing Authorization header".into()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| {
                ApiError::Unauthorized("Invalid Authorization header format; expected Bearer token".into())
            })?;

        let user_id = jwt::verify_access_token(token, app_state.config.jwt_secret.as_bytes())?;

        Ok(AuthUser { id: user_id })
    }
}

/// Extractor that requires the authenticated user to possess role == UserRole::Admin
#[derive(Debug, Clone)]
pub struct RequireAdmin {
    pub user: User,
}

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_user = AuthUser::from_request_parts(parts, state).await?;

        let app_state = parts
            .extensions
            .get::<Arc<AppState>>()
            .ok_or_else(|| ApiError::Internal("AppState extension missing".into()))?;

        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
                   email_verified_at, role as "role: UserRole", is_premium, premium_until,
                   created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
            auth_user.id
        )
        .fetch_optional(&app_state.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

        if user.role != UserRole::Admin {
            return Err(ApiError::Forbidden(
                "Access denied: administrator privileges required".into(),
            ));
        }

        Ok(RequireAdmin { user })
    }
}
