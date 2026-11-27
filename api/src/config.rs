#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppEnv {
    Development,
    Local,
    Test,
    Staging,
    Production,
}

impl AppEnv {
    /// Fail-closed parser: missing or unknown APP_ENV values are treated as Production.
    pub fn from_raw(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("development") => Self::Development,
            Some("local") => Self::Local,
            Some("test") => Self::Test,
            Some("staging") => Self::Staging,
            Some("production") | None => Self::Production,
            Some(_) => Self::Production,
        }
    }

    pub fn is_production_like(self) -> bool {
        matches!(self, Self::Staging | Self::Production)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailerKind {
    Console,
    File,
    Resend,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub app_env: AppEnv,
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_access_expiration_secs: i64,
    pub jwt_refresh_expiration_secs: i64,
    pub google_client_id: String,
    pub google_jwks_url: Option<String>,
    pub admin_emails: Vec<String>,
    pub cors_origin: String,
    pub mailer: MailerKind,
    pub resend_api_key: Option<String>,
    pub resend_base_url: Option<String>,
    pub mail_from: Option<String>,
    pub web_base_url: String,
    pub mail_outbox_path: String,
}

pub fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1"
}

pub fn extract_url_host(raw: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(raw.trim()).map_err(|e| format!("Invalid URL: {}", e))?;
    parsed
        .host_str()
        .map(str::to_string)
        .ok_or_else(|| "URL must contain a valid host".to_string())
}

impl Config {
    pub fn validate_cors_origin(val: &str, is_production: bool) -> Result<String, String> {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            return Err("CORS_ORIGIN cannot be empty".to_string());
        }
        if trimmed == "*" {
            return Err("CORS_ORIGIN cannot be wildcard '*'".to_string());
        }
        if trimmed.ends_with('/') {
            return Err("CORS_ORIGIN cannot have a trailing slash".to_string());
        }
        if trimmed.contains('?') {
            return Err("CORS_ORIGIN cannot contain query parameters".to_string());
        }
        if trimmed.contains('#') {
            return Err("CORS_ORIGIN cannot contain fragments".to_string());
        }

        let uri: axum::http::Uri = trimmed
            .parse()
            .map_err(|e| format!("Invalid CORS_ORIGIN URI: {}", e))?;

        let scheme = uri
            .scheme_str()
            .ok_or_else(|| "CORS_ORIGIN must include scheme (http or https)".to_string())?;

        if scheme != "http" && scheme != "https" {
            return Err(format!(
                "CORS_ORIGIN scheme must be http or https, got: {}",
                scheme
            ));
        }

        let host = uri
            .host()
            .ok_or_else(|| "CORS_ORIGIN must have a valid host".to_string())?;

        let port_str = match uri.port_u16() {
            Some(port) => format!(":{}", port),
            None => String::new(),
        };

        let expected = format!("{}://{}{}", scheme, host, port_str);
        if trimmed != expected {
            return Err(format!(
                "CORS_ORIGIN must be exactly scheme + host + optional port without path (expected '{}', got '{}')",
                expected, trimmed
            ));
        }

        if is_production {
            if scheme != "https" {
                return Err("CORS_ORIGIN must use https in production".to_string());
            }
            if is_loopback_host(host) {
                return Err(
                    "CORS_ORIGIN cannot be localhost or 127.0.0.1 in production".to_string()
                );
            }
        }

        axum::http::HeaderValue::from_str(trimmed)
            .map_err(|e| format!("CORS_ORIGIN is not a valid HTTP header value: {}", e))?;

