use async_trait::async_trait;
use chrono::Utc;
use resend_rs::types::CreateEmailBaseOptions;
use resend_rs::{ConfigBuilder, Resend};
use serde_json::json;
use std::fs::{OpenOptions, Permissions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::config::{AppEnv, Config, MailerKind};

const DEFAULT_RESEND_TIMEOUT: Duration = Duration::from_secs(10);

pub fn extract_email_domain(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('>').trim();
    trimmed
        .rsplit_once('@')
        .map(|(_, dom)| dom.trim().to_string())
        .filter(|dom| !dom.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn startup_mailer_notice(config: &Config) -> String {
    match config.mailer {
        MailerKind::Console => {
            "MAILER=console: no real emails are sent; verification and reset links are printed in this terminal".to_string()
        }
        MailerKind::Resend => {
            let domain = extract_email_domain(config.mail_from.as_deref().unwrap_or(""));
            format!("mail via Resend, from {}", domain)
        }
        MailerKind::File => {
            format!("MAILER=file: writing test emails to {}", config.mail_outbox_path)
        }
    }
}

fn redact_tokens_and_keys(input: &str, api_key: &str) -> String {
    let mut out = if !api_key.trim().is_empty() {
        input.replace(api_key.trim(), "[REDACTED_KEY]")
    } else {
        input.to_string()
    };

    // Redact any Resend-style API key fragments if echoed
    let mut redacted_keys = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(pos) = rest.find("re_") {
        redacted_keys.push_str(&rest[..pos]);
        let after = &rest[pos + 3..];
        let token_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(after.len());
        if token_len >= 6 {
            redacted_keys.push_str("re_[REDACTED]");
        } else {
            redacted_keys.push_str(&rest[pos..pos + 3 + token_len]);
        }
        rest = &after[token_len..];
    }
    redacted_keys.push_str(rest);
    out = redacted_keys;

    // Redact any query token= values or verification/reset links if echoed
    let mut redacted_tokens = String::with_capacity(out.len());
    let mut rem = out.as_str();
    while let Some(pos) = rem.find("token=") {
        redacted_tokens.push_str(&rem[..pos + 6]);
        let after = &rem[pos + 6..];
        let tok_end = after
            .find(|c: char| c.is_whitespace() || matches!(c, '&' | '"' | '\'' | '<' | '>'))
            .unwrap_or(after.len());
        redacted_tokens.push_str("[REDACTED_TOKEN]");
        rem = &after[tok_end..];
    }
    redacted_tokens.push_str(rem);
    redacted_tokens
}

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String>;
    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String>;
}

pub fn verification_link(web_base_url: &str, token: &str) -> String {
    format!(
        "{}/verify-email?token={}",
        web_base_url.trim_end_matches('/'),
        token
    )
}

pub fn password_reset_link(web_base_url: &str, token: &str) -> String {
    format!(
        "{}/reset-password?token={}",
        web_base_url.trim_end_matches('/'),
        token
    )
}

#[derive(Clone)]
pub struct ConsoleMailer {
    web_base_url: String,
}

impl ConsoleMailer {
    pub fn new(web_base_url: impl Into<String>) -> Self {
        Self {
            web_base_url: web_base_url.into(),
        }
    }
}

impl Default for ConsoleMailer {
    fn default() -> Self {
        Self {
            web_base_url: "http://localhost:3000".to_string(),
        }
    }
}

#[async_trait]
impl Mailer for ConsoleMailer {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        let link = verification_link(&self.web_base_url, token);
        info!(
            "\n========================================================================\n\
             [MAILER: VERIFICATION EMAIL]\n\
             To: {}\n\
             Verification Link: {}\n\
             ========================================================================",
            to, link
        );
        Ok(())
    }

    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        let link = password_reset_link(&self.web_base_url, token);
        info!(
            "\n========================================================================\n\
             [MAILER: PASSWORD RESET EMAIL]\n\
             To: {}\n\
             Reset Link: {}\n\
             ========================================================================",
            to, link
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct FileMailer {
    outbox_path: PathBuf,
    web_base_url: String,
}

impl FileMailer {
    pub fn new(
        app_env: AppEnv,
        outbox_path: PathBuf,
        web_base_url: impl Into<String>,
    ) -> Result<Self, String> {
        if !matches!(app_env, AppEnv::Local | AppEnv::Test) {
            return Err("FileMailer is permitted only when APP_ENV is test or local".to_string());
        }
        Ok(Self {
            outbox_path,
            web_base_url: web_base_url.into(),
        })
    }

    fn append_entry(
        &self,
        email_type: &str,
        to: &str,
        subject: &str,
        token: &str,
        url: &str,
    ) -> Result<(), String> {
        let line = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "type": email_type,
            "to": to,
            "subject": subject,
            "token": token,
            "url": url,
        })
        .to_string();

        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        opts.mode(0o600);

        let mut file = opts
            .open(&self.outbox_path)
            .map_err(|e| format!("Failed to open mail outbox file: {}", e))?;

        #[cfg(unix)]
        std::fs::set_permissions(&self.outbox_path, Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to set mail outbox permissions: {}", e))?;

        writeln!(file, "{}", line)
            .map_err(|e| format!("Failed to write mail outbox entry: {}", e))?;

        Ok(())
    }
}

#[async_trait]
impl Mailer for FileMailer {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        let url = verification_link(&self.web_base_url, token);
        self.append_entry("verification", to, "Verify your 1degree email", token, &url)
    }

    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        let url = password_reset_link(&self.web_base_url, token);
        self.append_entry(
            "password_reset",
            to,
            "Reset your 1degree password",
            token,
            &url,
        )
    }
}

