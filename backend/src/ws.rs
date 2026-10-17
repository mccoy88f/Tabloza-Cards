//! Endpoint WebSocket: frame binari postcard, un task di scrittura per connessione.

use crate::error::AppError;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use shared::protocol::{decode, ClientMsg};
use std::time::Duration;
use tokio::sync::mpsc;

const PING_EVERY: Duration = Duration::from_secs(20);
const MAX_FRAME: usize = 4 * 1024;

#[derive(Deserialize)]
pub struct WsAuth {
    token: String,
}

/// `GET /ws?token=<jwt>`: il browser non può impostare header sul WebSocket,
/// quindi il token viaggia come parametro.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsAuth>,
) -> Result<Response, AppError> {
    let claims = state.jwt.verify(&q.token)?;
    if state.mailer.is_some() {
        let user = crate::db::find_user(&state.db, claims.sub)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !user.email_verified {
            return Err(AppError::Forbidden(
                "Conferma la tua email per giocare".into(),
            ));
        }
    }
    Ok(ws
        .max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| run(socket, state, claims.sub, claims.name)))
}

async fn run(socket: WebSocket, state: AppState, user: uuid::Uuid, username: String) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let conn_id = state.hub.connect(user, &username, tx);
    tracing::debug!(%username, conn_id, "ws connesso");

    let writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(PING_EVERY);
        ping.tick().await;
        loop {
            tokio::select! {
                frame = rx.recv() => match frame {
                    Some(f) => {
                        if sink.send(Message::Binary((*f).clone().into())).await.is_err() {
                            break;
                        }
                    }
                    None => break, // connessione sostituita o hub chiuso
                },
                _ = ping.tick() => {
                    if sink.send(Message::Ping(Default::default())).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = sink.close().await;
    });

    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(bytes) => match decode::<ClientMsg>(&bytes) {
                Ok(m) => state.hub.handle(user, &username, m),
                Err(e) => tracing::debug!(%username, error = %e, "frame non valido"),
            },
            Message::Close(_) => break,
            _ => {}
        }
        if writer.is_finished() {
            break;
        }
    }

    state.hub.disconnect(user, conn_id);
    writer.abort();
    tracing::debug!(%username, conn_id, "ws disconnesso");
}
