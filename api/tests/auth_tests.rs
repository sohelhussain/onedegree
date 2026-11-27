use axum::{
    Json, Router,
    body::Body,
    extract::Extension,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, ORIGIN, SET_COOKIE},
    },
    routing::get,
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

use api::{
    AppState, GoogleVerifier, Mailer,
    auth::{google::GoogleIdTokenPayload, jwt},
    config::{Config, extract_url_host, is_loopback_host},
    create_router,
    mailer::ConsoleMailer,
    models::{User, UserRole, has_premium},
    rate_limiter::RateLimiter,
};

async fn setup_test_app_with_mailer(mailer: Arc<dyn Mailer>) -> (Router, Arc<AppState>) {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/onedegree".to_string());
    let db_host = extract_url_host(&db_url).expect("Valid DATABASE_URL host");
    assert!(
        is_loopback_host(&db_host),
        "Tests refuse non-local DATABASE_URL host '{}'",
        db_host
    );

    let config = Config::try_from_lookup(|k| match k {
        "APP_ENV" => Some("test".to_string()),
        "DATABASE_URL" => Some(db_url.clone()),
        "CORS_ORIGIN" => Some("http://localhost:3000".to_string()),
        "MAILER" => Some("console".to_string()),
        "ADMIN_EMAILS" => Some("sohelhussaing@gmail.com".to_string()),
        _ => std::env::var(k).ok(),
    })
    .expect("Valid test configuration");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(0)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to local test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations in test");

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

    let app = create_router(state.clone());
    (app, state)
}

async fn setup_test_app() -> (Router, Arc<AppState>) {
    setup_test_app_with_mailer(Arc::new(ConsoleMailer::default())).await
}

async fn create_test_user(state: &AppState) -> User {
    let email = format!("test_{}@example.com", Uuid::new_v4());
    sqlx::query_as!(
        User,
        r#"
        INSERT INTO users (email, full_name, avatar_url, timezone, role)
        VALUES ($1, 'Test User', 'https://example.com/avatar.png', 'UTC', 'user')
        RETURNING id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
                  email_verified_at, role as "role: UserRole", is_premium, premium_until,
                  created_at, updated_at
        "#,
        email
    )
    .fetch_one(&state.pool)
    .await
    .expect("Failed to create test user")
}

#[tokio::test]
async fn test_token_rotation() {
    let (app, state) = setup_test_app().await;
    let user = create_test_user(&state).await;

    let raw_token_1 = jwt::generate_refresh_token();
    let hash_1 = jwt::hash_refresh_token(&raw_token_1);
    let expires_at = Utc::now() + Duration::days(30);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        user.id,
        hash_1,
        expires_at
    )
    .execute(&state.pool)
    .await
    .expect("Failed to insert refresh token");

    let req = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(COOKIE, format!("refresh_token={}", raw_token_1))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let set_cookie_header = res
        .headers()
        .get(SET_COOKIE)
        .expect("Missing Set-Cookie header")
        .to_str()
        .unwrap();
    assert!(set_cookie_header.contains("refresh_token="));
    assert!(set_cookie_header.contains("HttpOnly"));

    let raw_token_2 = set_cookie_header
        .split(';')
        .find(|s| s.trim().starts_with("refresh_token="))
        .unwrap()
        .trim()
        .strip_prefix("refresh_token=")
        .unwrap()
        .to_string();

    assert_ne!(raw_token_1, raw_token_2);

    let token_1_record = sqlx::query!(
        "SELECT revoked FROM refresh_tokens WHERE token_hash = $1",
        hash_1
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert!(token_1_record.revoked, "Old token must be marked revoked");

    let hash_2 = jwt::hash_refresh_token(&raw_token_2);
    let token_2_record = sqlx::query!(
        "SELECT revoked FROM refresh_tokens WHERE token_hash = $1",
        hash_2
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert!(!token_2_record.revoked, "New token must be active");
}

#[tokio::test]
async fn test_replayed_refresh_token_revokes_all_sessions() {
    let (app, state) = setup_test_app().await;
    let user = create_test_user(&state).await;

    let raw_token_1 = jwt::generate_refresh_token();
    let hash_1 = jwt::hash_refresh_token(&raw_token_1);
    let expires_at = Utc::now() + Duration::days(30);

    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        user.id,
        hash_1,
        expires_at
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(COOKIE, format!("refresh_token={}", raw_token_1))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let set_cookie = res.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
    let raw_token_2 = set_cookie
        .split(';')
        .find(|s| s.trim().starts_with("refresh_token="))
        .unwrap()
        .trim()
        .strip_prefix("refresh_token=")
        .unwrap()
        .to_string();

    // Replay old token
    let replay_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(COOKIE, format!("refresh_token={}", raw_token_1))
        .body(Body::empty())
        .unwrap();

    let replay_res = app.clone().oneshot(replay_req).await.unwrap();
    assert_eq!(replay_res.status(), StatusCode::UNAUTHORIZED);

    let body_bytes = replay_res.into_body().collect().await.unwrap().to_bytes();
    let json_body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(
        json_body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("revoked or replayed")
    );

    // Verify all tokens revoked
    let hash_2 = jwt::hash_refresh_token(&raw_token_2);
    let token_2_record = sqlx::query!(
        "SELECT revoked FROM refresh_tokens WHERE token_hash = $1",
        hash_2
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert!(token_2_record.revoked);
}

#[tokio::test]
async fn test_hash_never_returned() {
    let (app, state) = setup_test_app().await;
    let email = format!("nohash_{}@example.com", Uuid::new_v4());
    let password = "SuperSecurePassword123!";

    // 1. Register
    let reg_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email,
                "password": password,
                "full_name": "No Hash User"
            }))
            .unwrap(),
        ))
        .unwrap();

    let reg_res = app.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);
    let reg_body = reg_res.into_body().collect().await.unwrap().to_bytes();
    let reg_str = String::from_utf8_lossy(&reg_body);
    assert!(!reg_str.contains("password_hash"));
    assert!(!reg_str.contains("argon2"));

    // Verify user email before login
    sqlx::query!(
        "UPDATE users SET email_verified_at = now() WHERE LOWER(email) = $1",
        email.to_lowercase()
    )
    .execute(&state.pool)
    .await
    .unwrap();

    // 2. Login
    let login_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();

    let login_res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);
    let login_body = login_res.into_body().collect().await.unwrap().to_bytes();
    let login_json: Value = serde_json::from_slice(&login_body).unwrap();
    assert!(login_json["user"].get("password_hash").is_none());
    assert!(!String::from_utf8_lossy(&login_body).contains("password_hash"));

    let access_token = login_json["access_token"].as_str().unwrap();

    // 3. GET /v1/me
    let me_req = Request::builder()
        .method("GET")
        .uri("/v1/me")
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .body(Body::empty())
        .unwrap();

    let me_res = app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(me_res.status(), StatusCode::OK);
    let me_body = me_res.into_body().collect().await.unwrap().to_bytes();
    let me_json: Value = serde_json::from_slice(&me_body).unwrap();
    assert!(me_json.get("password_hash").is_none());
    assert!(!String::from_utf8_lossy(&me_body).contains("password_hash"));

    // 4. PATCH /v1/me
    let patch_req = Request::builder()
        .method("PATCH")
        .uri("/v1/me")
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "timezone": "Europe/London"
            }))
            .unwrap(),
        ))
        .unwrap();

    let patch_res = app.clone().oneshot(patch_req).await.unwrap();
    assert_eq!(patch_res.status(), StatusCode::OK);
    let patch_body = patch_res.into_body().collect().await.unwrap().to_bytes();
    let patch_json: Value = serde_json::from_slice(&patch_body).unwrap();
    assert!(patch_json.get("password_hash").is_none());
    assert!(!String::from_utf8_lossy(&patch_body).contains("password_hash"));
}

