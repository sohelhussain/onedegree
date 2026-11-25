use axum::{
    Json,
    extract::Extension,
    http::{
        HeaderMap, StatusCode,
        header::{COOKIE, SET_COOKIE},
    },
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::{jwt, password},
    errors::ApiError,
    models::{AuthResponse, RefreshResponse, User, UserRole},
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct GoogleAuthRequest {
    pub id_token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub full_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

fn get_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn extract_refresh_token(headers: &HeaderMap, body: &RefreshRequest) -> Option<String> {
    if let Some(token) = &body.refresh_token {
        if !token.trim().is_empty() {
            return Some(token.clone());
        }
    }

    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|pair| {
            let (name, val) = pair.trim().split_once('=')?;
            if name == "refresh_token" {
                Some(val.to_string())
            } else {
                None
            }
        })
}

fn build_cookie_header(token: &str, max_age: i64) -> String {
    format!(
        "refresh_token={}; HttpOnly; Path=/v1/auth; SameSite=Lax; Max-Age={}",
        token, max_age
    )
}

#[utoipa::path(
    post,
    path = "/v1/auth/google",
    request_body = GoogleAuthRequest,
    responses(
        (status = 200, description = "Google authentication successful", body = AuthResponse),
        (status = 401, description = "Invalid Google ID token")
    ),
    tag = "Auth"
)]
pub async fn google_auth(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<GoogleAuthRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let claims = state.google_verifier.verify(&payload.id_token).await?;

    let mut tx = state.pool.begin().await?;

    // Determine initial role: Admin only if email is verified and in ADMIN_EMAILS
    let initial_role = if let Some(ref em) = claims.email {
        let lower = em.trim().to_lowercase();
        if claims.email_verified.unwrap_or(false) && state.config.admin_emails.contains(&lower) {
            UserRole::Admin
        } else {
            UserRole::User
        }
    } else {
        UserRole::User
    };

    let existing_user_id: Option<uuid::Uuid> = sqlx::query_scalar!(
        r#"
        SELECT user_id 
        FROM auth_identities 
        WHERE provider = 'google' AND provider_user_id = $1
        "#,
        claims.sub
    )
    .fetch_optional(&mut *tx)
    .await?;

    let user = match existing_user_id {
        Some(uid) => {
            // If user's email is verified by Google and matches ADMIN_EMAILS, update role to admin if not already
            if initial_role == UserRole::Admin {
                sqlx::query!(
                    r#"
                    UPDATE users
                    SET role = 'admin', email_verified_at = COALESCE(email_verified_at, now())
                    WHERE id = $1
                    "#,
                    uid
                )
                .execute(&mut *tx)
                .await?;
            }

            sqlx::query_as!(
                User,
                r#"
                SELECT id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
                       email_verified_at, role as "role: UserRole", is_premium, premium_until,
                       created_at, updated_at
                FROM users
                WHERE id = $1
                "#,
                uid
            )
            .fetch_one(&mut *tx)
            .await?
        }
        None => {
            let email_verified_at = if claims.email_verified.unwrap_or(false) {
                Some(Utc::now())
            } else {
                None
            };

            let new_user = sqlx::query_as!(
                User,
                r#"
                INSERT INTO users (email, full_name, avatar_url, timezone, role, email_verified_at)
                VALUES ($1, $2, $3, 'UTC', $4, $5)
                RETURNING id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
                          email_verified_at, role as "role: UserRole", is_premium, premium_until,
                          created_at, updated_at
                "#,
                claims.email,
                claims.name,
                claims.picture,
                initial_role as UserRole,
                email_verified_at
            )
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query!(
                r#"
                INSERT INTO auth_identities (user_id, provider, provider_user_id)
                VALUES ($1, 'google', $2)
                "#,
                new_user.id,
                claims.sub
            )
            .execute(&mut *tx)
            .await?;

            new_user
        }
    };

    let access_token = jwt::create_access_token(
        user.id,
        state.config.jwt_secret.as_bytes(),
        state.config.jwt_access_expiration_secs,
    )?;

    let raw_refresh = jwt::generate_refresh_token();
    let hashed_refresh = jwt::hash_refresh_token(&raw_refresh);
    let expires_at =
        Utc::now() + chrono::Duration::seconds(state.config.jwt_refresh_expiration_secs);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        user.id,
        hashed_refresh,
        expires_at
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let cookie = build_cookie_header(&raw_refresh, state.config.jwt_refresh_expiration_secs);
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, cookie.parse().unwrap());

    Ok((
        headers,
        Json(AuthResponse {
            access_token,
            expires_in: state.config.jwt_access_expiration_secs,
            user: user.into(),
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registration successful; check email for verification"),
        (status = 400, description = "Validation error or email already in use")
    ),
    tag = "Auth"
)]
pub async fn register(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Rate limiting
    let ip = get_client_ip(&headers);
    state
        .rate_limiter
        .check(
            &format!("register:{}", ip),
            5,
            std::time::Duration::from_secs(60),
        )
        .await?;

    // 2. Validate password length (minimum 8 characters)
    if payload.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "Password must be at least 8 characters long".into(),
        ));
    }

    let email = payload.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    }

    // 3. Reject if email belongs to an existing Google account
    let google_account = sqlx::query!(
        r#"
        SELECT u.id 
        FROM users u
        JOIN auth_identities ai ON ai.user_id = u.id
        WHERE LOWER(u.email) = $1 AND ai.provider = 'google'
        "#,
        email
    )
    .fetch_optional(&state.pool)
    .await?;

    if google_account.is_some() {
        return Err(ApiError::BadRequest(
            "This email is registered with Google. Please sign in with Google.".into(),
        ));
    }

    // 4. Reject if email belongs to an existing password account
    let existing_user = sqlx::query!("SELECT id FROM users WHERE LOWER(email) = $1", email)
        .fetch_optional(&state.pool)
        .await?;

    if existing_user.is_some() {
        return Err(ApiError::BadRequest("Email is already registered".into()));
    }

    // 5. Hash password with Argon2id (never logged or returned)
    let password_hash = password::hash_password(&payload.password)?;

    let mut tx = state.pool.begin().await?;

    // 6. Insert user (role is always 'user' at registration; unverified email cannot be admin)
    let user_id = sqlx::query_scalar!(
        r#"
        INSERT INTO users (email, password_hash, full_name, timezone, role)
        VALUES ($1, $2, $3, 'UTC', 'user')
        RETURNING id
        "#,
        email,
        password_hash,
        payload.full_name
    )
    .fetch_one(&mut *tx)
    .await?;

    // 7. Generate email verification token (random, stored hashed, single-use, 24h expiry)
    let raw_token = jwt::generate_refresh_token();
    let token_hash = jwt::hash_refresh_token(&raw_token);
    let expires_at = Utc::now() + Duration::hours(24);

    sqlx::query!(
        r#"
        INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        user_id,
        token_hash,
        expires_at
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 8. Dispatch email via mailer
    let _ = state
        .mailer
        .send_verification_email(&email, &raw_token)
        .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "message": "Registration successful. Please verify your email to activate your account."
        })),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthResponse),
        (status = 401, description = "Invalid email or password")
    ),
    tag = "Auth"
)]
pub async fn login(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Rate limiting
    let ip = get_client_ip(&headers);
    state
        .rate_limiter
        .check(
            &format!("login:{}", ip),
            10,
            std::time::Duration::from_secs(60),
        )
        .await?;

    let email = payload.email.trim().to_lowercase();

    // 2. Query user by lowercase email
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
               email_verified_at, role as "role: UserRole", is_premium, premium_until,
               created_at, updated_at
        FROM users
        WHERE LOWER(email) = $1
        "#,
        email
    )
    .fetch_optional(&state.pool)
    .await?;

    let user = match user {
        Some(u) => u,
        None => {
            // Same error message for wrong email and wrong password
            return Err(ApiError::Unauthorized("Invalid email or password".into()));
        }
    };

    // If user has no password_hash, check if it's a Google account
    let pwd_hash = match &user.password_hash {
        Some(h) => h,
        None => {
            let google_identity = sqlx::query!(
                "SELECT id FROM auth_identities WHERE user_id = $1 AND provider = 'google'",
                user.id
            )
            .fetch_optional(&state.pool)
            .await?;

            if google_identity.is_some() {
                return Err(ApiError::BadRequest(
                    "This email is registered with Google. Please sign in with Google.".into(),
                ));
            }

            return Err(ApiError::Unauthorized("Invalid email or password".into()));
        }
    };

    // 3. Verify password with Argon2id (never log or return hash)
    let is_valid = password::verify_password(&payload.password, pwd_hash)?;
    if !is_valid {
        return Err(ApiError::Unauthorized("Invalid email or password".into()));
    }

    // 4. Issue 15-minute access token and rotating refresh token
    let access_token = jwt::create_access_token(
        user.id,
        state.config.jwt_secret.as_bytes(),
        state.config.jwt_access_expiration_secs,
    )?;

    let raw_refresh = jwt::generate_refresh_token();
    let hashed_refresh = jwt::hash_refresh_token(&raw_refresh);
    let expires_at = Utc::now() + Duration::seconds(state.config.jwt_refresh_expiration_secs);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        user.id,
        hashed_refresh,
        expires_at
    )
    .execute(&state.pool)
    .await?;

    let cookie = build_cookie_header(&raw_refresh, state.config.jwt_refresh_expiration_secs);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(SET_COOKIE, cookie.parse().unwrap());

    Ok((
        response_headers,
        Json(AuthResponse {
            access_token,
            expires_in: state.config.jwt_access_expiration_secs,
            user: user.into(),
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/auth/verify-email",
    request_body = VerifyEmailRequest,
    responses(
        (status = 200, description = "Email verified successfully"),
        (status = 400, description = "Invalid or expired token")
    ),
    tag = "Auth"
)]
pub async fn verify_email(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let token_hash = jwt::hash_refresh_token(&payload.token);

    let mut tx = state.pool.begin().await?;

    let record = sqlx::query!(
        r#"
        SELECT id, user_id, expires_at, used_at
        FROM email_verification_tokens
        WHERE token_hash = $1
        FOR UPDATE
        "#,
        token_hash
    )
    .fetch_optional(&mut *tx)
    .await?;

    let record = match record {
        Some(r) => r,
        None => {
            return Err(ApiError::BadRequest(
                "Invalid or expired verification token".into(),
            ));
        }
    };

    if record.used_at.is_some() || record.expires_at < Utc::now() {
        return Err(ApiError::BadRequest(
            "Invalid or expired verification token".into(),
        ));
    }

    // Mark token used (single-use)
    sqlx::query!(
        "UPDATE email_verification_tokens SET used_at = now() WHERE id = $1",
        record.id
    )
    .execute(&mut *tx)
    .await?;

    // Mark email verified
    sqlx::query!(
        "UPDATE users SET email_verified_at = now() WHERE id = $1",
        record.user_id
    )
    .execute(&mut *tx)
    .await?;

    // Check if user's verified email is in ADMIN_EMAILS
    let user_email: Option<String> =
        sqlx::query_scalar!("SELECT email FROM users WHERE id = $1", record.user_id)
            .fetch_one(&mut *tx)
            .await?;

    if let Some(em) = user_email {
        let lower = em.trim().to_lowercase();
        if state.config.admin_emails.contains(&lower) {
            sqlx::query!(
                "UPDATE users SET role = 'admin' WHERE id = $1",
                record.user_id
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "message": "Email verified successfully. You may now access all account features."
    })))
}

#[utoipa::path(
    post,
    path = "/v1/auth/forgot-password",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Instructions sent if email exists")
    ),
    tag = "Auth"
)]
pub async fn forgot_password(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let email = payload.email.trim().to_lowercase();

    let user = sqlx::query!(
        "SELECT id, password_hash FROM users WHERE LOWER(email) = $1",
        email
    )
    .fetch_optional(&state.pool)
    .await?;

    if let Some(u) = user {
        if u.password_hash.is_some() {
            let raw_token = jwt::generate_refresh_token();
            let token_hash = jwt::hash_refresh_token(&raw_token);
            let expires_at = Utc::now() + Duration::hours(1); // 1 hour expiry for reset

            let _ = sqlx::query!(
                r#"
                INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
                VALUES ($1, $2, $3)
                "#,
                u.id,
                token_hash,
                expires_at
            )
            .execute(&state.pool)
            .await;

            let _ = state
                .mailer
                .send_password_reset_email(&email, &raw_token)
                .await;
        }
    }

    Ok(Json(serde_json::json!({
        "message": "If this email is registered, password reset instructions have been sent."
    })))
}

