//! DTO delle API REST (JSON).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    /// Username oppure email.
    pub login: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub rating: i32,
    pub games_played: i32,
    pub games_won: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub username: String,
    pub rating: i32,
    pub games_played: i32,
    pub games_won: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchRecord {
    pub id: Uuid,
    pub game: String,
    pub opponent: String,
    pub my_score: i32,
    pub opponent_score: i32,
    pub won: bool,
    pub forfeit: bool,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

/// Validazione condivisa tra client e server.
pub fn validate_registration(r: &RegisterRequest) -> Result<(), String> {
    let u = r.username.trim();
    if u.len() < 3 || u.len() > 20 {
        return Err("Lo username deve avere tra 3 e 20 caratteri".into());
    }
    if !u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Lo username può contenere solo lettere, numeri e _".into());
    }
    let e = r.email.trim();
    if e.len() > 254 || !e.contains('@') || e.starts_with('@') || e.ends_with('@') {
        return Err("Email non valida".into());
    }
    if r.password.len() < 8 {
        return Err("La password deve avere almeno 8 caratteri".into());
    }
    if r.password.len() > 128 {
        return Err("Password troppo lunga".into());
    }
    Ok(())
}
