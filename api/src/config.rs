#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_access_expiration_secs: i64,
    pub jwt_refresh_expiration_secs: i64,
    pub google_client_id: String,
    pub admin_emails: Vec<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let admin_emails_str =
            std::env::var("ADMIN_EMAILS").unwrap_or_else(|_| "sohelhussaing@gmail.com".into());
        let admin_emails = admin_emails_str
            .split(',')
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .collect();

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
        }
    }
}