#[utoipa::path(
    post,
    path = "/v1/auth/reset-password",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset successfully"),
        (status = 400, description = "Invalid token or password too short")
    ),
    tag = "Auth"
)]
pub async fn reset_password(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if payload.new_password.len() < 8 {
        return Err(ApiError::BadRequest(
            "New password must be at least 8 characters long".into(),
        ));
    }

    let token_hash = jwt::hash_refresh_token(&payload.token);
    let mut tx = state.pool.begin().await?;

    let record = sqlx::query!(
        r#"
        SELECT id, user_id, expires_at, used_at
        FROM password_reset_tokens
        WHERE token_hash = $1
        FOR UPDATE
        "#,
        token_hash
    )
    .fetch_optional(&mut *tx)
    .await?;

    let record = match record {
        Some(r) => r,
        None => {
            return Err(ApiError::BadRequest(
                "Invalid or expired password reset token".into(),
            ));
        }
    };

    if record.used_at.is_some() || record.expires_at < Utc::now() {
        return Err(ApiError::BadRequest(
            "Invalid or expired password reset token".into(),
        ));
    }

    let new_hash = password::hash_password(&payload.new_password)?;

    // Update password
    sqlx::query!(
        "UPDATE users SET password_hash = $1 WHERE id = $2",
        new_hash,
        record.user_id
    )
    .execute(&mut *tx)
    .await?;

    // Mark reset token used (single-use)
    sqlx::query!(
        "UPDATE password_reset_tokens SET used_at = now() WHERE id = $1",
        record.id
    )
    .execute(&mut *tx)
    .await?;

    // Revoke all refresh tokens on password change
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1",
        record.user_id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "message": "Password reset successfully. You may now log in with your new password."
    })))
}

