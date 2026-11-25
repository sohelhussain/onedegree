use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};

use crate::errors::ApiError;

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes())
        .map_err(|e| ApiError::Internal(format!("Failed to hash password: {}", e)))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, ApiError> {
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