        Ok(trimmed.to_string())
    }

    pub fn validate_web_base_url(val: &str, app_env: AppEnv) -> Result<String, String> {
        let trimmed = val.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return Err("WEB_BASE_URL cannot be empty".to_string());
        }
        if trimmed.contains('?') || trimmed.contains('#') {
            return Err("WEB_BASE_URL cannot contain query parameters or fragments".to_string());
        }
        let parsed =
            reqwest::Url::parse(trimmed).map_err(|e| format!("Invalid WEB_BASE_URL URI: {}", e))?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(format!(
                "WEB_BASE_URL scheme must be http or https, got: {}",
                scheme
            ));
        }
        if parsed.host_str().is_none() {
            return Err("WEB_BASE_URL must have a valid host".to_string());
        }
        if app_env.is_production_like() && scheme != "https" {
            return Err("WEB_BASE_URL must use https in staging or production".to_string());
        }
        Ok(trimmed.to_string())
    }

    pub fn try_from_lookup<F>(get: F) -> Result<Self, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let non_empty = |key: &str| -> Option<String> {
            get(key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let app_env = AppEnv::from_raw(non_empty("APP_ENV").as_deref());
        let is_production = app_env == AppEnv::Production;

        let cors_origin_raw = non_empty("CORS_ORIGIN").ok_or_else(|| {
            "CORS_ORIGIN environment variable is required; refusing to start".to_string()
        })?;
        let cors_origin = Self::validate_cors_origin(&cors_origin_raw, is_production)?;

        let database_url = match non_empty("DATABASE_URL") {
            Some(url) => url,
            None => {
                if app_env.is_production_like() {
                    return Err("DATABASE_URL is required in staging or production".to_string());
                }
                "postgres://postgres:postgres@localhost:5432/onedegree".to_string()
            }
        };

        if app_env == AppEnv::Test {
            let db_host = extract_url_host(&database_url)
                .map_err(|e| format!("Invalid DATABASE_URL in test mode: {}", e))?;
            if !is_loopback_host(&db_host) {
                return Err(format!(
                    "APP_ENV=test refuses non-local DATABASE_URL host '{}' (only localhost or 127.0.0.1 allowed)",
                    db_host
                ));
            }
        }

        let web_base_url_raw = match non_empty("WEB_BASE_URL") {
            Some(v) => v,
            None => {
                if app_env.is_production_like() {
                    return Err("WEB_BASE_URL is required in staging or production".to_string());
                }
                "http://localhost:3000".to_string()
            }
        };
        let web_base_url = Self::validate_web_base_url(&web_base_url_raw, app_env)?;

        let mailer_raw = non_empty("MAILER").map(|m| m.to_ascii_lowercase());
        let mailer = match (app_env, mailer_raw.as_deref()) {
            (AppEnv::Staging | AppEnv::Production, Some("resend")) => MailerKind::Resend,
            (AppEnv::Staging | AppEnv::Production, Some("console"))
            | (AppEnv::Staging | AppEnv::Production, Some("file")) => {
                return Err(
                    "Refusing console or file mailer when APP_ENV is staging, production, missing, or unknown; MAILER=resend is required"
                        .to_string(),
                );
            }
            (AppEnv::Staging | AppEnv::Production, None) => {
                return Err(
                    "MAILER=resend is required when APP_ENV is staging, production, missing, or unknown"
                        .to_string(),
                );
            }
            (AppEnv::Staging | AppEnv::Production, Some(other)) => {
                return Err(format!(
                    "Unsupported MAILER '{}' in staging/production; MAILER=resend is required",
                    other
                ));
            }
            (AppEnv::Development, None | Some("console")) => MailerKind::Console,
            (AppEnv::Development, Some("file")) => {
                return Err("MAILER=file is allowed only when APP_ENV is test or local".to_string());
            }
            (AppEnv::Development, Some("resend")) => MailerKind::Resend,
            (AppEnv::Local | AppEnv::Test, None | Some("console")) => MailerKind::Console,
            (AppEnv::Local | AppEnv::Test, Some("file")) => MailerKind::File,
            (AppEnv::Local | AppEnv::Test, Some("resend")) => MailerKind::Resend,
            (_, Some(other)) => {
                return Err(format!("Unknown MAILER value '{}'", other));
            }
        };

        let resend_api_key = non_empty("RESEND_API_KEY");
        let mail_from = non_empty("MAIL_FROM");
        let resend_base_url = non_empty("RESEND_BASE_URL");

        if mailer == MailerKind::Resend {
            if resend_api_key.is_none() {
                return Err("RESEND_API_KEY is required when MAILER=resend".to_string());
            }
            if mail_from.is_none() {
                return Err("MAIL_FROM is required when MAILER=resend".to_string());
            }
        }

        // GOOGLE_JWKS_URL is honored ONLY when APP_ENV=test AND its host is localhost or 127.0.0.1.
        let google_jwks_url = if app_env == AppEnv::Test {
            match non_empty("GOOGLE_JWKS_URL") {
                Some(raw_jwks) => {
                    let parsed = reqwest::Url::parse(&raw_jwks)
                        .map_err(|e| format!("Invalid GOOGLE_JWKS_URL: {}", e))?;
                    let scheme = parsed.scheme();
                    if scheme != "http" && scheme != "https" {
                        return Err("GOOGLE_JWKS_URL must use http or https".to_string());
                    }
                    let host = parsed
                        .host_str()
                        .ok_or_else(|| "GOOGLE_JWKS_URL must have a valid host".to_string())?;
                    if !is_loopback_host(host) {
                        return Err(format!(
                            "GOOGLE_JWKS_URL host '{}' rejected: only localhost or 127.0.0.1 is allowed in APP_ENV=test",
                            host
                        ));
                    }
                    Some(raw_jwks)
                }
                None => None,
            }
        } else {
            None
        };

        let admin_emails_str =
            non_empty("ADMIN_EMAILS").unwrap_or_else(|| "sohelhussaing@gmail.com".into());
        let admin_emails = admin_emails_str
            .split(',')
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .collect();

        let jwt_secret = match non_empty("JWT_SECRET") {
            Some(s) => s,
            None => {
                if app_env.is_production_like() {
                    return Err("JWT_SECRET is required in staging or production".to_string());
                }
                "super-secure-jwt-secret-key-at-least-32-chars-long".to_string()
            }
        };

        let mail_outbox_path = non_empty("MAIL_OUTBOX_PATH")
            .unwrap_or_else(|| "/tmp/onedegree-mail-outbox.jsonl".to_string());

        Ok(Self {
            app_env,
            database_url,
            port: non_empty("PORT")
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            jwt_secret,
            jwt_access_expiration_secs: non_empty("JWT_ACCESS_EXPIRATION_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(900),
            jwt_refresh_expiration_secs: non_empty("JWT_REFRESH_EXPIRATION_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(2592000),
            google_client_id: non_empty("GOOGLE_CLIENT_ID")
                .unwrap_or_else(|| "test-google-client-id".into()),
            google_jwks_url,
            admin_emails,
            cors_origin,
            mailer,
            resend_api_key,
            resend_base_url,
            mail_from,
            web_base_url,
            mail_outbox_path,
        })
    }

    pub fn try_from_env() -> Result<Self, String> {
        Self::try_from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_env() -> Self {
        Self::try_from_env().unwrap_or_else(|err| {
            panic!("FATAL: Invalid configuration ({}): refusing to start", err)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map_lookup(map: HashMap<&str, &str>) -> impl Fn(&str) -> Option<String> {
        move |k| map.get(k).map(|v| (*v).to_string())
    }

    #[test]
    fn test_validate_cors_origin() {
        assert!(Config::validate_cors_origin("http://localhost:3000", false).is_ok());
        assert!(Config::validate_cors_origin("http://localhost:3000/", false).is_err());
        assert!(Config::validate_cors_origin("*", false).is_err());
        assert!(Config::validate_cors_origin("", false).is_err());
        assert!(Config::validate_cors_origin("localhost:3000", false).is_err());
        assert!(Config::validate_cors_origin("https://1degree.fit", true).is_ok());
        assert!(Config::validate_cors_origin("http://localhost:3000", true).is_err());
    }

    #[test]
    fn test_missing_or_unknown_app_env_is_fail_closed_production() {
        assert_eq!(AppEnv::from_raw(None), AppEnv::Production);
        assert_eq!(AppEnv::from_raw(Some("")), AppEnv::Production);
        assert_eq!(AppEnv::from_raw(Some("qa")), AppEnv::Production);
        assert_eq!(AppEnv::from_raw(Some("prod")), AppEnv::Production);
        assert_eq!(AppEnv::from_raw(Some("DEVELOPMENT")), AppEnv::Development);
        assert_eq!(AppEnv::from_raw(Some("local")), AppEnv::Local);
        assert_eq!(AppEnv::from_raw(Some("test")), AppEnv::Test);
        assert_eq!(AppEnv::from_raw(Some("staging")), AppEnv::Staging);
        assert_eq!(AppEnv::from_raw(Some("production")), AppEnv::Production);

        // Missing APP_ENV with MAILER=console must be refused
        let missing_env = HashMap::from([
            ("CORS_ORIGIN", "https://1degree.fit"),
            ("DATABASE_URL", "postgres://u:p@db.example/app"),
            ("JWT_SECRET", "12345678901234567890123456789012"),
            ("WEB_BASE_URL", "https://1degree.fit"),
            ("MAILER", "console"),
        ]);
        assert!(Config::try_from_lookup(map_lookup(missing_env)).is_err());

        // Unknown APP_ENV with MAILER=file must be refused
        let unknown_env = HashMap::from([
            ("APP_ENV", "preview"),
            ("CORS_ORIGIN", "https://1degree.fit"),
            ("DATABASE_URL", "postgres://u:p@db.example/app"),
            ("JWT_SECRET", "12345678901234567890123456789012"),
            ("WEB_BASE_URL", "https://1degree.fit"),
            ("MAILER", "file"),
        ]);
        assert!(Config::try_from_lookup(map_lookup(unknown_env)).is_err());

        // Unknown APP_ENV with valid resend ignores GOOGLE_JWKS_URL override
        let unknown_with_resend = HashMap::from([
            ("APP_ENV", "mystery"),
            ("CORS_ORIGIN", "https://1degree.fit"),
            ("DATABASE_URL", "postgres://u:p@db.example/app"),
            ("JWT_SECRET", "12345678901234567890123456789012"),
            ("WEB_BASE_URL", "https://1degree.fit"),
            ("MAILER", "resend"),
            ("RESEND_API_KEY", "re_dummy"),
            ("MAIL_FROM", "noreply@1degree.fit"),
            ("GOOGLE_JWKS_URL", "http://127.0.0.1:9999/certs"),
        ]);
        let cfg = Config::try_from_lookup(map_lookup(unknown_with_resend)).unwrap();
        assert_eq!(cfg.app_env, AppEnv::Production);
        assert!(cfg.google_jwks_url.is_none());
    }

    #[test]
    fn test_production_refuses_console_and_file_requires_resend_and_https() {
        fn base_prod<'a>(mailer: &'a str, web_url: &'a str) -> HashMap<&'static str, &'a str> {
            HashMap::from([
                ("APP_ENV", "production"),
                ("CORS_ORIGIN", "https://1degree.fit"),
                ("DATABASE_URL", "postgres://u:p@db.example/app"),
                ("JWT_SECRET", "12345678901234567890123456789012"),
                ("MAILER", mailer),
                ("RESEND_API_KEY", "re_dummy"),
                ("MAIL_FROM", "noreply@1degree.fit"),
                ("WEB_BASE_URL", web_url),
            ])
        }

        assert!(
            Config::try_from_lookup(map_lookup(base_prod("console", "https://1degree.fit")))
                .is_err()
        );
        assert!(
            Config::try_from_lookup(map_lookup(base_prod("file", "https://1degree.fit"))).is_err()
        );
        assert!(
            Config::try_from_lookup(map_lookup(base_prod("resend", "http://1degree.fit"))).is_err()
        );
        assert!(
            Config::try_from_lookup(map_lookup(base_prod("resend", "https://1degree.fit"))).is_ok()
        );

        // Staging also requires https WEB_BASE_URL and refuses console/file
        let staging_http = HashMap::from([
            ("APP_ENV", "staging"),
            ("CORS_ORIGIN", "https://staging.1degree.fit"),
            ("DATABASE_URL", "postgres://u:p@db.example/app"),
            ("JWT_SECRET", "12345678901234567890123456789012"),
            ("MAILER", "resend"),
            ("RESEND_API_KEY", "re_dummy"),
            ("MAIL_FROM", "noreply@1degree.fit"),
            ("WEB_BASE_URL", "http://staging.1degree.fit"),
        ]);
        assert!(Config::try_from_lookup(map_lookup(staging_http)).is_err());
    }

    #[test]
    fn test_app_env_test_requires_local_database_and_local_google_jwks_only() {
        let remote_db = HashMap::from([
            ("APP_ENV", "test"),
            ("CORS_ORIGIN", "http://localhost:3000"),
            (
                "DATABASE_URL",
                "postgres://u:p@ep-cool-dev.neon.tech/onedegree?sslmode=require",
            ),
        ]);
        assert!(Config::try_from_lookup(map_lookup(remote_db)).is_err());

        let local_db_nonlocal_jwks = HashMap::from([
            ("APP_ENV", "test"),
            ("CORS_ORIGIN", "http://localhost:3000"),
            (
                "DATABASE_URL",
                "postgres://postgres:postgres@127.0.0.1:5432/onedegree",
            ),
            ("GOOGLE_JWKS_URL", "https://evil.example/certs"),
        ]);
        assert!(Config::try_from_lookup(map_lookup(local_db_nonlocal_jwks)).is_err());

        let valid_test = HashMap::from([
            ("APP_ENV", "test"),
            ("CORS_ORIGIN", "http://localhost:3000"),
            (
                "DATABASE_URL",
                "postgres://postgres:postgres@localhost:5432/onedegree",
            ),
            ("MAILER", "file"),
            ("GOOGLE_JWKS_URL", "http://127.0.0.1:18080/certs"),
        ]);
        let cfg = Config::try_from_lookup(map_lookup(valid_test)).unwrap();
        assert_eq!(cfg.mailer, MailerKind::File);
        assert_eq!(
            cfg.google_jwks_url.as_deref(),
            Some("http://127.0.0.1:18080/certs")
        );

        // Development ignores GOOGLE_JWKS_URL and refuses MAILER=file
        let dev_jwks = HashMap::from([
            ("APP_ENV", "development"),
            ("CORS_ORIGIN", "http://localhost:3000"),
            ("GOOGLE_JWKS_URL", "http://127.0.0.1:18080/certs"),
        ]);
        let dev_cfg = Config::try_from_lookup(map_lookup(dev_jwks)).unwrap();
        assert!(dev_cfg.google_jwks_url.is_none());

        let dev_file = HashMap::from([
            ("APP_ENV", "development"),
            ("CORS_ORIGIN", "http://localhost:3000"),
            ("MAILER", "file"),
        ]);
        assert!(Config::try_from_lookup(map_lookup(dev_file)).is_err());
    }
}
