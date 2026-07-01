//! Password hashing (argon2id) and JWT access tokens (§9/§15).

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Hash a password with argon2id and a random salt.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Verify a password against a stored argon2id hash.
pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// JWT claims: subject = user id (uuid string), exp = unix seconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

fn now_secs() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0)
}

/// Mint a signed access token for `user_id`, valid for `ttl_secs`.
pub fn make_token(secret: &[u8], user_id: &str, ttl_secs: usize) -> Result<String, String> {
    let claims = Claims {
        sub: user_id.to_string(),
        exp: now_secs() + ttl_secs,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| e.to_string())
}

/// Verify and decode a token, returning the claims (checks signature + expiry).
pub fn verify_token(secret: &[u8], token: &str) -> Result<Claims, String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|e| e.to_string())
}

/// Refresh-token claims: subject, a unique token id (`jti`), and a `family`
/// shared by a rotation chain (so reuse can revoke the whole family, §9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub jti: String,
    pub family: String,
    pub exp: usize,
}

/// Mint a refresh token.
pub fn make_refresh_token(
    secret: &[u8],
    user_id: &str,
    jti: &str,
    family: &str,
    ttl_secs: usize,
) -> Result<String, String> {
    let claims = RefreshClaims {
        sub: user_id.to_string(),
        jti: jti.to_string(),
        family: family.to_string(),
        exp: now_secs() + ttl_secs,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| e.to_string())
}

/// Verify a refresh token's signature + expiry.
pub fn verify_refresh_token(secret: &[u8], token: &str) -> Result<RefreshClaims, String> {
    decode::<RefreshClaims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let h = hash_password("s3cret!").unwrap();
        assert!(verify_password("s3cret!", &h));
        assert!(!verify_password("wrong", &h));
        assert!(h.starts_with("$argon2id$"));
    }

    #[test]
    fn token_roundtrip_and_tamper() {
        let secret = b"test-secret";
        let t = make_token(secret, "user-123", 3600).unwrap();
        assert_eq!(verify_token(secret, &t).unwrap().sub, "user-123");
        assert!(verify_token(b"other-secret", &t).is_err());
    }
}
