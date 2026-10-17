//! Connessione WebSocket in tempo reale: un coroutine Dioxus mantiene il socket,
//! riconnette in automatico e riversa i messaggi del server in segnali reattivi.

use crate::api;
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedReceiver;
use futures_util::future::{select, Either};
use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
use shared::protocol::{decode, encode, ClientMsg, GameView, RoomInfo, RoomSummary, ServerMsg};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnStatus {
    Connecting,
    Online,
    Offline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatLine {
    pub from: String,
    pub text: String,
}

/// Stato condiviso via context con tutte le schermate "online".
#[derive(Clone, Copy)]
pub struct Live {
    pub status: Signal<ConnStatus>,
    pub rooms: Signal<Vec<RoomSummary>>,
    pub room: Signal<Option<RoomInfo>>,
    pub game: Signal<Option<GameView>>,
    pub chat: Signal<Vec<ChatLine>>,
    pub error: Signal<Option<String>>,
    pub tx: Coroutine<ClientMsg>,
}

impl Live {
    pub fn send(&self, msg: ClientMsg) {
        self.tx.send(msg);
    }
}

const MAX_CHAT: usize = 100;

/// Da chiamare una volta nel layout autenticato: avvia il socket e fornisce il context.
pub fn use_live(token: String) -> Live {
    let mut status = use_signal(|| ConnStatus::Connecting);
    let mut rooms = use_signal(Vec::new);
    let mut room = use_signal(|| None::<RoomInfo>);
    let mut game = use_signal(|| None::<GameView>);
    let mut chat = use_signal(Vec::<ChatLine>::new);
    let mut error = use_signal(|| None::<String>);

    let tx = use_coroutine(move |mut rx: UnboundedReceiver<ClientMsg>| {
        let token = token.clone();
        async move {
            let mut backoff = 500u32;
            loop {
                status.set(ConnStatus::Connecting);
                let ws = match WebSocket::open(&api::ws_url(&token)) {
                    Ok(ws) => ws,
                    Err(_) => {
                        status.set(ConnStatus::Offline);
                        gloo_timers::future::TimeoutFuture::new(backoff).await;
                        backoff = (backoff * 2).min(10_000);
                        continue;
                    }
                };
                let (mut write, mut read) = ws.split();
                let mut got_any = false;
                loop {
                    match select(rx.next(), read.next()).await {
                        Either::Left((Some(msg), _)) => {
                            if let Ok(bytes) = encode(&msg) {
                                if write.send(Message::Bytes(bytes)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        // Il componente è stato smontato: chiudiamo tutto.
                        Either::Left((None, _)) => return,
                        Either::Right((Some(Ok(Message::Bytes(bytes))), _)) => {
                            if !got_any {
                                got_any = true;
                                backoff = 500;
                                status.set(ConnStatus::Online);
                            }
                            let Ok(msg) = decode::<ServerMsg>(&bytes) else {
                                continue;
                            };
                            match msg {
                                ServerMsg::Welcome { .. } | ServerMsg::Pong { .. } => {}
                                ServerMsg::Rooms { rooms: r } => rooms.set(r),
                                ServerMsg::Room { room: r } => {
                                    if r.is_none() {
                                        game.set(None);
                                        chat.write().clear();
                                    }
                                    room.set(r);
                                }
                                ServerMsg::Game { view } => game.set(Some(view)),
                                ServerMsg::Chat { from, text } => {
                                    let mut c = chat.write();
                                    c.push(ChatLine { from, text });
                                    if c.len() > MAX_CHAT {
                                        c.remove(0);
                                    }
                                }
                                ServerMsg::Error { message } => error.set(Some(message)),
                            }
                        }
                        Either::Right((Some(Ok(Message::Text(_))), _)) => {}
                        Either::Right((Some(Err(_)), _)) | Either::Right((None, _)) => break,
                    }
                }
                status.set(ConnStatus::Offline);
                gloo_timers::future::TimeoutFuture::new(backoff).await;
                backoff = (backoff * 2).min(10_000);
            }
        }
    });

    let live = Live {
        status,
        rooms,
        room,
        game,
        chat,
        error,
        tx,
    };
    use_context_provider(|| live)
}
