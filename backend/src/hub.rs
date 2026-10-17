//! Hub in tempo reale: connessioni WebSocket, stanze e partite in memoria.
//!
//! Tutto lo stato vive dietro un unico `Mutex` sincrono: le operazioni sono brevi e
//! non fanno mai `.await` mentre il lock è preso. I risultati delle partite finite
//! vengono inviati su un canale e salvati su database da un task separato.

use crate::db::FinishedMatch;
use rand::seq::SliceRandom;
use shared::full_deck;
use shared::protocol::{
    encode, ClientMsg, GameKind, GameView, PlayerView, RoomInfo, RoomStatus, RoomSummary, SeatInfo,
    ServerMsg,
};
use shared::scopa::{Phase, ScopaGame, DEFAULT_TARGET};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Frame binario già codificato, condivisibile tra più connessioni senza copie.
pub type Frame = Arc<Vec<u8>>;
pub type Tx = mpsc::UnboundedSender<Frame>;

const MAX_PLAYERS: usize = 2;
const MAX_CHAT_LEN: usize = 300;
const MAX_ROOM_NAME: usize = 40;
/// Una stanza senza giocatori connessi viene chiusa dopo questo intervallo.
const ABANDON_AFTER: Duration = Duration::from_secs(5 * 60);

struct Conn {
    id: u64,
    tx: Tx,
}

struct Seat {
    user_id: Uuid,
    username: String,
    disconnected_since: Option<Instant>,
    ready_next: bool,
}

struct Room {
    id: Uuid,
    name: String,
    game_kind: GameKind,
    target: u32,
    seats: Vec<Seat>,
    game: Option<ScopaGame>,
    status: RoomStatus,
    forfeit: bool,
}

impl Room {
    fn summary(&self) -> RoomSummary {
        RoomSummary {
            id: self.id,
            name: self.name.clone(),
            game: self.game_kind,
            target: self.target,
            players: self.seats.iter().map(|s| s.username.clone()).collect(),
            max_players: MAX_PLAYERS as u8,
            status: self.status,
        }
    }

    fn seat_of(&self, user: Uuid) -> Option<usize> {
        self.seats.iter().position(|s| s.user_id == user)
    }

    fn info_for(&self, you: usize) -> RoomInfo {
        RoomInfo {
            summary: self.summary(),
            seats: self
                .seats
                .iter()
                .map(|s| SeatInfo {
                    username: s.username.clone(),
                    connected: s.disconnected_since.is_none(),
                })
                .collect(),
            you,
        }
    }

    fn view_for(&self, you: usize) -> Option<GameView> {
        let g = self.game.as_ref()?;
        Some(GameView {
            room_id: self.id,
            you,
            players: self
                .seats
                .iter()
                .enumerate()
                .map(|(i, s)| PlayerView {
                    username: s.username.clone(),
                    connected: s.disconnected_since.is_none(),
                    hand_count: g.hands[i].len() as u8,
                    captured_count: g.captured[i].len() as u8,
                    scope: g.scope[i],
                    score: g.scores[i],
                    ready_next: s.ready_next,
                })
                .collect(),
            hand: g.hands[you].clone(),
            table: g.table.clone(),
            deck_remaining: g.deck.len() as u8,
            turn: g.turn,
            dealer: g.dealer,
            round: g.round,
            target: g.target,
            phase: g.phase.clone(),
            last_move: g.last_move.clone(),
            last_round: g.last_round.clone(),
            forfeit: self.forfeit,
        })
    }
}

#[derive(Default)]
struct Inner {
    conns: HashMap<Uuid, Conn>,
    rooms: HashMap<Uuid, Room>,
    user_room: HashMap<Uuid, Uuid>,
}

pub struct Hub {
    inner: Mutex<Inner>,
    next_conn: AtomicU64,
    finished: mpsc::UnboundedSender<FinishedMatch>,
}

fn frame(msg: &ServerMsg) -> Frame {
    // La codifica di tipi noti non può fallire.
    Arc::new(encode(msg).expect("codifica postcard"))
}

fn shuffled_deck() -> Vec<shared::Card> {
    let mut d = full_deck();
    d.shuffle(&mut rand::thread_rng());
    d
}

impl Inner {
    fn send(&self, user: Uuid, f: &Frame) {
        if let Some(c) = self.conns.get(&user) {
            let _ = c.tx.send(f.clone());
        }
    }

    fn send_msg(&self, user: Uuid, msg: ServerMsg) {
        self.send(user, &frame(&msg));
    }

    fn rooms_frame(&self) -> Frame {
        let mut rooms: Vec<RoomSummary> = self
            .rooms
            .values()
            .filter(|r| r.status != RoomStatus::Finished)
            .map(Room::summary)
            .collect();
        rooms.sort_by(|a, b| (a.status as u8, &a.name).cmp(&(b.status as u8, &b.name)));
        frame(&ServerMsg::Rooms { rooms })
    }