#[tokio::test]
async fn test_unverified_email_cannot_become_admin() {
    let (app, state) = setup_test_app().await;

    // Use admin email from config (sohelhussaing@gmail.com)
    let admin_email = "sohelhussaing@gmail.com";
    let password = "AdminPassword123!";

    // Delete existing record if any from previous test runs
    let _ = sqlx::query!("DELETE FROM users WHERE LOWER(email) = $1", admin_email)
        .execute(&state.pool)
        .await;

    // 1. Register with the admin email
    let reg_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": admin_email,
                "password": password,
                "full_name": "Admin Candidate"
            }))
            .unwrap(),
        ))
        .unwrap();

    let reg_res = app.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);

    // 2. Query user from DB: MUST be role='user', NOT 'admin' while unverified!
    let user = sqlx::query!(
        r#"SELECT role as "role: UserRole", email_verified_at FROM users WHERE LOWER(email) = $1"#,
        admin_email
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    assert_eq!(
        user.role,
        UserRole::User,
        "Unverified user MUST NOT have admin role"
    );
    assert!(
        user.email_verified_at.is_none(),
        "Email must be unverified initially"
    );

    // 3. Test RequireAdmin extractor rejection:
    let token_rec = sqlx::query!(
        r#"
        SELECT token_hash 
        FROM email_verification_tokens evt
        JOIN users u ON u.id = evt.user_id
        WHERE LOWER(u.email) = $1
        ORDER BY evt.created_at DESC LIMIT 1
        "#,
        admin_email
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    // 4. Verify email using raw token
    // We update email_verification_tokens directly or test /v1/auth/verify-email
    let raw_token = jwt::generate_refresh_token();
    let new_hash = jwt::hash_refresh_token(&raw_token);
    sqlx::query!(
        "UPDATE email_verification_tokens SET token_hash = $1 WHERE token_hash = $2",
        new_hash,
        token_rec.token_hash
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let verify_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/verify-email")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "token": raw_token
            }))
            .unwrap(),
        ))
        .unwrap();

    let verify_res = app.clone().oneshot(verify_req).await.unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    // 5. Query user again: NOW role must be 'admin' because email was verified!
    let verified_user = sqlx::query!(
        r#"SELECT role as "role: UserRole", email_verified_at FROM users WHERE LOWER(email) = $1"#,
        admin_email
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    assert_eq!(
        verified_user.role,
        UserRole::Admin,
        "Verified admin email MUST become admin"
    );
    assert!(verified_user.email_verified_at.is_some());
}