#[derive(Clone)]
pub struct ResendMailer {
    client: Resend,
    api_key: String,
    from: String,
    web_base_url: String,
    timeout: Duration,
}

impl ResendMailer {
    pub fn new(
        api_key: &str,
        from: impl Into<String>,
        web_base_url: impl Into<String>,
        base_url: Option<&str>,
    ) -> Result<Self, String> {
        Self::with_timeout(
            api_key,
            from,
            web_base_url,
            base_url,
            DEFAULT_RESEND_TIMEOUT,
        )
    }

    pub fn with_timeout(
        api_key: &str,
        from: impl Into<String>,
        web_base_url: impl Into<String>,
        base_url: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, String> {
        if api_key.trim().is_empty() {
            return Err("RESEND_API_KEY cannot be empty".to_string());
        }
        let from_str = from.into();
        if from_str.trim().is_empty() {
            return Err("MAIL_FROM cannot be empty".to_string());
        }

        let mut cfg = ConfigBuilder::new(api_key);
        if let Some(url_str) = base_url.filter(|s| !s.trim().is_empty()) {
            let parsed = url_str
                .trim()
                .parse()
                .map_err(|e| format!("Invalid RESEND_BASE_URL: {}", e))?;
            cfg = cfg.base_url(parsed);
        }

        Ok(Self {
            client: Resend::with_config(cfg.build()),
            api_key: api_key.to_string(),
            from: from_str,
            web_base_url: web_base_url.into(),
            timeout,
        })
    }

    async fn dispatch(
        &self,
        to: &str,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), String> {
        let email = CreateEmailBaseOptions::new(&self.from, [to], subject)
            .with_text(text_body)
            .with_html(html_body);

        match tokio::time::timeout(self.timeout, self.client.emails.send(email)).await {
            Ok(Ok(resp)) => {
                info!(
                    "Resend send succeeded: template='{}' status=200 id='{}'",
                    subject, resp.id
                );
                Ok(())
            }
            Ok(Err(err)) => {
                let (status_str, err_name, err_msg) = match &err {
                    resend_rs::Error::Resend(resp) => (
                        resp.status_code.to_string(),
                        resp.name.clone(),
                        resp.message.clone(),
                    ),
                    resend_rs::Error::RateLimit {
                        ratelimit_limit,
                        ratelimit_remaining,
                        ratelimit_reset,
                    } => (
                        "429".to_string(),
                        "rate_limit_exceeded".to_string(),
                        format!(
                            "limit={:?} remaining={:?} reset={:?}",
                            ratelimit_limit, ratelimit_remaining, ratelimit_reset
                        ),
                    ),
                    resend_rs::Error::Http(http_err) => {
                        let status = http_err
                            .status()
                            .map(|s| s.as_u16().to_string())
                            .unwrap_or_else(|| "network_error".to_string());
                        let name = if http_err.is_timeout() {
                            "timeout".to_string()
                        } else {
                            "http_error".to_string()
                        };
                        (status, name, http_err.to_string())
                    }
                    resend_rs::Error::Parse { message, .. } => (
                        "parse_error".to_string(),
                        "parse_error".to_string(),
                        message.clone(),
                    ),
                    resend_rs::Error::Other(msg) => (
                        "unknown".to_string(),
                        "resend_other".to_string(),
                        msg.clone(),
                    ),
                };
                let safe_name = redact_tokens_and_keys(&err_name, &self.api_key);
                let safe_msg = redact_tokens_and_keys(&err_msg, &self.api_key);
                error!(
                    "Resend API error: template='{}' status={} name='{}' message='{}'",
                    subject, status_str, safe_name, safe_msg
                );
                Err(format!(
                    "Resend failed (status={}, name={}, message={})",
                    status_str, safe_name, safe_msg
                ))
            }
            Err(_) => {
                error!(
                    "Resend API error: template='{}' status=timeout name='timeout' message='Resend request timed out after {}s'",
                    subject,
                    self.timeout.as_secs()
                );
                Err(format!(
                    "Resend failed (status=timeout, name=timeout, message=request timed out after {}s)",
                    self.timeout.as_secs()
                ))
            }
        }
    }
}

#[async_trait]
impl Mailer for ResendMailer {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        let link = verification_link(&self.web_base_url, token);
        let text = format!(
            "Welcome to 1degree.\n\nVerify your email address by visiting:\n{}\n\nThis link expires in 24 hours.",
            link
        );
        let html = format!(
            "<p>Welcome to <strong>1degree</strong>.</p><p><a href=\"{}\">Verify your email address</a></p><p>This link expires in 24 hours.</p>",
            link
        );
        self.dispatch(to, "Verify your 1degree email", &text, &html)
            .await
    }

    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        let link = password_reset_link(&self.web_base_url, token);
        let text = format!(
            "Reset your 1degree password by visiting:\n{}\n\nThis link expires in 1 hour.",
            link
        );
        let html = format!(
            "<p>Reset your <strong>1degree</strong> password:</p><p><a href=\"{}\">Reset password</a></p><p>This link expires in 1 hour.</p>",
            link
        );
        self.dispatch(to, "Reset your 1degree password", &text, &html)
            .await
    }
}