    fn broadcast_rooms(&self) {
        let f = self.rooms_frame();
        for c in self.conns.values() {
            let _ = c.tx.send(f.clone());
        }
    }

    /// Invia a ogni membro della stanza info stanza + vista di gioco personalizzata.
    fn sync_room(&self, room_id: Uuid) {
        let Some(room) = self.rooms.get(&room_id) else {
            return;
        };
        for (i, seat) in room.seats.iter().enumerate() {
            if self.user_room.get(&seat.user_id) != Some(&room_id) {
                continue; // ha già lasciato il tavolo
            }
            self.send_msg(
                seat.user_id,
                ServerMsg::Room {
                    room: Some(room.info_for(i)),
                },
            );
            if let Some(view) = room.view_for(i) {
                self.send_msg(seat.user_id, ServerMsg::Game { view });
            }
        }
    }

    fn finished_match(room: &Room) -> Option<FinishedMatch> {
        let g = room.game.as_ref()?;
        let winner = g.winner()?;
        Some(FinishedMatch {
            game: room.game_kind.name(),
            players: [room.seats[0].user_id, room.seats[1].user_id],
            scores: [g.scores[0] as i32, g.scores[1] as i32],
            winner,
            forfeit: room.forfeit,
            rounds: g.round as i32,
        })
    }

    /// Rimuove l'utente dalla sua stanza. Se la partita è in corso, perde a tavolino.
    fn leave_room(&mut self, user: Uuid) -> Option<FinishedMatch> {
        let room_id = self.user_room.remove(&user)?;
        let mut result = None;
        let room = self.rooms.get_mut(&room_id)?;
        let seat = room.seat_of(user)?;

        if let Some(g) = room.game.as_mut() {
            if room.status == RoomStatus::Playing {
                g.forfeit(seat);
                room.forfeit = true;
                room.status = RoomStatus::Finished;
                result = Self::finished_match(room);
            }
            // Il posto resta assegnato per mantenere coerenti gli indici della partita:
            // lo segniamo come disconnesso.
            room.seats[seat].disconnected_since = Some(Instant::now());
        } else {
            room.seats.remove(seat);
        }

        let empty = room.seats.iter().all(|s| {
            !self
                .user_room
                .get(&s.user_id)
                .is_some_and(|r| *r == room_id)
        });
        if empty {
            self.rooms.remove(&room_id);
        } else {
            self.sync_room(room_id);
        }
        self.send_msg(user, ServerMsg::Room { room: None });
        self.broadcast_rooms();
        result
    }

    fn create_room(
        &mut self,
        user: Uuid,
        username: &str,
        name: String,
        game: GameKind,
        target: u32,
    ) {
        let name: String = name.trim().chars().take(MAX_ROOM_NAME).collect();
        let name = if name.is_empty() {
            format!("Tavolo di {username}")
        } else {
            name
        };
        let target = if (1..=61).contains(&target) {
            target
        } else {
            DEFAULT_TARGET
        };
        let id = Uuid::new_v4();
        self.rooms.insert(
            id,
            Room {
                id,
                name,
                game_kind: game,
                target,
                seats: vec![Seat {
                    user_id: user,
                    username: username.to_string(),
                    disconnected_since: None,
                    ready_next: false,
                }],
                game: None,
                status: RoomStatus::Waiting,
                forfeit: false,
            },
        );
        self.user_room.insert(user, id);
        self.sync_room(id);
        self.broadcast_rooms();
    }

    fn join_room(&mut self, user: Uuid, username: &str, room_id: Uuid) -> Result<(), String> {
        let room = self.rooms.get_mut(&room_id).ok_or("Stanza inesistente")?;
        if room.status != RoomStatus::Waiting || room.seats.len() >= MAX_PLAYERS {
            return Err("La stanza è piena o la partita è già iniziata".into());
        }
        room.seats.push(Seat {
            user_id: user,
            username: username.to_string(),
            disconnected_since: None,
            ready_next: false,
        });
        if room.seats.len() == MAX_PLAYERS {
            room.game =
                Some(ScopaGame::new(room.target, shuffled_deck()).map_err(|e| e.to_string())?);
            room.status = RoomStatus::Playing;
        }
        self.user_room.insert(user, room_id);
        self.sync_room(room_id);
        self.broadcast_rooms();
        Ok(())
    }

