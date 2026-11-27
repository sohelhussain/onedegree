use async_trait::async_trait;
use chrono::Utc;
use resend_rs::types::CreateEmailBaseOptions;
use resend_rs::{ConfigBuilder, Resend};
use serde_json::json;
use std::fs::{OpenOptions, Permissions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::config::{AppEnv, Config, MailerKind};

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
    from: String,
    web_base_url: String,
}

impl ResendMailer {
    pub fn new(
        api_key: &str,
        from: impl Into<String>,
        web_base_url: impl Into<String>,
        base_url: Option<&str>,
    ) -> Result<Self, String> {
        if api_key.trim().is_empty() {
            return Err("RESEND_API_KEY cannot be empty".to_string());
        }
        let from_str = from.into();
        if from_str.trim().is_empty() {
            return Err("MAIL_FROM cannot be empty".to_string());
        }

        let client = match base_url {
            Some(url_str) if !url_str.trim().is_empty() => {
                let parsed = url_str
                    .trim()
                    .parse()
                    .map_err(|e| format!("Invalid RESEND_BASE_URL: {}", e))?;
                let cfg = ConfigBuilder::new(api_key).base_url(parsed).build();
                Resend::with_config(cfg)
            }
            _ => Resend::new(api_key),
        };

        Ok(Self {
            client,
            from: from_str,
            web_base_url: web_base_url.into(),
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

        self.client.emails.send(email).await.map_err(|e| {
            // Never log API keys, tokens, or links when MAILER=resend
            error!("Resend API dispatch error for template '{}'", subject);
            format!("Resend send failed: {}", e)
        })?;

        Ok(())
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
}
