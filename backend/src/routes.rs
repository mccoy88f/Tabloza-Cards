//! Rotte REST.

use crate::auth::{hash_password, verify_password, AuthUser};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use shared::dto::{
    validate_registration, AuthResponse, LeaderboardEntry, LoginRequest, MatchRecord,
    RegisterRequest, UserProfile,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/me", get(me))
        .route("/me/matches", get(my_matches))
        .route("/leaderboard", get(leaderboard))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<AuthResponse>> {
    validate_registration(&req).map_err(AppError::BadRequest)?;
    let hash = {
        let pw = req.password.clone();
        // Argon2 è volutamente lento: fuori dal runtime async.
        tokio::task::spawn_blocking(move || hash_password(&pw))
            .await
            .map_err(|e| AppError::Internal(e.into()))??
    };
    let user = db::create_user(&state.db, req.username.trim(), req.email.trim(), &hash)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                AppError::Conflict("Username o email già registrati".into())
            }
            e => e.into(),
        })?;
    let token = state.jwt.issue(user.id, &user.username)?;
    Ok(Json(AuthResponse {
        token,
        user: user.into(),
    }))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    let user = db::find_user_by_login(&state.db, req.login.trim())
        .await?
        .ok_or(AppError::Unauthorized)?;
    let hash = user.password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&req.password, &hash))
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !ok {
        return Err(AppError::Unauthorized);
    }
    let token = state.jwt.issue(user.id, &user.username)?;
    Ok(Json(AuthResponse {
        token,
        user: user.into(),
    }))
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<UserProfile>> {
    let user = db::find_user(&state.db, auth.id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(user.into()))
}

#[derive(Deserialize)]
struct Limit {
    limit: Option<i64>,
}

async fn my_matches(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<Limit>,
) -> AppResult<Json<Vec<MatchRecord>>> {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    Ok(Json(db::match_history(&state.db, auth.id, limit).await?))
}

async fn leaderboard(
    State(state): State<AppState>,
    Query(q): Query<Limit>,
) -> AppResult<Json<Vec<LeaderboardEntry>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(db::leaderboard(&state.db, limit).await?))
}
