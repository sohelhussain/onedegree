#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_access_expiration_secs: i64,
    pub jwt_refresh_expiration_secs: i64,
    pub google_client_id: String,
    pub admin_emails: Vec<String>,
    pub cors_origin: String,
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
            if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
                return Err(
                    "CORS_ORIGIN cannot be localhost or 127.0.0.1 in production".to_string()
                );
            }
        }

        axum::http::HeaderValue::from_str(trimmed)
            .map_err(|e| format!("CORS_ORIGIN is not a valid HTTP header value: {}", e))?;

        Ok(trimmed.to_string())
    }

    pub fn from_env() -> Self {
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());
        let is_production = app_env.eq_ignore_ascii_case("production");

        let admin_emails_str =
            std::env::var("ADMIN_EMAILS").unwrap_or_else(|_| "sohelhussaing@gmail.com".into());
        let admin_emails = admin_emails_str
            .split(',')
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .collect();

        let cors_origin_raw = std::env::var("CORS_ORIGIN").unwrap_or_else(|_| {
            panic!("FATAL: CORS_ORIGIN environment variable is required; refusing to start")
        });
        let cors_origin = Self::validate_cors_origin(&cors_origin_raw, is_production)
            .unwrap_or_else(|err| {
                panic!("FATAL: Invalid CORS_ORIGIN ({}): refusing to start", err)
            });

        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/onedegree".into()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "super-secure-jwt-secret-key-at-least-32-chars-long".into()),
            jwt_access_expiration_secs: std::env::var("JWT_ACCESS_EXPIRATION_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(900), // 15 minutes
            jwt_refresh_expiration_secs: std::env::var("JWT_REFRESH_EXPIRATION_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2592000), // 30 days
            google_client_id: std::env::var("GOOGLE_CLIENT_ID")
                .unwrap_or_else(|_| "test-google-client-id".into()),
            admin_emails,
            cors_origin,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
