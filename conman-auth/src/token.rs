use std::collections::HashMap;

use chrono::Utc;
use conman_core::{ConmanError, Role};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub roles: HashMap<String, Role>,
    pub iat: i64,
    pub exp: i64,
}

pub fn issue_token(
    user_id: &str,
    email: &str,
    roles: HashMap<String, Role>,
    jwt_secret: &str,
    expiry_hours: u64,
) -> Result<String, ConmanError> {
    let now = Utc::now().timestamp();
    let exp = now + (expiry_hours as i64 * 3600);

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        roles,
        iat: now,
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| ConmanError::Unauthorized {
        message: format!("JWT encoding failed: {e}"),
    })
}

pub fn validate_token(token: &str, jwt_secret: &str) -> Result<Claims, ConmanError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|e| ConmanError::Forbidden {
        message: format!("invalid token: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip() {
        let token =
            issue_token("u1", "u@example.com", HashMap::new(), "secret", 24).expect("token");

        let claims = validate_token(&token, "secret").expect("claims");
        assert_eq!(claims.sub, "u1");
        assert_eq!(claims.email, "u@example.com");
    }
}
