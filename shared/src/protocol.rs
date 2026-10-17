//! Protocollo WebSocket. I messaggi viaggiano come frame **binari** codificati con
//! `postcard`: compatti e veloci da (de)serializzare sia sul server che in WASM.

use crate::cards::Card;
use crate::scopa::{Move, Phase, RoundScore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameKind {
    Scopa,
}

impl GameKind {
    pub fn name(self) -> &'static str {
        match self {
            GameKind::Scopa => "scopa",
        }
    }
}

/// Messaggi client -> server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    ListRooms,
    CreateRoom {
        name: String,
        game: GameKind,
        target: u32,
    },
    JoinRoom {
        room_id: Uuid,
    },
    LeaveRoom,
    /// Conferma per iniziare la smazzata successiva.
    NextRound,
    PlayCard {
        card: Card,
        capture: Option<Vec<Card>>,
    },
    Chat {
        text: String,
    },
    Ping {
        t: u64,
    },
}

/// Messaggi server -> client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        user_id: Uuid,
        username: String,
    },
    Rooms {
        rooms: Vec<RoomSummary>,
    },
    /// Stato della stanza in cui si trova l'utente (None = nessuna stanza).
    Room {
        room: Option<RoomInfo>,
    },
    Game {
        view: GameView,
    },
    Chat {
        from: String,
        text: String,
    },
    Error {
        message: String,
    },
    Pong {
        t: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomStatus {
    Waiting,
    Playing,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomSummary {
    pub id: Uuid,
    pub name: String,
    pub game: GameKind,
    pub target: u32,
    pub players: Vec<String>,
    pub max_players: u8,
    pub status: RoomStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatInfo {
    pub username: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomInfo {
    pub summary: RoomSummary,
    pub seats: Vec<SeatInfo>,
    pub you: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerView {
    pub username: String,
    pub connected: bool,
    pub hand_count: u8,
    pub captured_count: u8,
    pub scope: u32,
    pub score: u32,
    pub ready_next: bool,
}

/// Vista della partita personalizzata per ogni giocatore (la mano avversaria è nascosta).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameView {
    pub room_id: Uuid,
    pub you: usize,
    pub players: Vec<PlayerView>,
    pub hand: Vec<Card>,
    pub table: Vec<Card>,
    pub deck_remaining: u8,
    pub turn: usize,
    pub dealer: usize,
    pub round: u32,
    pub target: u32,
    pub phase: Phase,
    pub last_move: Option<Move>,
    pub last_round: Option<Vec<RoundScore>>,
    pub forfeit: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("errore di codifica: {0}")]
pub struct CodecError(#[from] postcard::Error);

pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, CodecError> {
    Ok(postcard::to_stdvec(msg)?)
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, CodecError> {
    Ok(postcard::from_bytes(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Suit;

    #[test]
    fn roundtrip_binary() {
        let msg = ClientMsg::PlayCard {
            card: Card::new(Suit::Denari, 7),
            capture: Some(vec![Card::new(Suit::Coppe, 3), Card::new(Suit::Spade, 4)]),
        };
        let bytes = encode(&msg).unwrap();
        assert!(bytes.len() < 16, "frame compatto: {} byte", bytes.len());
        assert_eq!(decode::<ClientMsg>(&bytes).unwrap(), msg);
    }
}
