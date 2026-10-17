//! Rotte REST.

use crate::auth::{hash_password, verify_password, AuthUser};
use crate::db;
use crate::db::{TokenKind, UserRow};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use shared::dto::{
    validate_password, validate_registration, AuthResponse, ForgotPasswordRequest,
    LeaderboardEntry, LoginRequest, MatchRecord, MessageResponse, RegisterRequest,
    ResetPasswordRequest, ServerConfig, TokenRequest, UserProfile,
};

const VERIFY_TTL_HOURS: i64 = 48;
const RESET_TTL_HOURS: i64 = 1;
/// Intervallo minimo tra due email dello stesso tipo allo stesso utente.
const EMAIL_COOLDOWN_SECS: i64 = 60;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/verify", post(verify_email))
        .route("/auth/resend-verification", post(resend_verification))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/reset-password", post(reset_password))
        .route("/config", get(config))
        .route("/me", get(me))
        .route("/me/matches", get(my_matches))
        .route("/leaderboard", get(leaderboard))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<AuthResponse>> {
    validate_registration(&req).map_err(AppError::BadRequest)?;
    // Argon2 è volutamente lento: fuori dal runtime async.
    let hash = hash_blocking(req.password.clone()).await?;
    // Senza SMTP non si può verificare l'email: l'account è attivo subito.
    let verified = state.mailer.is_none();
    let user = db::create_user(
        &state.db,
        req.username.trim(),
        req.email.trim(),
        &hash,
        verified,
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AppError::Conflict("Username o email già registrati".into())
        }
        e => e.into(),
    })?;
    if !verified {
        send_verification(&state, &user).await?;
    }
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

fn msg(m: &str) -> Json<MessageResponse> {
    Json(MessageResponse {
        message: m.to_string(),
    })
}

async fn hash_blocking(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| AppError::Internal(e.into()))?
}

/// Crea il token e spedisce l'email in background (l'SMTP può essere lento).
/// Restituisce `false` se un'email è già partita da meno di un minuto.
async fn send_verification(state: &AppState, user: &UserRow) -> AppResult<bool> {
    let Some(mailer) = state.mailer.clone() else {
        return Ok(false);
    };
    let ttl = chrono::Duration::hours(VERIFY_TTL_HOURS);
    let Some(token) = db::create_email_token(
        &state.db,
        user.id,
        TokenKind::Verify,
        ttl,
        EMAIL_COOLDOWN_SECS,
    )
    .await?
    else {
        return Ok(false);
    };
    let (email, username) = (user.email.clone(), user.username.clone());
    tokio::spawn(async move {
        if let Err(e) = mailer.send_verification(&email, &username, &token).await {
            tracing::error!(error = %e, "invio email di conferma fallito");
        }
    });
    Ok(true)
}

async fn config(State(state): State<AppState>) -> Json<ServerConfig> {
    Json(ServerConfig {
        email_enabled: state.mailer.is_some(),
    })
}

async fn verify_email(
    State(state): State<AppState>,
    Json(req): Json<TokenRequest>,
) -> AppResult<Json<MessageResponse>> {
    let user = db::consume_email_token(&state.db, req.token.trim(), TokenKind::Verify)
        .await?
        .ok_or_else(|| AppError::BadRequest("Link non valido o scaduto".into()))?;
    db::mark_email_verified(&state.db, user).await?;
    Ok(msg("Email confermata: ora puoi giocare!"))
}

async fn resend_verification(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<MessageResponse>> {
    let user = db::find_user(&state.db, auth.id)
        .await?
        .ok_or(AppError::NotFound)?;
    if user.email_verified {
        return Ok(msg("La tua email è già confermata"));
    }
    if state.mailer.is_none() {
        return Err(AppError::BadRequest("Invio email non configurato".into()));
    }
    if !send_verification(&state, &user).await? {
        return Err(AppError::BadRequest(
            "Email già inviata: attendi un minuto e riprova".into(),
        ));
    }
    Ok(msg(&format!("Email di conferma inviata a {}", user.email)))
}

async fn forgot_password(
    State(state): State<AppState>,
    Json(req): Json<ForgotPasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    let Some(mailer) = state.mailer.clone() else {
        return Err(AppError::BadRequest(
            "Recupero password non disponibile".into(),
        ));
    };
    // La risposta è sempre la stessa: non riveliamo quali email sono registrate.
    let reply = msg("Se l'indirizzo è registrato riceverai un'email con le istruzioni");
    let Some(user) = db::find_user_by_email(&state.db, req.email.trim()).await? else {
        return Ok(reply);
    };
    let ttl = chrono::Duration::hours(RESET_TTL_HOURS);
    if let Some(token) = db::create_email_token(
        &state.db,
        user.id,
        TokenKind::Reset,
        ttl,
        EMAIL_COOLDOWN_SECS,
    )
    .await?
    {
        tokio::spawn(async move {
            if let Err(e) = mailer.send_reset(&user.email, &user.username, &token).await {
                tracing::error!(error = %e, "invio email di reset fallito");
            }
        });
    }
    Ok(reply)
}

async fn reset_password(
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> AppResult<Json<MessageResponse>> {
    validate_password(&req.password).map_err(AppError::BadRequest)?;
    let user = db::consume_email_token(&state.db, req.token.trim(), TokenKind::Reset)
        .await?
        .ok_or_else(|| AppError::BadRequest("Link non valido o scaduto".into()))?;
    let hash = hash_blocking(req.password).await?;
    db::set_password(&state.db, user, &hash).await?;
    Ok(msg("Password aggiornata: ora puoi accedere"))
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
