use axum::{
    Json,
    extract::{ConnectInfo, Extension},
    http::{
        HeaderMap, StatusCode,
        header::{COOKIE, ORIGIN, SET_COOKIE},
    },
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::net::SocketAddr;
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
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResendVerificationRequest {
    pub email: String,
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

fn verify_csrf_origin(headers: &HeaderMap, allowed_origin: &str) -> Result<(), ApiError> {
    if let Some(origin_val) = headers.get(ORIGIN) {
        let origin = origin_val
            .to_str()
            .map_err(|_| ApiError::Forbidden("Invalid Origin header".into()))?;
        if origin != allowed_origin {
            return Err(ApiError::Forbidden(
                "Cross-site request forbidden: Origin does not match allowed CORS origin".into(),
            ));
        }
    }
    Ok(())
}

const MAX_EMAIL_CHARS: usize = 254;
const MAX_EMAIL_LOCAL_CHARS: usize = 64;
const MAX_NAME_CHARS: usize = 100;

fn get_client_ip(connect_info: Option<Extension<ConnectInfo<SocketAddr>>>) -> String {
    // Do not trust spoofable X-Forwarded-For headers from direct peers.
    connect_info
        .map(|Extension(ConnectInfo(addr))| addr.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn validate_and_normalize_email(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_EMAIL_CHARS {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    }
    if !trimmed.is_ascii() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    }
    let Some((local, domain)) = trimmed.split_once('@') else {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    };
    if local.is_empty()
        || local.len() > MAX_EMAIL_LOCAL_CHARS
        || domain.is_empty()
        || domain.contains('@')
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
        || domain.split('.').any(str::is_empty)
    {
        return Err(ApiError::BadRequest("Invalid email address".into()));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn validate_and_normalize_name(raw: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(name) = raw else {
        return Ok(None);
    };
    if name.chars().any(|c| c.is_control()) {
        return Err(ApiError::BadRequest(
            "Name contains invalid control characters".into(),
        ));
    }
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::BadRequest(format!(
            "Name must be {} characters or fewer",
            MAX_NAME_CHARS
        )));
    }
    Ok(Some(trimmed.to_string()))
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
            if claims.email_verified.unwrap_or(false) {
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
                } else {
                    sqlx::query!(
                        r#"
                        UPDATE users
                        SET email_verified_at = COALESCE(email_verified_at, now())
                        WHERE id = $1 AND email_verified_at IS NULL
                        "#,
                        uid
                    )
                    .execute(&mut *tx)
                    .await?;
                }
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

    if let Some(ref em) = user.email {
        let lower = em.trim().to_ascii_lowercase();
        sqlx::query!("DELETE FROM pending_registrations WHERE email = $1", lower)
            .execute(&mut *tx)
            .await?;
    }

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
        (status = 201, description = "Generic registration confirmation"),
        (status = 400, description = "Validation error")
    ),
    tag = "Auth"
)]
pub async fn register(
    Extension(state): Extension<Arc<AppState>>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Rate limiting (peer socket IP; ignores spoofed X-Forwarded-For)
    let ip = get_client_ip(connect_info);
    state
        .rate_limiter
        .check(
            &format!("register:{}", ip),
            5,
            std::time::Duration::from_secs(60),
        )
        .await?;

    // 2. Validate email, name, and password length BEFORE Argon2 hashing
    let email = validate_and_normalize_email(&payload.email)?;
    let full_name = validate_and_normalize_name(payload.full_name)?;
    password::validate_password_length(&payload.password)?;

    // 3. Opportunistic cleanup of expired pending registrations (indexed by expires_at)
    let _ = sqlx::query!("DELETE FROM pending_registrations WHERE expires_at <= now()")
        .execute(&state.pool)
        .await;

    // 4. Hash password upfront (also provides Argon2 timing parity on existing-account path)
    let password_hash = password::hash_password(&payload.password)?;

    let generic_response = (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "message": "If this address can be used, we sent a verification link. It expires in 24 hours. Already have an account? Sign in or reset your password."
        })),
    );

    // 5. If any existing user (password or Google) already owns this email, create nothing and send notice (rate-limited)
    let existing_user = sqlx::query!("SELECT id FROM users WHERE LOWER(email) = $1", email)
        .fetch_optional(&state.pool)
        .await?;

    if existing_user.is_some() {
        let _ = sqlx::query!("DELETE FROM pending_registrations WHERE email = $1", email)
            .execute(&state.pool)
            .await;

        let cooldown_ok = state
            .rate_limiter
            .check(
                &format!("existing_notice:cooldown:{}", email),
                1,
                std::time::Duration::from_secs(60),
            )
            .await
            .is_ok();
        let daily_ok = state
            .rate_limiter
            .check(
                &format!("existing_notice:day:{}", email),
                5,
                std::time::Duration::from_secs(86_400),
            )
            .await
            .is_ok();

        if cooldown_ok && daily_ok {
            if let Err(err) = state.mailer.send_existing_account_notice(&email).await {
                tracing::error!(
                    "Failed to dispatch existing-account notice during registration: {}",
                    err
                );
            }
        }

        return Ok(generic_response);
    }

    // 6. Upsert pending_registrations with 60s cooldown and 5 sends/day cap (reset after 24h)
    let mut tx = state.pool.begin().await?;
    let existing_pending = sqlx::query!(
        r#"
        SELECT id, last_sent_at, send_count
        FROM pending_registrations
        WHERE email = $1
        FOR UPDATE
        "#,
        email
    )
    .fetch_optional(&mut *tx)
    .await?;

    let next_send_count = if let Some(pending) = existing_pending {
        let elapsed = Utc::now().signed_duration_since(pending.last_sent_at);
        if elapsed < Duration::seconds(60) {
            tx.rollback().await?;
            return Ok(generic_response);
        }
        let count = if elapsed >= Duration::hours(24) {
            1
        } else {
            pending.send_count + 1
        };
        if count > 5 {
            tx.rollback().await?;
            return Ok(generic_response);
        }
        count
    } else {
        1
    };

    let raw_token = jwt::generate_refresh_token();
    let token_hash = jwt::hash_refresh_token(&raw_token);
    let expires_at = Utc::now() + Duration::hours(24);

    sqlx::query!(
        r#"
        INSERT INTO pending_registrations
            (email, full_name, password_hash, token_hash, expires_at, last_sent_at, send_count, failed_attempts)
        VALUES ($1, $2, $3, $4, $5, now(), $6, 0)
        ON CONFLICT (email) DO UPDATE SET
            full_name = EXCLUDED.full_name,
            password_hash = EXCLUDED.password_hash,
            token_hash = EXCLUDED.token_hash,
            expires_at = EXCLUDED.expires_at,
            last_sent_at = now(),
            send_count = EXCLUDED.send_count,
            failed_attempts = 0
        "#,
        email,
        full_name,
        password_hash,
        token_hash,
        expires_at,
        next_send_count
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if let Err(err) = state
        .mailer
        .send_verification_email(&email, &raw_token)
        .await
    {
        tracing::error!(
            "Failed to dispatch verification email during registration: {}",
            err
        );
    }

    Ok(generic_response)
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
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // 1. Rate limiting (peer socket IP; ignores spoofed X-Forwarded-For)
    let ip = get_client_ip(connect_info);
    state
        .rate_limiter
        .check(
            &format!("login:{}", ip),
            10,
            std::time::Duration::from_secs(60),
        )
        .await?;

    // Reject oversized/control inputs BEFORE touching DB or Argon2id
    if payload.email.trim().len() > MAX_EMAIL_CHARS
        || password::validate_password_length(&payload.password).is_err()
    {
        return Err(ApiError::Unauthorized("Invalid email or password".into()));
    }
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
            // Check pending_registrations before falling back cleanly with dummy Argon2 timing parity
            let pending = sqlx::query!(
                "SELECT password_hash FROM pending_registrations WHERE email = $1 AND expires_at > now()",
                email
            )
            .fetch_optional(&state.pool)
            .await?;

            if let Some(p) = pending {
                if password::verify_password(&payload.password, &p.password_hash)? {
                    return Err(ApiError::EmailNotVerified(
                        "Please verify your email before signing in.".into(),
                    ));
                }
            } else {
                password::verify_dummy_password(&payload.password);
            }
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

            password::verify_dummy_password(&payload.password);
            return Err(ApiError::Unauthorized("Invalid email or password".into()));
        }
    };

    // 3. Verify password with Argon2id (never log or return hash)
    let is_valid = password::verify_password(&payload.password, pwd_hash)?;
    if !is_valid {
        return Err(ApiError::Unauthorized("Invalid email or password".into()));
    }

    // 4. Reject verified-required password login if email is not verified
    if user.email_verified_at.is_none() {
        return Err(ApiError::EmailNotVerified(
            "Please verify your email before signing in.".into(),
        ));
    }

    // 5. Issue 15-minute access token and rotating refresh token
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
        (status = 200, description = "Email verified and account created"),
        (status = 400, description = "Invalid token, expired token, or password mismatch")
    ),
    tag = "Auth"
)]
pub async fn verify_email(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if payload.token.trim().is_empty() || payload.password.is_empty() {
        return Err(ApiError::BadRequest(
            "Verification token and password are required.".into(),
        ));
    }
    if payload.password.len() > 512 {
        return Err(ApiError::BadRequest(
            "Invalid verification token or password.".into(),
        ));
    }

    let token_hash = jwt::hash_refresh_token(payload.token.trim());
    let mut tx = state.pool.begin().await?;

    let pending = sqlx::query!(
        r#"
        SELECT id, email, full_name, password_hash, expires_at, failed_attempts
        FROM pending_registrations
        WHERE token_hash = $1
        FOR UPDATE
        "#,
        token_hash
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(pending) = pending else {
        return Err(ApiError::BadRequest(
            "Invalid or expired verification token".into(),
        ));
    };

    if pending.failed_attempts >= 5 || pending.expires_at < Utc::now() {
        return Err(ApiError::BadRequest(
            "Invalid or expired verification token".into(),
        ));
    }

    let password_matches = password::verify_password(&payload.password, &pending.password_hash)?;
    if !password_matches {
        let next_attempts = pending.failed_attempts + 1;
        sqlx::query!(
            r#"
            UPDATE pending_registrations
            SET failed_attempts = $2,
                token_hash = CASE WHEN $2 >= 5 THEN NULL ELSE token_hash END
            WHERE id = $1
            "#,
            pending.id,
            next_attempts
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        if next_attempts >= 5 {
            return Err(ApiError::BadRequest(
                "Too many incorrect password attempts. This link is no longer valid; please request a new verification email.".into(),
            ));
        }
        return Err(ApiError::BadRequest(
            "Incorrect password for this registration link.".into(),
        ));
    }

    let email_lower = pending.email.trim().to_ascii_lowercase();
    let existing_user = sqlx::query!(
        "SELECT id FROM users WHERE LOWER(email) = $1 FOR UPDATE",
        email_lower
    )
    .fetch_optional(&mut *tx)
    .await?;

    if existing_user.is_some() {
        sqlx::query!(
            "DELETE FROM pending_registrations WHERE id = $1",
            pending.id
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Err(ApiError::BadRequest(
            "An account with this email already exists. Please sign in.".into(),
        ));
    }

    let initial_role = if state.config.admin_emails.contains(&email_lower) {
        UserRole::Admin
    } else {
        UserRole::User
    };

    let insert_result = sqlx::query!(
        r#"
        INSERT INTO users (email, password_hash, full_name, timezone, role, email_verified_at)
        VALUES ($1, $2, $3, 'UTC', $4, now())
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
        email_lower,
        pending.password_hash,
        pending.full_name,
        initial_role as UserRole
    )
    .fetch_optional(&mut *tx)
    .await?;

    sqlx::query!(
        "DELETE FROM pending_registrations WHERE id = $1",
        pending.id
    )
    .execute(&mut *tx)
    .await?;

    if insert_result.is_none() {
        tx.commit().await?;
        return Err(ApiError::BadRequest(
            "An account with this email already exists. Please sign in.".into(),
        ));
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "message": "Email verified successfully. You may now sign in."
    })))
}

