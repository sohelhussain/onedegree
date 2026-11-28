use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};
use std::sync::OnceLock;

use crate::errors::ApiError;

static DUMMY_HASH: OnceLock<String> = OnceLock::new();

fn dummy_password_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        hash_password("dummy-timing-equalizer-password-1degree")
            .unwrap_or_else(|_| "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string())
    })
}

pub fn verify_dummy_password(password: &str) {
    let _ = verify_password(password, dummy_password_hash());
}

pub const MIN_PASSWORD_CHARS: usize = 8;
pub const MAX_PASSWORD_CHARS: usize = 128;
const MAX_PASSWORD_BYTES: usize = MAX_PASSWORD_CHARS * 4;

pub fn validate_password_length(password: &str) -> Result<(), ApiError> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(ApiError::BadRequest(format!(
            "Password must be between {} and {} characters long",
            MIN_PASSWORD_CHARS, MAX_PASSWORD_CHARS
        )));
    }
    let char_count = password.chars().count();
    if !(MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&char_count) {
        return Err(ApiError::BadRequest(format!(
            "Password must be between {} and {} characters long",
            MIN_PASSWORD_CHARS, MAX_PASSWORD_CHARS
        )));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    validate_password_length(password)?;
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes())
        .map_err(|e| ApiError::Internal(format!("Failed to hash password: {}", e)))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, ApiError> {
    if password.len() > MAX_PASSWORD_BYTES || password.chars().count() > MAX_PASSWORD_CHARS {
        return Ok(false);
    }
    let parsed_hash = argon2::password_hash::phc::PasswordHash::new(password_hash)
        .map_err(|e| ApiError::Internal(format!("Failed to parse password hash: {}", e)))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2_hashing_and_verification() {
        let password = "SuperSecretPassword123!";
        let hash = hash_password(password).expect("hashing succeeds");

        assert!(verify_password(password, &hash).expect("verification succeeds"));
        assert!(!verify_password("WrongPassword!", &hash).expect("verification succeeds"));
    }
}