    fn play(
        &mut self,
        user: Uuid,
        card: shared::Card,
        capture: Option<Vec<shared::Card>>,
    ) -> Result<Option<FinishedMatch>, String> {
        let room_id = *self.user_room.get(&user).ok_or("Non sei in una stanza")?;
        let room = self.rooms.get_mut(&room_id).ok_or("Stanza inesistente")?;
        let seat = room.seat_of(user).ok_or("Non sei seduto al tavolo")?;
        let game = room.game.as_mut().ok_or("La partita non è iniziata")?;
        game.play(seat, card, capture).map_err(|e| e.to_string())?;
        let mut result = None;
        match game.phase {
            Phase::RoundOver => room.seats.iter_mut().for_each(|s| s.ready_next = false),
            Phase::GameOver { .. } => {
                room.status = RoomStatus::Finished;
                result = Self::finished_match(room);
            }
            Phase::Playing => {}
        }
        let finished = room.status == RoomStatus::Finished;
        self.sync_room(room_id);
        if finished {
            self.broadcast_rooms();
        }
        Ok(result)
    }

    fn next_round(&mut self, user: Uuid) -> Result<(), String> {
        let room_id = *self.user_room.get(&user).ok_or("Non sei in una stanza")?;
        let room = self.rooms.get_mut(&room_id).ok_or("Stanza inesistente")?;
        let seat = room.seat_of(user).ok_or("Non sei seduto al tavolo")?;
        let game = room.game.as_mut().ok_or("La partita non è iniziata")?;
        if game.phase != Phase::RoundOver {
            return Err("La smazzata non è finita".into());
        }
        room.seats[seat].ready_next = true;
        if room.seats.iter().all(|s| s.ready_next) {
            game.start_round(shuffled_deck())
                .map_err(|e| e.to_string())?;
            room.seats.iter_mut().for_each(|s| s.ready_next = false);
        }
        self.sync_room(room_id);
        Ok(())
    }

    fn chat(&self, user: Uuid, username: &str, text: String) {
        let text: String = text.trim().chars().take(MAX_CHAT_LEN).collect();
        if text.is_empty() {
            return;
        }
        let f = frame(&ServerMsg::Chat {
            from: username.to_string(),
            text,
        });
        match self.user_room.get(&user).and_then(|r| self.rooms.get(r)) {
            Some(room) => room
                .seats
                .iter()
                .filter(|s| self.user_room.get(&s.user_id) == Some(&room.id))
                .for_each(|s| self.send(s.user_id, &f)),
            // Chat della lobby: a tutti quelli che non sono seduti a un tavolo.
            None => self
                .conns
                .keys()
                .filter(|u| !self.user_room.contains_key(u))
                .for_each(|u| self.send(*u, &f)),
        }
    }
}

