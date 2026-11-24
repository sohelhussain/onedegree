use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::errors::ApiError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id as string
    pub exp: usize,  // Expiration timestamp
    pub iat: usize,  // Issued at timestamp
}

pub fn create_access_token(
    user_id: Uuid,
    secret: &[u8],
    expiration_secs: i64,
) -> Result<String, ApiError> {
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now,
        exp: now + (expiration_secs as usize),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| ApiError::Internal(format!("Failed to create access token: {}", e)))
}

pub fn verify_access_token(token: &str, secret: &[u8]) -> Result<Uuid, ApiError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &validation,
    )
    .map_err(|e| ApiError::Unauthorized(format!("Invalid or expired access token: {}", e)))?;

    Uuid::parse_str(&token_data.claims.sub)
        .map_err(|_| ApiError::Unauthorized("Invalid user ID in token subject".into()))
}

pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_generation_and_verification() {
        let secret = b"my-super-secret-key-for-testing-purposes";
        let user_id = Uuid::new_v4();

        let token = create_access_token(user_id, secret, 900).expect("token creation");
        let verified_id = verify_access_token(&token, secret).expect("token verification");

        assert_eq!(user_id, verified_id);
    }

    #[test]
    fn test_refresh_token_hashing() {
        let token1 = generate_refresh_token();
        let hash1 = hash_refresh_token(&token1);
        let hash2 = hash_refresh_token(&token1);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }
}
