use axum::{
    Router,
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, SET_COOKIE},
    },
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use api::{
    AppState, GoogleVerifier,
    auth::jwt,
    config::Config,
    create_router,
    mailer::ConsoleMailer,
    models::{User, UserRole, has_premium},
    rate_limiter::RateLimiter,
};

async fn setup_test_app() -> (Router, Arc<AppState>) {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env();

    // Tests must strictly only use the local Docker database
    let local_db_url = std::env::var("LOCAL_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/onedegree".to_string());
    config.database_url = local_db_url.clone();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(0)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .max_connections(5)
        .connect(&local_db_url)
        .await
        .expect("Failed to connect to local test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations in test");

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

    let app = create_router(state.clone());
    (app, state)
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
    let (app, _state) = setup_test_app().await;
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
    assert_eq!(me_json["has_premium"].as_bool().unwrap(), true);
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
