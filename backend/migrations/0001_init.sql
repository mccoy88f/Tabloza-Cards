CREATE TABLE IF NOT EXISTS users (
    id             UUID PRIMARY KEY,
    username       TEXT NOT NULL,
    email          TEXT NOT NULL,
    password_hash  TEXT NOT NULL,
    rating         INTEGER NOT NULL DEFAULT 1000,
    games_played   INTEGER NOT NULL DEFAULT 0,
    games_won      INTEGER NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS users_username_lower ON users (lower(username));
CREATE UNIQUE INDEX IF NOT EXISTS users_email_lower ON users (lower(email));

CREATE TABLE IF NOT EXISTS matches (
    id           UUID PRIMARY KEY,
    game         TEXT NOT NULL,
    player1_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    player2_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    score1       INTEGER NOT NULL,
    score2       INTEGER NOT NULL,
    winner_id    UUID REFERENCES users(id) ON DELETE SET NULL,
    forfeit      BOOLEAN NOT NULL DEFAULT FALSE,
    rounds       INTEGER NOT NULL DEFAULT 0,
    finished_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS matches_player1 ON matches (player1_id, finished_at DESC);
CREATE INDEX IF NOT EXISTS matches_player2 ON matches (player2_id, finished_at DESC);