impl Hub {
    pub fn new(finished: mpsc::UnboundedSender<FinishedMatch>) -> Self {
        Hub {
            inner: Mutex::new(Inner::default()),
            next_conn: AtomicU64::new(1),
            finished,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn emit(&self, m: Option<FinishedMatch>) {
        if let Some(m) = m {
            let _ = self.finished.send(m);
        }
    }

    /// Registra una connessione. Una nuova connessione dello stesso utente sostituisce la precedente.
    pub fn connect(&self, user: Uuid, username: &str, tx: Tx) -> u64 {
        let id = self.next_conn.fetch_add(1, Ordering::Relaxed);
        let mut inner = self.lock();
        if let Some(old) = inner.conns.insert(user, Conn { id, tx }) {
            let _ = old.tx.send(frame(&ServerMsg::Error {
                message: "Sessione aperta in un'altra finestra".into(),
            }));
        }
        inner.send_msg(
            user,
            ServerMsg::Welcome {
                user_id: user,
                username: username.to_string(),
            },
        );
        let rooms = inner.rooms_frame();
        inner.send(user, &rooms);

        // Riconnessione a una partita in corso.
        if let Some(room_id) = inner.user_room.get(&user).copied() {
            if let Some(room) = inner.rooms.get_mut(&room_id) {
                if let Some(seat) = room.seat_of(user) {
                    room.seats[seat].disconnected_since = None;
                }
            }
            inner.sync_room(room_id);
        } else {
            inner.send_msg(user, ServerMsg::Room { room: None });
        }
        id
    }

    pub fn disconnect(&self, user: Uuid, conn_id: u64) {
        let mut inner = self.lock();
        if inner.conns.get(&user).map(|c| c.id) != Some(conn_id) {
            return; // già sostituita da una connessione più recente
        }
        inner.conns.remove(&user);
        let Some(room_id) = inner.user_room.get(&user).copied() else {
            return;
        };
        let playing = inner
            .rooms
            .get(&room_id)
            .is_some_and(|r| r.status == RoomStatus::Playing);
        if playing {
            if let Some(room) = inner.rooms.get_mut(&room_id) {
                if let Some(seat) = room.seat_of(user) {
                    room.seats[seat].disconnected_since = Some(Instant::now());
                }
            }
            inner.sync_room(room_id);
        } else {
            // In attesa o partita finita: si libera il posto.
            let m = inner.leave_room(user);
            drop(inner);
            self.emit(m);
        }
    }

    pub fn handle(&self, user: Uuid, username: &str, msg: ClientMsg) {
        let mut inner = self.lock();
        let res: Result<Option<FinishedMatch>, String> = match msg {
            ClientMsg::Ping { t } => {
                inner.send_msg(user, ServerMsg::Pong { t });
                Ok(None)
            }
            ClientMsg::ListRooms => {
                let f = inner.rooms_frame();
                inner.send(user, &f);
                Ok(None)
            }
            ClientMsg::CreateRoom { name, game, target } => {
                let m = inner.leave_room(user);
                inner.create_room(user, username, name, game, target);
                Ok(m)
            }
            ClientMsg::JoinRoom { room_id } => {
                if inner.user_room.get(&user) == Some(&room_id) {
                    Ok(None)
                } else {
                    let m = inner.leave_room(user);
                    inner.join_room(user, username, room_id).map(|_| m)
                }
            }
            ClientMsg::LeaveRoom => Ok(inner.leave_room(user)),
            ClientMsg::NextRound => inner.next_round(user).map(|_| None),
            ClientMsg::PlayCard { card, capture } => inner.play(user, card, capture),
            ClientMsg::Chat { text } => {
                inner.chat(user, username, text);
                Ok(None)
            }
        };
        match res {
            Ok(m) => {
                drop(inner);
                self.emit(m);
            }
            Err(message) => inner.send_msg(user, ServerMsg::Error { message }),
        }
    }

    /// Chiude le stanze abbandonate (nessun giocatore connesso da troppo tempo).
    pub fn sweep(&self) {
        let mut inner = self.lock();
        let now = Instant::now();
        let stale: Vec<Uuid> = inner
            .rooms
            .values()
            .filter(|r| {
                r.seats.iter().all(|s| {
                    s.disconnected_since
                        .is_some_and(|t| now.duration_since(t) > ABANDON_AFTER)
                })
            })
            .map(|r| r.id)
            .collect();
        if stale.is_empty() {
            return;
        }
        for id in stale {
            if let Some(room) = inner.rooms.remove(&id) {
                for s in room.seats {
                    if inner.user_room.get(&s.user_id) == Some(&id) {
                        inner.user_room.remove(&s.user_id);
                    }
                }
            }
        }
        inner.broadcast_rooms();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::protocol::decode;

    fn client(hub: &Hub, name: &str) -> (Uuid, mpsc::UnboundedReceiver<Frame>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = Uuid::new_v4();
        hub.connect(id, name, tx);
        (id, rx)
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<Frame>) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        while let Ok(f) = rx.try_recv() {
            out.push(decode(&f).unwrap());
        }
        out
    }

    fn last_view(msgs: &[ServerMsg]) -> Option<GameView> {
        msgs.iter().rev().find_map(|m| match m {
            ServerMsg::Game { view } => Some(view.clone()),
            _ => None,
        })
    }

    #[test]
    fn two_players_start_and_forfeit() {
        let (ftx, mut frx) = mpsc::unbounded_channel();
        let hub = Hub::new(ftx);
        let (a, mut ra) = client(&hub, "anna");
        let (b, mut rb) = client(&hub, "bruno");
        hub.handle(
            a,
            "anna",
            ClientMsg::CreateRoom {
                name: "t".into(),
                game: GameKind::Scopa,
                target: 11,
            },
        );
        let room_id = drain(&mut ra)
            .into_iter()
            .find_map(|m| match m {
                ServerMsg::Room { room: Some(r) } => Some(r.summary.id),
                _ => None,
            })
            .unwrap();
        hub.handle(b, "bruno", ClientMsg::JoinRoom { room_id });
        let va = last_view(&drain(&mut ra)).unwrap();
        let vb = last_view(&drain(&mut rb)).unwrap();
        assert_eq!(va.hand.len(), 3);
        assert_eq!(vb.table.len(), 4);
        assert_ne!(va.hand, vb.hand);

        // Mossa fuori turno -> errore solo a chi l'ha fatta.
        let (off, off_name, off_view) = if va.turn == 0 {
            (b, "bruno", &vb)
        } else {
            (a, "anna", &va)
        };
        hub.handle(
            off,
            off_name,
            ClientMsg::PlayCard {
                card: off_view.hand[0],
                capture: None,
            },
        );
        let errs = drain(if off == a { &mut ra } else { &mut rb });
        assert!(matches!(errs.last(), Some(ServerMsg::Error { .. })));

        hub.handle(a, "anna", ClientMsg::LeaveRoom);
        let m = frx.try_recv().unwrap();
        assert!(m.forfeit);
        assert_eq!(m.players[m.winner], b);
    }
}
