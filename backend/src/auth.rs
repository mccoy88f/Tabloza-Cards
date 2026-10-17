//! Hash delle password (Argon2) e token JWT.

use crate::error::AppError;
use crate::state::AppState;
use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub name: String,
    pub exp: i64,
}

pub struct JwtKeys {
    enc: EncodingKey,
    dec: DecodingKey,
    ttl_hours: i64,
}

impl JwtKeys {
    pub fn new(secret: &str, ttl_hours: i64) -> Self {
        JwtKeys {
            enc: EncodingKey::from_secret(secret.as_bytes()),
            dec: DecodingKey::from_secret(secret.as_bytes()),
            ttl_hours,
        }
    }

    pub fn issue(&self, user_id: Uuid, username: &str) -> Result<String, AppError> {
        let exp = (chrono::Utc::now() + chrono::Duration::hours(self.ttl_hours)).timestamp();
        let claims = Claims {
            sub: user_id,
            name: username.to_string(),
            exp,
        };
        encode(&Header::default(), &claims, &self.enc).map_err(|e| AppError::Internal(e.into()))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AppError> {
        decode::<Claims>(token, &self.dec, &Validation::default())
            .map(|d| d.claims)
            .map_err(|_| AppError::Unauthorized)
    }
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("hash: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Token casuale (256 bit, esadecimale) da inviare via email.
pub fn random_token() -> String {
    use argon2::password_hash::rand_core::RngCore;
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

pub fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex(&Sha256::digest(token.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Estrattore per le rotte protette: `Authorization: Bearer <jwt>`.
pub struct AuthUser {
    pub id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let claims = state.jwt.verify(token)?;
        Ok(AuthUser { id: claims.sub })
    }
}