#[utoipa::path(
    post,
    path = "/v1/auth/resend-verification",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, description = "Generic confirmation regardless of account existence"),
        (status = 429, description = "Rate limit exceeded")
    ),
    tag = "Auth"
)]
pub async fn resend_verification(
    Extension(state): Extension<Arc<AppState>>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ip = get_client_ip(connect_info);
    state
        .rate_limiter
        .check(
            &format!("resend_verification:ip:{}", ip),
            10,
            std::time::Duration::from_secs(60),
        )
        .await?;

    let email = validate_and_normalize_email(&payload.email)?;

    state
        .rate_limiter
        .check(
            &format!("resend_verification:email:{}", email),
            5,
            std::time::Duration::from_secs(900),
        )
        .await?;

    let generic_response = Json(serde_json::json!({
        "message": "If a pending registration exists for this email, a verification link has been sent."
    }));

    let user_exists = sqlx::query!("SELECT id FROM users WHERE LOWER(email) = $1", email)
        .fetch_optional(&state.pool)
        .await?;
    if user_exists.is_some() {
        return Ok(generic_response);
    }

    let mut tx = state.pool.begin().await?;
    let pending = sqlx::query!(
        r#"
        SELECT id, last_sent_at, send_count
        FROM pending_registrations
        WHERE email = $1 AND expires_at > now()
        FOR UPDATE
        "#,
        email
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(pending) = pending else {
        tx.rollback().await?;
        return Ok(generic_response);
    };

    let elapsed = Utc::now().signed_duration_since(pending.last_sent_at);
    if elapsed < Duration::seconds(60) {
        tx.rollback().await?;
        return Ok(generic_response);
    }

    let next_send_count = if elapsed >= Duration::hours(24) {
        1
    } else {
        pending.send_count + 1
    };
    if next_send_count > 5 {
        tx.rollback().await?;
        return Ok(generic_response);
    }

    let raw_token = jwt::generate_refresh_token();
    let token_hash = jwt::hash_refresh_token(&raw_token);
    let expires_at = Utc::now() + Duration::hours(24);

    sqlx::query!(
        r#"
        UPDATE pending_registrations
        SET token_hash = $2,
            expires_at = $3,
            last_sent_at = now(),
            send_count = $4,
            failed_attempts = 0
        WHERE id = $1
        "#,
        pending.id,
        token_hash,
        expires_at,
        next_send_count
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if let Err(err) = state
        .mailer
        .send_verification_email(&email, &raw_token)
        .await
    {
        tracing::error!("Failed to dispatch verification email on resend: {}", err);
    }

    Ok(generic_response)
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
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ip = get_client_ip(connect_info);
    state
        .rate_limiter
        .check(
            &format!("forgot_password:ip:{}", ip),
            20,
            std::time::Duration::from_secs(60),
        )
        .await?;

    let email = validate_and_normalize_email(&payload.email)?;

    state
        .rate_limiter
        .check(
            &format!("forgot_password:email:{}", email),
            5,
            std::time::Duration::from_secs(900),
        )
        .await?;

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

            let mut tx = state.pool.begin().await?;
            let _ = sqlx::query!(
                "UPDATE password_reset_tokens SET used_at = now() WHERE user_id = $1 AND used_at IS NULL",
                u.id
            )
            .execute(&mut *tx)
            .await;

            let _ = sqlx::query!(
                r#"
                INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
                VALUES ($1, $2, $3)
                "#,
                u.id,
                token_hash,
                expires_at
            )
            .execute(&mut *tx)
            .await;

            let _ = tx.commit().await;

            if let Err(err) = state
                .mailer
                .send_password_reset_email(&email, &raw_token)
                .await
            {
                tracing::error!("Failed to dispatch password reset email: {}", err);
            }
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
    password::validate_password_length(&payload.new_password)?;

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
    verify_csrf_origin(&headers, &state.config.cors_origin)?;

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
    verify_csrf_origin(&headers, &state.config.cors_origin)?;

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
