use std::sync::Arc;
use axum::{extract::Extension, response::IntoResponse, Json};

use crate::{
    auth::AuthUser,
    errors::ApiError,
    models::{UpdateUserRequest, User, UserResponse, UserRole},
    AppState,
};

#[utoipa::path(
    get,
    path = "/v1/me",
    responses(
        (status = 200, description = "Current authenticated user profile", body = UserResponse),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "User"
)]
pub async fn get_me(
    Extension(state): Extension<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
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
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

    Ok(Json(UserResponse::from(user)))
}

#[utoipa::path(
    patch,
    path = "/v1/me",
    request_body = UpdateUserRequest,
    responses(
        (status = 200, description = "Updated user profile", body = UserResponse),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "User"
)]
pub async fn update_me(
    Extension(state): Extension<Arc<AppState>>,
    auth_user: AuthUser,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user = sqlx::query_as!(
        User,
        r#"
        UPDATE users
        SET 
            timezone = COALESCE($1, timezone),
            full_name = COALESCE($2, full_name),
            avatar_url = COALESCE($3, avatar_url),
            updated_at = now()
        WHERE id = $4
        RETURNING id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
                  email_verified_at, role as "role: UserRole", is_premium, premium_until,
                  created_at, updated_at
        "#,
        payload.timezone,
        payload.full_name,
        payload.avatar_url,
        auth_user.id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

    Ok(Json(UserResponse::from(user)))
}
