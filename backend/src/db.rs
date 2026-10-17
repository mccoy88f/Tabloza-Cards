//! Accesso al database (PostgreSQL via sqlx, query a runtime: niente DB in fase di build).

use chrono::{DateTime, Utc};
use shared::dto::{LeaderboardEntry, MatchRecord, UserProfile};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub rating: i32,
    pub games_played: i32,
    pub games_won: i32,
    pub created_at: DateTime<Utc>,
}

impl From<UserRow> for UserProfile {
    fn from(u: UserRow) -> Self {
        UserProfile {
            id: u.id,
            username: u.username,
            email: u.email,
            rating: u.rating,
            games_played: u.games_played,
            games_won: u.games_won,
            created_at: u.created_at,
        }
    }
}

const USER_COLS: &str =
    "id, username, email, password_hash, rating, games_played, games_won, created_at";

pub async fn create_user(
    db: &PgPool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> sqlx::Result<UserRow> {
    sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4) RETURNING {USER_COLS}"
    ))
    .bind(Uuid::new_v4())
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .fetch_one(db)
    .await
}

pub async fn find_user_by_login(db: &PgPool, login: &str) -> sqlx::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {USER_COLS} FROM users WHERE lower(username) = lower($1) OR lower(email) = lower($1) LIMIT 1"
    ))
    .bind(login)
    .fetch_optional(db)
    .await
}

pub async fn find_user(db: &PgPool, id: Uuid) -> sqlx::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(&format!("SELECT {USER_COLS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn leaderboard(db: &PgPool, limit: i64) -> sqlx::Result<Vec<LeaderboardEntry>> {
    let rows: Vec<(String, i32, i32, i32)> = sqlx::query_as(
        "SELECT username, rating, games_played, games_won FROM users
         ORDER BY rating DESC, games_won DESC, username ASC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(username, rating, games_played, games_won)| LeaderboardEntry {
                username,
                rating,
                games_played,
                games_won,
            },
        )
        .collect())
}

pub async fn match_history(db: &PgPool, user: Uuid, limit: i64) -> sqlx::Result<Vec<MatchRecord>> {
    #[derive(FromRow)]
    struct Row {
        id: Uuid,
        game: String,
        opponent: String,
        my_score: i32,
        opponent_score: i32,
        won: bool,
        forfeit: bool,
        finished_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT m.id, m.game,
                u.username AS opponent,
                CASE WHEN m.player1_id = $1 THEN m.score1 ELSE m.score2 END AS my_score,
                CASE WHEN m.player1_id = $1 THEN m.score2 ELSE m.score1 END AS opponent_score,
                COALESCE(m.winner_id = $1, false) AS won,
                m.forfeit, m.finished_at
         FROM matches m
         JOIN users u ON u.id = CASE WHEN m.player1_id = $1 THEN m.player2_id ELSE m.player1_id END
         WHERE m.player1_id = $1 OR m.player2_id = $1
         ORDER BY m.finished_at DESC LIMIT $2",
    )
    .bind(user)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MatchRecord {
            id: r.id,
            game: r.game,
            opponent: r.opponent,
            my_score: r.my_score,
            opponent_score: r.opponent_score,
            won: r.won,
            forfeit: r.forfeit,
            finished_at: r.finished_at,
        })
        .collect())
}

/// Risultato di una partita conclusa, prodotto dall'hub e salvato in modo asincrono.
#[derive(Debug, Clone)]
pub struct FinishedMatch {
    pub game: &'static str,
    pub players: [Uuid; 2],
    pub scores: [i32; 2],
    pub winner: usize,
    pub forfeit: bool,
    pub rounds: i32,
}

/// Salva la partita e aggiorna statistiche ed Elo in un'unica transazione.
pub async fn record_match(db: &PgPool, m: &FinishedMatch) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    let ratings: Vec<(Uuid, i32)> =
        sqlx::query_as("SELECT id, rating FROM users WHERE id = ANY($1) FOR UPDATE")
            .bind(&m.players[..])
            .fetch_all(&mut *tx)
            .await?;
    let rating_of = |id: Uuid| {
        ratings
            .iter()
            .find(|r| r.0 == id)
            .map(|r| r.1)
            .unwrap_or(1000)
    };
    let w = m.players[m.winner];
    let l = m.players[1 - m.winner];
    let (dw, dl) = elo_delta(rating_of(w), rating_of(l));

    sqlx::query(
        "INSERT INTO matches (id, game, player1_id, player2_id, score1, score2, winner_id, forfeit, rounds)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4())
    .bind(m.game)
    .bind(m.players[0])
    .bind(m.players[1])
    .bind(m.scores[0])
    .bind(m.scores[1])
    .bind(w)
    .bind(m.forfeit)
    .bind(m.rounds)
    .execute(&mut *tx)
    .await?;

    for (id, delta, won) in [(w, dw, 1), (l, dl, 0)] {
        sqlx::query(
            "UPDATE users SET rating = GREATEST(rating + $2, 100), games_played = games_played + 1,
             games_won = games_won + $3 WHERE id = $1",
        )
        .bind(id)
        .bind(delta)
        .bind(won)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Elo con K = 32: restituisce (delta vincitore, delta perdente).
pub fn elo_delta(winner: i32, loser: i32) -> (i32, i32) {
    let expected = 1.0 / (1.0 + 10f64.powf((loser - winner) as f64 / 400.0));
    let d = (32.0 * (1.0 - expected)).round() as i32;
    (d, -d)
}

#[cfg(test)]
mod tests {
    #[test]
    fn elo_is_symmetric() {
        assert_eq!(super::elo_delta(1000, 1000), (16, -16));
        let (w, _) = super::elo_delta(1400, 1000);
        assert!(w < 16);
    }
}