#[tokio::test]
async fn test_admin_has_premium() {
    let (app, state) = setup_test_app().await;

    // 1. Test unit function has_premium
    let admin_user = User {
        id: Uuid::new_v4(),
        email: Some("admin@example.com".into()),
        password_hash: None,
        full_name: Some("Admin".into()),
        avatar_url: None,
        timezone: "UTC".into(),
        coin_balance: 0,
        email_verified_at: Some(Utc::now()),
        role: UserRole::Admin,
        is_premium: false, // Notice false!
        premium_until: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(
        has_premium(&admin_user),
        "Admin MUST automatically have has_premium == true"
    );

    let normal_user = User {
        role: UserRole::User,
        is_premium: false,
        ..admin_user.clone()
    };
    assert!(
        !has_premium(&normal_user),
        "Non-premium normal user must have has_premium == false"
    );

    let premium_normal_user = User {
        role: UserRole::User,
        is_premium: true,
        premium_until: Some(Utc::now() + Duration::days(10)),
        ..admin_user.clone()
    };
    assert!(
        has_premium(&premium_normal_user),
        "Active premium user must have has_premium == true"
    );

    let expired_premium_user = User {
        role: UserRole::User,
        is_premium: true,
        premium_until: Some(Utc::now() - Duration::days(1)),
        ..admin_user.clone()
    };
    assert!(
        !has_premium(&expired_premium_user),
        "Expired premium user must have has_premium == false"
    );

    // 2. Test via GET /v1/me
    let db_admin = sqlx::query_as!(
        User,
        r#"
        INSERT INTO users (email, full_name, timezone, role, email_verified_at, is_premium)
        VALUES ($1, 'Live Admin', 'UTC', 'admin', now(), false)
        RETURNING id, email, password_hash, full_name, avatar_url, timezone, coin_balance,
                  email_verified_at, role as "role: UserRole", is_premium, premium_until,
                  created_at, updated_at
        "#,
        format!("db_admin_{}@example.com", Uuid::new_v4())
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    let access_token =
        jwt::create_access_token(db_admin.id, state.config.jwt_secret.as_bytes(), 900).unwrap();

    let me_req = Request::builder()
        .method("GET")
        .uri("/v1/me")
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .body(Body::empty())
        .unwrap();

    let me_res = app.clone().oneshot(me_req).await.unwrap();
    assert_eq!(me_res.status(), StatusCode::OK);
    let me_body = me_res.into_body().collect().await.unwrap().to_bytes();
    let me_json: Value = serde_json::from_slice(&me_body).unwrap();

    assert_eq!(me_json["role"].as_str().unwrap(), "admin");
    assert!(me_json["has_premium"].as_bool().unwrap());
}

#[tokio::test]
async fn test_duplicate_email_rejected() {
    let (app, state) = setup_test_app().await;
    let email = format!("dupe_{}@example.com", Uuid::new_v4());
    let password = "TestPassword123!";

    // 1. Initial registration
    let reg1 = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();

    let res1 = app.clone().oneshot(reg1).await.unwrap();
    assert_eq!(res1.status(), StatusCode::CREATED);

    // 2. Duplicate registration with exact same email
    let reg2 = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();

    let res2 = app.clone().oneshot(reg2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::BAD_REQUEST);

    // 3. Duplicate registration with UPPERCASE email
    let reg3 = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email.to_uppercase(),
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();

    let res3 = app.clone().oneshot(reg3).await.unwrap();
    assert_eq!(res3.status(), StatusCode::BAD_REQUEST);

    // 4. Email already belongs to a Google account
    let google_email = format!("google_{}@example.com", Uuid::new_v4());
    let g_user = sqlx::query!(
        "INSERT INTO users (email, timezone) VALUES ($1, 'UTC') RETURNING id",
        google_email
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO auth_identities (user_id, provider, provider_user_id) VALUES ($1, 'google', $2)",
        g_user.id,
        format!("sub_{}", Uuid::new_v4())
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let reg4 = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": google_email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();

    let res4 = app.clone().oneshot(reg4).await.unwrap();
    assert_eq!(res4.status(), StatusCode::BAD_REQUEST);
    let body4 = res4.into_body().collect().await.unwrap().to_bytes();
    let json4: Value = serde_json::from_slice(&body4).unwrap();
    assert!(
        json4["error"]["message"]
            .as_str()
            .unwrap()
            .contains("sign in with Google")
    );
}

#[tokio::test]
async fn test_duration_check() {
    let (app, state) = setup_test_app().await;
    let user = create_test_user(&state).await;

    let goal_id = sqlx::query_scalar!(
        "INSERT INTO goals (user_id, title) VALUES ($1, 'Fitness') RETURNING id",
        user.id
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    // 1. duration_minutes = 0 violates CHECK (between 1 and 600)
    let bad_action_0 = sqlx::query!(
        "INSERT INTO daily_actions (goal_id, user_id, title, duration_minutes) VALUES ($1, $2, 'Pushups', 0)",
        goal_id,
        user.id
    )
    .execute(&state.pool)
    .await;
    assert!(
        bad_action_0.is_err(),
        "duration_minutes = 0 must violate check constraint"
    );

    // 2. duration_minutes = 601 violates CHECK
    let bad_action_601 = sqlx::query!(
        "INSERT INTO daily_actions (goal_id, user_id, title, duration_minutes) VALUES ($1, $2, 'Pushups', 601)",
        goal_id,
        user.id
    )
    .execute(&state.pool)
    .await;
    assert!(
        bad_action_601.is_err(),
        "duration_minutes = 601 must violate check constraint"
    );

    // 3. Valid duration_minutes = 30 succeeds
    let good_action = sqlx::query!(
        "INSERT INTO daily_actions (goal_id, user_id, title, duration_minutes) VALUES ($1, $2, 'Pushups', 30) RETURNING id",
        goal_id,
        user.id
    )
    .fetch_one(&state.pool)
    .await;
    assert!(good_action.is_ok(), "duration_minutes = 30 must succeed");

    // 4. Todo duration check
    let today = Utc::now().date_naive();
    let bad_todo = sqlx::query!(
        "INSERT INTO todos (user_id, title, target_date, duration_minutes) VALUES ($1, 'Read', $2, -1)",
        user.id,
        today
    )
    .execute(&state.pool)
    .await;
    assert!(
        bad_todo.is_err(),
        "todo duration_minutes = -1 must fail check constraint"
    );

    let good_todo = sqlx::query!(
        "INSERT INTO todos (user_id, title, target_date, duration_minutes) VALUES ($1, 'Read', $2, 45) RETURNING id",
        user.id,
        today
    )
    .fetch_one(&state.pool)
    .await;
    assert!(good_todo.is_ok(), "todo duration_minutes = 45 must succeed");

    // 5. Test GET /v1/home calculates planned minutes (30 + 45 = 75)
    let access_token =
        jwt::create_access_token(user.id, state.config.jwt_secret.as_bytes(), 900).unwrap();

    let home_req = Request::builder()
        .method("GET")
        .uri(format!("/v1/home?date={}", today))
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .body(Body::empty())
        .unwrap();

    let home_res = app.clone().oneshot(home_req).await.unwrap();
    assert_eq!(home_res.status(), StatusCode::OK);
    let home_body = home_res.into_body().collect().await.unwrap().to_bytes();
    let home_json: Value = serde_json::from_slice(&home_body).unwrap();

    assert_eq!(home_json["planned_minutes"].as_i64().unwrap(), 75);
    assert_eq!(home_json["completed_minutes"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn test_cors_allowed_origin_and_preflight() {
    let (app, _) = setup_test_app().await;

    // 1. Preflight OPTIONS request
    let preflight_req = Request::builder()
        .method("OPTIONS")
        .uri("/v1/auth/refresh")
        .header(ORIGIN, "http://localhost:3000")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type")
        .body(Body::empty())
        .unwrap();

    let preflight_res = app.clone().oneshot(preflight_req).await.unwrap();
    assert!(
        preflight_res.status().is_success(),
        "preflight OPTIONS should succeed, got {}",
        preflight_res.status()
    );

    let headers = preflight_res.headers();
    assert_eq!(
        headers.get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
    assert_eq!(
        headers.get("access-control-allow-credentials").unwrap(),
        "true"
    );
    assert!(headers.contains_key("access-control-allow-methods"));
    assert!(headers.contains_key("access-control-allow-headers"));
    assert!(headers.contains_key("access-control-max-age"));
    let allow_origin = headers
        .get("access-control-allow-origin")
        .unwrap()
        .to_str()
        .unwrap();
    assert_ne!(
        allow_origin, "*",
        "wildcard must never be used in access-control-allow-origin"
    );

    // 2. Simple GET request with allowed origin
    let get_req = Request::builder()
        .method("GET")
        .uri("/health")
        .header(ORIGIN, "http://localhost:3000")
        .body(Body::empty())
        .unwrap();

    let get_res = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);
    let get_headers = get_res.headers();
    assert_eq!(
        get_headers.get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
    assert_eq!(
        get_headers.get("access-control-allow-credentials").unwrap(),
        "true"
    );
    assert!(
        get_headers
            .get("vary")
            .unwrap()
            .to_str()
            .unwrap()
            .to_lowercase()
            .contains("origin")
    );
}

#[tokio::test]
async fn test_cors_evil_origin_gets_no_cors_headers() {
    let (app, _) = setup_test_app().await;

    let evil_req = Request::builder()
        .method("GET")
        .uri("/health")
        .header(ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();

    let evil_res = app.oneshot(evil_req).await.unwrap();
    let headers = evil_res.headers();
    assert!(
        headers.get("access-control-allow-origin").is_none(),
        "evil origin must not receive access-control-allow-origin header"
    );
    assert!(
        headers.get("access-control-allow-credentials").is_none(),
        "evil origin must not receive access-control-allow-credentials header"
    );
}

#[tokio::test]
async fn test_cors_outermost_layer_error_responses() {
    let (app, _) = setup_test_app().await;

    // 1. 401 from /v1/auth/refresh with no cookie from allowed origin
    let req_401 = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(ORIGIN, "http://localhost:3000")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let res_401 = app.clone().oneshot(req_401).await.unwrap();
    assert_eq!(res_401.status(), StatusCode::UNAUTHORIZED);
    let headers_401 = res_401.headers();
    assert_eq!(
        headers_401.get("access-control-allow-origin").unwrap(),
        "http://localhost:3000",
        "401 error response from allowed origin must include CORS allow-origin"
    );
    assert_eq!(
        headers_401.get("access-control-allow-credentials").unwrap(),
        "true",
        "401 error response from allowed origin must include credentials header"
    );

    // 2. 403 from /v1/auth/refresh with foreign origin
    let req_403 = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(ORIGIN, "https://evil.example")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let res_403 = app.clone().oneshot(req_403).await.unwrap();
    assert_eq!(res_403.status(), StatusCode::FORBIDDEN);
    let headers_403 = res_403.headers();
    assert!(
        headers_403.get("access-control-allow-origin").is_none(),
        "foreign origin must not receive access-control-allow-origin on 403 response"
    );
    assert!(
        headers_403
            .get("access-control-allow-credentials")
            .is_none(),
        "foreign origin must not receive access-control-allow-credentials on 403 response"
    );
}

#[tokio::test]
async fn test_csrf_foreign_origin_rejected_and_token_preserved() {
    let (app, state) = setup_test_app().await;

    // 1. Register a test user
    let email = format!("csrf_test_{}@example.com", Uuid::new_v4());

    let user = sqlx::query!(
        "INSERT INTO users (email, timezone) VALUES ($1, 'UTC') RETURNING id",
        email
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    // 2. Insert an active refresh token
    let raw_refresh = jwt::generate_refresh_token();
    let token_hash = jwt::hash_refresh_token(&raw_refresh);
    let expires_at = Utc::now() + Duration::seconds(state.config.jwt_refresh_expiration_secs);

    let token_record = sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        RETURNING id, revoked
        "#,
        user.id,
        token_hash,
        expires_at
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    assert!(!token_record.revoked, "token must initially be active");

    // 3. Foreign Origin (CSRF attack) attempts refresh
    let attack_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(ORIGIN, "https://evil.example")
        .header(COOKIE, format!("refresh_token={}", raw_refresh))
        .body(Body::empty())
        .unwrap();

    let attack_res = app.clone().oneshot(attack_req).await.unwrap();
    assert_eq!(
        attack_res.status(),
        StatusCode::FORBIDDEN,
        "foreign origin refresh must return 403 Forbidden"
    );

    // 4. Assert token was NOT rotated or revoked in database
    let checked_token = sqlx::query!(
        "SELECT revoked FROM refresh_tokens WHERE id = $1",
        token_record.id
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    assert!(
        !checked_token.revoked,
        "refresh token must remain valid and unrevoked after rejected CSRF attempt"
    );

    // 5. Legitimate request from allowed origin succeeds and rotates token
    let legit_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(ORIGIN, "http://localhost:3000")
        .header(COOKIE, format!("refresh_token={}", raw_refresh))
        .body(Body::empty())
        .unwrap();

    let legit_res = app.clone().oneshot(legit_req).await.unwrap();
    assert_eq!(
        legit_res.status(),
        StatusCode::OK,
        "legitimate refresh after blocked attack must succeed"
    );

    // 6. Old token is now rotated (marked revoked)
    let post_legit_token = sqlx::query!(
        "SELECT revoked FROM refresh_tokens WHERE id = $1",
        token_record.id
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();

    assert!(
        post_legit_token.revoked,
        "token must be rotated and marked revoked after legitimate refresh"
    );

    // 7. Test logout CSRF rejection
    let attack_logout_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/logout")
        .header(ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();

    let attack_logout_res = app.oneshot(attack_logout_req).await.unwrap();
    assert_eq!(
        attack_logout_res.status(),
        StatusCode::FORBIDDEN,
        "foreign origin logout must return 403 Forbidden"
    );
}

#[derive(Clone, Default)]
struct RecordingMailer {
    sent_verification: Arc<Mutex<Vec<(String, String)>>>,
    sent_reset: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait::async_trait]
impl Mailer for RecordingMailer {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        self.sent_verification
            .lock()
            .await
            .push((to.to_string(), token.to_string()));
        Ok(())
    }

    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        self.sent_reset
            .lock()
            .await
            .push((to.to_string(), token.to_string()));
        Ok(())
    }
}

#[tokio::test]
async fn test_unverified_login_refused_and_generic_401() {
    let mailer = Arc::new(RecordingMailer::default());
    let (app, _state) = setup_test_app_with_mailer(mailer.clone()).await;

    let email = format!("unverified_{}@example.test", Uuid::new_v4());
    let password = "ValidPassword123!";

    let reg_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": password,
                "full_name": "Unverified Tester"
            }))
            .unwrap(),
        ))
        .unwrap();
    let reg_res = app.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);

    // 1. Correct password on unverified account -> 403 EMAIL_NOT_VERIFIED, no cookie, no token
    let unverified_login = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();
    let res_unv = app.clone().oneshot(unverified_login).await.unwrap();
    assert_eq!(res_unv.status(), StatusCode::FORBIDDEN);
    assert!(
        res_unv.headers().get(SET_COOKIE).is_none(),
        "unverified login must not set refresh cookie"
    );
    let body_unv: Value =
        serde_json::from_slice(&res_unv.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_unv["error"]["code"], "EMAIL_NOT_VERIFIED");
    assert!(body_unv.get("access_token").is_none());

    // 2. Wrong password returns exact same generic 401 as unknown email
    let wrong_pw = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": "WrongPassword999!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let res_wrong = app.clone().oneshot(wrong_pw).await.unwrap();
    assert_eq!(res_wrong.status(), StatusCode::UNAUTHORIZED);
    let body_wrong: Value =
        serde_json::from_slice(&res_wrong.into_body().collect().await.unwrap().to_bytes()).unwrap();

    let unknown_email = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": format!("missing_{}@example.test", Uuid::new_v4()),
                "password": "WrongPassword999!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let res_unknown = app.clone().oneshot(unknown_email).await.unwrap();
    assert_eq!(res_unknown.status(), StatusCode::UNAUTHORIZED);
    let body_unknown: Value =
        serde_json::from_slice(&res_unknown.into_body().collect().await.unwrap().to_bytes())
            .unwrap();

    assert_eq!(
        body_wrong["error"]["message"],
        body_unknown["error"]["message"]
    );
    assert_eq!(body_wrong["error"]["message"], "Invalid email or password");
}

#[tokio::test]
async fn test_resend_verification_generic_cooldown_invalidation_and_rate_limit() {
    let mailer = Arc::new(RecordingMailer::default());
    let (app, state) = setup_test_app_with_mailer(mailer.clone()).await;

    let email = format!("resend_{}@example.test", Uuid::new_v4());
    let password = "ValidPassword123!";

    // Register -> token #1 dispatched
    let reg_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email, "password": password })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(reg_req).await.unwrap().status(),
        StatusCode::CREATED
    );

    let first_token = mailer.sent_verification.lock().await[0].1.clone();

    // Non-existent email returns exact same generic 200
    let unknown_resend = Request::builder()
        .method("POST")
        .uri("/v1/auth/resend-verification")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": "nobody@example.test" })).unwrap(),
        ))
        .unwrap();
    let res_unknown = app.clone().oneshot(unknown_resend).await.unwrap();
    assert_eq!(res_unknown.status(), StatusCode::OK);
    let json_unknown: Value =
        serde_json::from_slice(&res_unknown.into_body().collect().await.unwrap().to_bytes())
            .unwrap();

    // Resend within 60s cooldown -> returns same 200 generic message, sends NO new email
    let cooldown_resend = Request::builder()
        .method("POST")
        .uri("/v1/auth/resend-verification")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email })).unwrap(),
        ))
        .unwrap();
    let res_cool = app.clone().oneshot(cooldown_resend).await.unwrap();
    assert_eq!(res_cool.status(), StatusCode::OK);
    let json_cool: Value =
        serde_json::from_slice(&res_cool.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json_unknown["message"], json_cool["message"]);
    assert_eq!(
        mailer.sent_verification.lock().await.len(),
        1,
        "60s cooldown must suppress issuing a second email"
    );

    // Move latest token created_at back 65 seconds -> cooldown elapsed
    sqlx::query!(
        r#"
        UPDATE email_verification_tokens evt
        SET created_at = now() - interval '65 seconds'
        FROM users u
        WHERE evt.user_id = u.id AND LOWER(u.email) = $1
        "#,
        email.to_lowercase()
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let resend_after = Request::builder()
        .method("POST")
        .uri("/v1/auth/resend-verification")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email })).unwrap(),
        ))
        .unwrap();
    let res_after = app.clone().oneshot(resend_after).await.unwrap();
    assert_eq!(res_after.status(), StatusCode::OK);
    assert_eq!(mailer.sent_verification.lock().await.len(), 2);

    let second_token = mailer.sent_verification.lock().await[1].1.clone();
    assert_ne!(first_token, second_token);

    // Old token #1 must now be invalidated (400)
    let verify_old = Request::builder()
        .method("POST")
        .uri("/v1/auth/verify-email")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "token": first_token })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(verify_old).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    // New token #2 verifies email (200)
    let verify_new = Request::builder()
        .method("POST")
        .uri("/v1/auth/verify-email")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "token": second_token })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(verify_new).await.unwrap().status(),
        StatusCode::OK
    );

    // Login now succeeds with 200
    let login_ok = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email, "password": password })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(login_ok).await.unwrap().status(),
        StatusCode::OK
    );

    // Per-email rate limit on resend-verification (5 attempts/15min; already sent 2 for `email`)
    for _ in 0..3 {
        let r = Request::builder()
            .method("POST")
            .uri("/v1/auth/resend-verification")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "email": email })).unwrap(),
            ))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(r).await.unwrap().status(),
            StatusCode::OK
        );
    }
    let rate_limited_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/resend-verification")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.oneshot(rate_limited_req).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn test_expired_tokens_rejected_and_reset_revokes_sessions() {
    let mailer = Arc::new(RecordingMailer::default());
    let (app, state) = setup_test_app_with_mailer(mailer.clone()).await;

    let email = format!("tokens_{}@example.test", Uuid::new_v4());
    let password = "OldPassword123!";

    let reg_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email, "password": password })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(reg_req).await.unwrap().status(),
        StatusCode::CREATED
    );
    let ver_token = mailer.sent_verification.lock().await[0].1.clone();

    // Expire verification token -> rejected
    sqlx::query!(
        "UPDATE email_verification_tokens SET expires_at = now() - interval '5 minutes' WHERE token_hash = $1",
        jwt::hash_refresh_token(&ver_token)
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let verify_exp = Request::builder()
        .method("POST")
        .uri("/v1/auth/verify-email")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "token": ver_token })).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(verify_exp).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    // Mark user verified, log in to obtain active refresh token, then test forgot/reset
    sqlx::query!(
        "UPDATE users SET email_verified_at = now() WHERE LOWER(email) = $1",
        email.to_lowercase()
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let login_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "email": email, "password": password })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);
    let cookie_hdr = login_res
        .headers()
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let old_refresh = cookie_hdr
        .split(';')
        .find(|s| s.trim().starts_with("refresh_token="))
        .unwrap()
        .trim()
        .strip_prefix("refresh_token=")
        .unwrap()
        .to_string();

    // Trigger forgot password twice -> old reset token invalidated, new reset token works
    for _ in 0..2 {
        let fp = Request::builder()
            .method("POST")
            .uri("/v1/auth/forgot-password")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "email": email })).unwrap(),
            ))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(fp).await.unwrap().status(),
            StatusCode::OK
        );
    }

    let resets = mailer.sent_reset.lock().await;
    let reset_old = resets[0].1.clone();
    let reset_new = resets[1].1.clone();
    drop(resets);

    // Old reset token invalid
    let use_old_reset = Request::builder()
        .method("POST")
        .uri("/v1/auth/reset-password")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "token": reset_old, "new_password": "NewPassword123!" }))
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(use_old_reset).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    // Valid reset token succeeds and revokes old refresh session
    let use_new_reset = Request::builder()
        .method("POST")
        .uri("/v1/auth/reset-password")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "token": reset_new, "new_password": "NewPassword123!" }))
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(use_new_reset).await.unwrap().status(),
        StatusCode::OK
    );

    // Reusing reset token fails
    let reuse_reset = Request::builder()
        .method("POST")
        .uri("/v1/auth/reset-password")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "token": reset_new, "new_password": "NewPassword123!" }))
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(reuse_reset).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );

    // Old refresh cookie must now be rejected (revoked)
    let refresh_after_reset = Request::builder()
        .method("POST")
        .uri("/v1/auth/refresh")
        .header(ORIGIN, "http://localhost:3000")
        .header(COOKIE, format!("refresh_token={}", old_refresh))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(refresh_after_reset).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn test_login_rate_limit() {
    let (app, _state) = setup_test_app().await;
    for _ in 0..10 {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/auth/login")
            .header(CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", "203.0.113.77")
            .body(Body::from(
                serde_json::to_vec(
                    &json!({ "email": "nobody@example.test", "password": "BadPassword1!" }),
                )
                .unwrap(),
            ))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let blocked = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "203.0.113.77")
        .body(Body::from(
            serde_json::to_vec(
                &json!({ "email": "nobody@example.test", "password": "BadPassword1!" }),
            )
            .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.oneshot(blocked).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn test_google_jwks_override_validates_aud_iss_exp_and_creates_verified_user() {
    const TEST_RSA_N_B64: &str = "qZuuEO-RGc3V6tmvuf5gCYVcJ27K6LTccbJLuDDtdrB128ZS33tUkZOHJ7fmJc0Rev-qgrPxr5nNEipRzcEG-vicxtWrqTWfoLnjFnOKmXAdvwraJ10lY7EtdX2CI8DkzEROHl57IvZPUb80GBo_kMueR5BXPPbhgQp33fBkU13ItjYSQ0ccEsFBNvIb4G7bsIPsxvW9Y0TEHjHSjKk1B_ioCNEHiYO0spQ5l8qEKtwf9Rql9bnFyhWHnBSAdCnBF-8sXdbXrNcRO4gl268b9eP4JRWa6DHqUgygF2mOAXKccJeKX_WnrPyiQTwL0YdkKIOAvBehC3aeA-PDa5AMUw";
    const TEST_RSA_E_B64: &str = "AQAB";
    const TEST_RSA_PRIVATE_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCpm64Q75EZzdXq\n2a+5/mAJhVwnbsrotNxxsku4MO12sHXbxlLfe1SRk4cnt+YlzRF6/6qCs/Gvmc0S\nKlHNwQb6+JzG1aupNZ+gueMWc4qZcB2/CtonXSVjsS11fYIjwOTMRE4eXnsi9k9R\nvzQYGj+Qy55HkFc89uGBCnfd8GRTXci2NhJDRxwSwUE28hvgbtuwg+zG9b1jRMQe\nMdKMqTUH+KgI0QeJg7SylDmXyoQq3B/1GqX1ucXKFYecFIB0KcEX7yxd1tes1xE7\niCXbrxv14/glFZroMepSDKAXaY4Bcpxwl4pf9aes/KJBPAvRh2Qog4C8F6ELdp4D\n48NrkAxTAgMBAAECggEAUlHRt72W5vfNAloanML2WNil5Cu1hyG1NufLMYBp2OgH\npB5LfdWNs8RvXls29JTmDZVGbH6q6MhUzKLGpn8wBKKi8Symzw9MRUXDjtOQay2L\n+nvpax6d9Zd6kWJTxkeqpOvoIHxJ9qumzu3m+mXPAVeaoHBRrzDCPY/Xotw4zW7W\nHeHaW2URA+VzW6DSZVN03sJD8tbVcmo2feK9kBtuJZSEsTKD6RCHQMFtpxx7rXFk\n3hTWKbgEW2NFkoLIpsv3Dkco9bWQ1T+FgDLaC2aXsOwszrqq0KrDC6JiwXdcPKpj\n9c6Ju6iJPYWS6DMfrtYuj9iKGNodQtGI4kevlQakkQKBgQDf92ybweThuc/ZEQ9o\ntTvkCK0hPvI+fA4pDIHV7rvw8Gz060W3Lu9XUkmT9GlTKY8rDNrKTXCAvWTZK/vS\nqzUvSF+wTZvU9srwKJGsluUzd3DbUqmxdxjOGlxY0xzToCNs8pyOb1EPrhAALgNH\nMuL8Zexsy9Lz9oAUA7b4/ttrowKBgQDB3enZz0hRTRSNFZtP/nsMmF+Xjzfwc+TX\nV0tM94sNXUgd9xF63Av8bwjMNW23UDyGGrW0kB7CO0F7+T8OIgcHeyQ9rdqETNS1\nIV282/K9Y0rON2C9nOIs/7ClPQPj+hIK6FhOaXfFMqW5r6ps1rTw6qxXconEx74Y\nfapW0rjnkQKBgCj/pR0aMGj118uXqOd1DcKfp/nHCRdRJFJ1XKsp6bOgf4GSNA/l\ngqDMinWQL1uu5uttjfusk2StxvFOcOnOt4yxRfsDKfyhNOMjug8LDGgQfFNZ3YCE\nGcFY6ADGnKdM0E9YVjpovEEPjxRJWAZHlW+4xL/hy/sm66k+SNdp6AbbAoGALu2A\nxfCfGM8Hwn8QcXSwZIQd35oTeGkVY+sFl0P0rO9/nYFJAexyGlDmWEg2nsEVZYDh\nqyuXxnyBhXj4AnvKJ81vIF8NHila8913y1eSkrWOTgyppjxro2jhYZ47atwFsg48\nEf8Ftv5jj2cuYdo3QnoL7b8UCkYvhc4lhY7Mo4ECgYAnYP23p91VcPfZribEI5FT\n6JJys7r1eqU7TkAbb4jizb0JPlCRJAld62i5qu01lJLLz3XbK7Bly8tFOZn1UFLi\nLQpYUhHNuuX9QAkFUGXIHI6kGKcc6Dgrz9hDeb11oEGwoWg0d+26PmvOMKKXENwT\nV/p2bEExzTXN82YC1dRMyg==\n-----END PRIVATE KEY-----";

    async fn jwks_handler() -> Json<Value> {
        Json(json!({
            "keys": [{
                "kid": "test-kid-1",
                "kty": "RSA",
                "alg": "RS256",
                "n": TEST_RSA_N_B64,
                "e": TEST_RSA_E_B64
            }]
        }))
    }

    let jwks_app = Router::new().route("/certs", get(jwks_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jwks_server = tokio::spawn(async move {
        axum::serve(listener, jwks_app).await.unwrap();
    });

    let jwks_url = format!("http://{}/certs", addr);
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/onedegree".to_string());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .unwrap();

    let env_map = HashMap::from([
        ("APP_ENV", "test"),
        ("DATABASE_URL", db_url.as_str()),
        ("CORS_ORIGIN", "http://localhost:3000"),
        (
            "GOOGLE_CLIENT_ID",
            "test-google-client.apps.googleusercontent.com",
        ),
        ("GOOGLE_JWKS_URL", jwks_url.as_str()),
    ]);
    let config = Config::try_from_lookup(|k| env_map.get(k).map(|v| (*v).to_string())).unwrap();

    let state = Arc::new(AppState {
        pool: pool.clone(),
        config: config.clone(),
        google_verifier: GoogleVerifier::with_jwks_url(
            config.google_client_id.clone(),
            config.google_jwks_url.clone(),
        ),
        mailer: Arc::new(ConsoleMailer::default()),
        rate_limiter: RateLimiter::new(),
    });
    let app = create_router(state);

    let enc_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_PEM.as_bytes()).unwrap();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-kid-1".to_string());
    let now_ts = Utc::now().timestamp() as usize;

    // 1. Valid token creates verified user
    let g_email = format!("google_mock_{}@example.test", Uuid::new_v4());
    let valid_payload = GoogleIdTokenPayload {
        iss: "https://accounts.google.com".into(),
        aud: config.google_client_id.clone(),
        sub: format!("sub_{}", Uuid::new_v4()),
        exp: now_ts + 300,
        email: Some(g_email.clone()),
        email_verified: Some(true),
        name: Some("Mock Google User".into()),
        picture: Some("https://lh3.googleusercontent.com/a/avatar".into()),
    };
    let valid_token = encode(&header, &valid_payload, &enc_key).unwrap();
    let res_ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/google")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "id_token": valid_token })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_ok.status(), StatusCode::OK);
    let body_ok: Value =
        serde_json::from_slice(&res_ok.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_ok["user"]["email"], g_email);
    assert_eq!(body_ok["user"]["email_verified"], true);

    // 2. Wrong aud rejected
    let bad_aud = encode(
        &header,
        &GoogleIdTokenPayload {
            aud: "other-client.apps.googleusercontent.com".into(),
            ..valid_payload.clone()
        },
        &enc_key,
    )
    .unwrap();
    let res_aud = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/google")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "id_token": bad_aud })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_aud.status(), StatusCode::UNAUTHORIZED);

    // 3. Wrong iss rejected
    let bad_iss = encode(
        &header,
        &GoogleIdTokenPayload {
            iss: "https://evil.example".into(),
            ..valid_payload.clone()
        },
        &enc_key,
    )
    .unwrap();
    let res_iss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/google")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "id_token": bad_iss })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_iss.status(), StatusCode::UNAUTHORIZED);

    // 4. Expired token rejected
    let expired = encode(
        &header,
        &GoogleIdTokenPayload {
            exp: now_ts.saturating_sub(300),
            ..valid_payload
        },
        &enc_key,
    )
    .unwrap();
    let res_exp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/google")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "id_token": expired })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_exp.status(), StatusCode::UNAUTHORIZED);

    let _ = Extension::<Value>;
    jwks_server.abort();
}
