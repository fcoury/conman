use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use conman_core::ConmanError;

pub struct PasswordPolicy;

impl PasswordPolicy {
    pub const MIN_LENGTH: usize = 12;
    pub const MAX_LENGTH: usize = 128;

    pub fn validate(password: &str) -> Result<(), ConmanError> {
        if password.len() < Self::MIN_LENGTH {
            return Err(ConmanError::Validation {
                message: format!("password must be at least {} characters", Self::MIN_LENGTH),
            });
        }

        if password.len() > Self::MAX_LENGTH {
            return Err(ConmanError::Validation {
                message: format!("password must be at most {} characters", Self::MAX_LENGTH),
            });
        }

        Ok(())
    }
}

pub fn hash_password(password: &str) -> Result<String, ConmanError> {
    PasswordPolicy::validate(password)?;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ConmanError::Internal {
            message: format!("password hashing failed: {e}"),
        })
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, ConmanError> {
    let parsed_hash = PasswordHash::new(hash).map_err(|e| ConmanError::Internal {
        message: format!("invalid stored password hash: {e}"),
    })?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_validates_length() {
        assert!(PasswordPolicy::validate("short").is_err());
        assert!(PasswordPolicy::validate(&"a".repeat(129)).is_err());
        assert!(PasswordPolicy::validate("good-password1").is_ok());
    }

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("very-secure-password").expect("hash");
        assert!(verify_password("very-secure-password", &hash).expect("verify"));
        assert!(!verify_password("wrong-password", &hash).expect("verify"));
    }
}