#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    request_body(content = Option<RefreshRequest>),
    responses(
        (status = 200, description = "Refresh token rotated successfully", body = RefreshResponse),
        (status = 401, description = "Invalid, expired, or replayed refresh token")
    ),
    tag = "Auth"
)]
pub async fn refresh_token(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<RefreshRequest>>,
) -> Result<impl IntoResponse, ApiError> {
    let req_body = body.map(|b| b.0).unwrap_or_default();
    let raw_token = extract_refresh_token(&headers, &req_body)
        .ok_or_else(|| ApiError::Unauthorized("Missing refresh token in cookie or body".into()))?;

    let token_hash = jwt::hash_refresh_token(&raw_token);
    let mut tx = state.pool.begin().await?;

    let record = sqlx::query!(
        r#"
        SELECT id, user_id, expires_at, revoked
        FROM refresh_tokens
        WHERE token_hash = $1
        FOR UPDATE
        "#,
        token_hash
    )
    .fetch_optional(&mut *tx)
    .await?;

    let record = match record {
        Some(r) => r,
        None => {
            return Err(ApiError::Unauthorized("Invalid refresh token".into()));
        }
    };

    if record.revoked {
        sqlx::query!(
            r#"
            UPDATE refresh_tokens
            SET revoked = TRUE
            WHERE user_id = $1
            "#,
            record.user_id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        return Err(ApiError::Unauthorized(
            "Refresh token has been revoked or replayed. All user sessions terminated.".into(),
        ));
    }

    if record.expires_at < Utc::now() {
        sqlx::query!(
            r#"
            UPDATE refresh_tokens
            SET revoked = TRUE
            WHERE id = $1
            "#,
            record.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        return Err(ApiError::Unauthorized("Refresh token has expired".into()));
    }

    // Mark current revoked
    sqlx::query!(
        r#"
        UPDATE refresh_tokens
        SET revoked = TRUE
        WHERE id = $1
        "#,
        record.id
    )
    .execute(&mut *tx)
    .await?;

    // Issue new refresh token
    let new_raw_refresh = jwt::generate_refresh_token();
    let new_hashed_refresh = jwt::hash_refresh_token(&new_raw_refresh);
    let new_expires_at = Utc::now() + Duration::seconds(state.config.jwt_refresh_expiration_secs);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        record.user_id,
        new_hashed_refresh,
        new_expires_at
    )
    .execute(&mut *tx)
    .await?;

    // Issue new 15-minute access token
    let access_token = jwt::create_access_token(
        record.user_id,
        state.config.jwt_secret.as_bytes(),
        state.config.jwt_access_expiration_secs,
    )?;

    tx.commit().await?;

    let cookie = build_cookie_header(&new_raw_refresh, state.config.jwt_refresh_expiration_secs);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(SET_COOKIE, cookie.parse().unwrap());

    Ok((
        response_headers,
        Json(RefreshResponse {
            access_token,
            expires_in: state.config.jwt_access_expiration_secs,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 200, description = "Logged out successfully")
    ),
    tag = "Auth"
)]
pub async fn logout(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<RefreshRequest>>,
) -> Result<impl IntoResponse, ApiError> {
    let req_body = body.map(|b| b.0).unwrap_or_default();
    if let Some(raw_token) = extract_refresh_token(&headers, &req_body) {
        let token_hash = jwt::hash_refresh_token(&raw_token);
        let _ = sqlx::query!(
            r#"
            UPDATE refresh_tokens
            SET revoked = TRUE
            WHERE token_hash = $1
            "#,
            token_hash
        )
        .execute(&state.pool)
        .await;
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        "refresh_token=; HttpOnly; Path=/v1/auth; SameSite=Lax; Max-Age=0"
            .parse()
            .unwrap(),
    );

    Ok((
        response_headers,
        Json(serde_json::json!({
            "message": "Logged out successfully"
        })),
    ))
}