pub fn build_mailer(config: &Config) -> Result<Arc<dyn Mailer>, String> {
    match config.mailer {
        MailerKind::Console => {
            if config.app_env.is_production_like() {
                return Err("ConsoleMailer is refused in staging or production".to_string());
            }
            Ok(Arc::new(ConsoleMailer::new(config.web_base_url.clone())))
        }
        MailerKind::File => Ok(Arc::new(FileMailer::new(
            config.app_env,
            PathBuf::from(&config.mail_outbox_path),
            config.web_base_url.clone(),
        )?)),
        MailerKind::Resend => {
            let key = config
                .resend_api_key
                .as_deref()
                .ok_or_else(|| "RESEND_API_KEY is required when MAILER=resend".to_string())?;
            let from = config
                .mail_from
                .as_deref()
                .ok_or_else(|| "MAIL_FROM is required when MAILER=resend".to_string())?;
            Ok(Arc::new(ResendMailer::new(
                key,
                from,
                config.web_base_url.clone(),
                config.resend_base_url.as_deref(),
            )?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
    use serde_json::Value;
    use tokio::sync::Mutex;

    #[test]
    fn test_link_contracts() {
        assert_eq!(
            verification_link("http://localhost:3000/", "tok123"),
            "http://localhost:3000/verify-email?token=tok123"
        );
        assert_eq!(
            password_reset_link("https://1degree.fit", "rst456"),
            "https://1degree.fit/reset-password?token=rst456"
        );
    }

    #[tokio::test]
    async fn test_file_mailer_permissions_and_jsonl_contract() {
        let tmp_path = std::env::temp_dir().join(format!(
            "onedegree-outbox-test-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        // Confirm refused outside test/local
        assert!(
            FileMailer::new(
                AppEnv::Development,
                tmp_path.clone(),
                "http://localhost:3000"
            )
            .is_err()
        );
        assert!(
            FileMailer::new(AppEnv::Production, tmp_path.clone(), "https://1degree.fit").is_err()
        );

        let mailer =
            FileMailer::new(AppEnv::Test, tmp_path.clone(), "http://localhost:3000").unwrap();
        mailer
            .send_verification_email("user@example.test", "ver-tok-1")
            .await
            .unwrap();
        mailer
            .send_password_reset_email("user@example.test", "rst-tok-2")
            .await
            .unwrap();

        #[cfg(unix)]
        {
            let mode = std::fs::metadata(&tmp_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Outbox file must have 0600 permissions");
        }

        let contents = std::fs::read_to_string(&tmp_path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        let entry1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(entry1["type"], "verification");
        assert_eq!(entry1["to"], "user@example.test");
        assert_eq!(entry1["token"], "ver-tok-1");
        assert_eq!(
            entry1["url"],
            "http://localhost:3000/verify-email?token=ver-tok-1"
        );

        let entry2: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(entry2["type"], "password_reset");
        assert_eq!(
            entry2["url"],
            "http://localhost:3000/reset-password?token=rst-tok-2"
        );

        let _ = std::fs::remove_file(&tmp_path);
    }

    #[tokio::test]
    async fn test_resend_mailer_against_mock_http_base_url() {
        #[derive(Clone, Default)]
        struct MockState {
            captured: Arc<Mutex<Vec<(String, Value)>>>,
        }

        async fn handle_emails(
            State(state): State<MockState>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Json<Value> {
            let auth = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            state.captured.lock().await.push((auth, body));
            Json(json!({ "id": "mock_email_id_123" }))
        }

        let state = MockState::default();
        let app = Router::new()
            .route("/emails", post(handle_emails))
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let base_url = format!("http://{}", addr);
        let mailer = ResendMailer::new(
            "re_mock_test_key",
            "onboarding@resend.dev",
            "https://1degree.fit",
            Some(&base_url),
        )
        .unwrap();

        mailer
            .send_verification_email("qa-user@example.test", "secret_ver_token")
            .await
            .unwrap();
        mailer
            .send_password_reset_email("qa-user@example.test", "secret_rst_token")
            .await
            .unwrap();

        let captured = state.captured.lock().await;
        assert_eq!(captured.len(), 2);

        assert_eq!(captured[0].0, "Bearer re_mock_test_key");
        assert_eq!(captured[0].1["from"], "onboarding@resend.dev");
        assert_eq!(captured[0].1["to"], json!(["qa-user@example.test"]));
        assert!(
            captured[0].1["text"]
                .as_str()
                .unwrap()
                .contains("https://1degree.fit/verify-email?token=secret_ver_token")
        );

        assert!(
            captured[1].1["text"]
                .as_str()
                .unwrap()
                .contains("https://1degree.fit/reset-password?token=secret_rst_token")
        );

        server_handle.abort();
    }

    #[test]
    fn test_startup_mailer_notice_variants() {
        let console_cfg = Config::try_from_lookup(|k| match k {
            "APP_ENV" => Some("development".to_string()),
            "CORS_ORIGIN" => Some("http://localhost:3000".to_string()),
            "MAILER" => Some("console".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            startup_mailer_notice(&console_cfg),
            "MAILER=console: no real emails are sent; verification and reset links are printed in this terminal"
        );

        let resend_cfg = Config::try_from_lookup(|k| match k {
            "APP_ENV" => Some("development".to_string()),
            "CORS_ORIGIN" => Some("http://localhost:3000".to_string()),
            "MAILER" => Some("resend".to_string()),
            "RESEND_API_KEY" => Some("re_test_dummy_123456".to_string()),
            "MAIL_FROM" => Some("1degree <onboarding@resend.dev>".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            startup_mailer_notice(&resend_cfg),
            "mail via Resend, from resend.dev"
        );
    }

    #[tokio::test]
    async fn test_resend_mailer_handles_http_errors_timeout_and_malformed_body_without_leaking_secrets()
     {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        #[derive(Clone, Default)]
        struct MockMode {
            mode: Arc<Mutex<String>>,
        }

        async fn handle_mock(State(st): State<MockMode>) -> impl IntoResponse {
            let mode = st.mode.lock().await.clone();
            match mode.as_str() {
                "401" => (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "statusCode": 401,
                        "name": "missing_api_key",
                        "message": "Invalid key re_super_secret_key_99999 and token=super_tok_abc"
                    })),
                )
                    .into_response(),
                "403" => (
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "statusCode": 403,
                        "name": "validation_error",
                        "message": "You can only send testing emails to your own email address."
                    })),
                )
                    .into_response(),
                "422" => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "statusCode": 422,
                        "name": "invalid_from_address",
                        "message": "The from address is not verified."
                    })),
                )
                    .into_response(),
                "429" => (
                    StatusCode::TOO_MANY_REQUESTS,
                    [
                        ("ratelimit-limit", "10"),
                        ("ratelimit-remaining", "0"),
                        ("ratelimit-reset", "1"),
                    ],
                    Json(json!({
                        "statusCode": 429,
                        "name": "rate_limit_exceeded",
                        "message": "Too many requests"
                    })),
                )
                    .into_response(),
                "500" => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "statusCode": 500,
                        "name": "application_error",
                        "message": "Internal Resend upstream error"
                    })),
                )
                    .into_response(),
                "malformed" => (StatusCode::OK, "{not-valid-json").into_response(),
                "timeout" => {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    (StatusCode::OK, Json(json!({ "id": "late_id" }))).into_response()
                }
                _ => (StatusCode::OK, Json(json!({ "id": "ok" }))).into_response(),
            }
        }

        let state = MockMode::default();
        let app = Router::new()
            .route("/emails", post(handle_mock))
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let secret_key = "re_super_secret_key_99999";
        let secret_tok = "super_tok_abc";
        let mailer = ResendMailer::with_timeout(
            secret_key,
            "onboarding@resend.dev",
            "https://1degree.fit",
            Some(&format!("http://{}", addr)),
            Duration::from_millis(120),
        )
        .unwrap();

        for case in ["401", "403", "422", "429", "500", "malformed", "timeout"] {
            *state.mode.lock().await = case.to_string();
            let err = mailer
                .send_verification_email("qa@example.test", secret_tok)
                .await
                .expect_err(&format!("case {} must fail cleanly", case));

            assert!(
                !err.contains(secret_key),
                "case {} leaked API key: {}",
                case,
                err
            );
            assert!(
                !err.contains(secret_tok),
                "case {} leaked verification token: {}",
                case,
                err
            );
            if case != "malformed" && case != "429" {
                assert!(
                    err.contains(case),
                    "case {} error missing status context: {}",
                    case,
                    err
                );
            }
        }

        server_handle.abort();
    }
}
