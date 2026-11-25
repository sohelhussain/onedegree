use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::errors::ApiError;

const GOOGLE_CERTS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";

#[derive(Debug, Clone, Deserialize)]
pub struct JwkKey {
    pub kid: String,
    pub kty: String,
    pub alg: Option<String>,
    pub n: String,
    pub e: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwksResponse {
    pub keys: Vec<JwkKey>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleIdTokenPayload {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub exp: usize,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GoogleClaims {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

type KeyCache = Option<(std::time::Instant, Vec<JwkKey>)>;

#[derive(Clone)]
pub struct GoogleVerifier {
    client_id: String,
    http_client: Client,
    keys_cache: Arc<RwLock<KeyCache>>,
}

impl GoogleVerifier {
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            http_client: Client::builder().build().unwrap_or_default(),
            keys_cache: Arc::new(RwLock::new(None)),
        }
    }

    async fn get_keys(&self) -> Result<Vec<JwkKey>, ApiError> {
        // 1. Check read cache
        {
            let cache = self.keys_cache.read().await;
            if let Some((fetched_at, keys)) = &*cache {
                // Cache keys for 1 hour
                if fetched_at.elapsed() < std::time::Duration::from_secs(3600) {
                    return Ok(keys.clone());
                }
            }
        }

        // 2. Fetch fresh JWKS from Google
        self.refresh_keys().await
    }

    async fn refresh_keys(&self) -> Result<Vec<JwkKey>, ApiError> {
        let res = self
            .http_client
            .get(GOOGLE_CERTS_URL)
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to fetch Google JWKS: {}", e)))?;

        let jwks = res
            .json::<JwksResponse>()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to parse Google JWKS: {}", e)))?;

        let mut cache = self.keys_cache.write().await;
        *cache = Some((std::time::Instant::now(), jwks.keys.clone()));

        Ok(jwks.keys)
    }

    pub async fn verify(&self, id_token: &str) -> Result<GoogleClaims, ApiError> {
        // 1. Parse token header to retrieve `kid`
        let header = decode_header(id_token)
            .map_err(|e| ApiError::BadRequest(format!("Invalid Google ID token header: {}", e)))?;

        let kid = header
            .kid
            .ok_or_else(|| ApiError::BadRequest("Missing 'kid' in ID token header".into()))?;

        // 2. Find matching key in cache
        let mut keys = self.get_keys().await?;
        let mut key = keys.iter().find(|k| k.kid == kid).cloned();

        // If not found in cache, force refresh once
        if key.is_none() {
            keys = self.refresh_keys().await?;
            key = keys.into_iter().find(|k| k.kid == kid);
        }

        let jwk = key.ok_or_else(|| {
            ApiError::Unauthorized("No matching Google public key found for token 'kid'".into())
        })?;

        // 3. Build decoding key from RSA components (n, e)
        let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e).map_err(|e| {
            ApiError::Internal(format!("Failed to construct RSA decoding key: {}", e))
        })?;

        // 4. Configure strict validation (iss, aud, exp)
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);
        validation.set_audience(&[&self.client_id]);
        validation.validate_exp = true;

        // 5. Decode and verify signature
        let token_data = decode::<GoogleIdTokenPayload>(id_token, &decoding_key, &validation)
            .map_err(|e| {
                ApiError::Unauthorized(format!("Google ID token verification failed: {}", e))
            })?;

        Ok(GoogleClaims {
            sub: token_data.claims.sub,
            email: token_data.claims.email,
            email_verified: token_data.claims.email_verified,
            name: token_data.claims.name,
            picture: token_data.claims.picture,
        })
    }
}
