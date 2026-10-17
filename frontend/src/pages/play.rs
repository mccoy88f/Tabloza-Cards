//! Lobby, sala d'attesa e tavolo di gioco.

use crate::components::{ChatBox, GameTable};
use crate::live::Live;
use dioxus::prelude::*;
use shared::protocol::{ClientMsg, GameKind, RoomInfo, RoomStatus};
use shared::scopa::DEFAULT_TARGET;

#[component]
pub fn Play() -> Element {
    let live = use_context::<Live>();
    let mut error = live.error;
    let room = live.room.read().clone();
    let game = live.game.read().clone();

    rsx! {
        if let Some(e) = error() {
            div { class: "toast", onclick: move |_| error.set(None), "{e} ✕" }
        }
        match (room, game) {
            (Some(_), Some(view)) => rsx! {
                div { class: "play-layout",
                    GameTable { view }
                    aside {
                        button { class: "btn ghost small",
                            onclick: move |_| {
                                let finished = live.room.read().as_ref().map(|r| r.summary.status == RoomStatus::Finished).unwrap_or(true);
                                if finished || confirm("Abbandonare la partita? Perderai a tavolino.") {
                                    live.send(ClientMsg::LeaveRoom);
                                }
                            },
                            "Lascia il tavolo"
                        }
                        ChatBox { title: "Chat del tavolo" }
                    }
                }
            },
            (Some(info), None) => rsx! { WaitingRoom { info } },
            (None, _) => rsx! { LobbyView {} },
        }
    }
}

fn confirm(msg: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.confirm_with_message(msg).ok())
        .unwrap_or(true)
}

#[component]
fn WaitingRoom(info: RoomInfo) -> Element {
    let live = use_context::<Live>();
    rsx! {
        div { class: "play-layout",
            section { class: "card-panel",
                h2 { "{info.summary.name}" }
                p { "In attesa di un avversario… ({info.seats.len()}/{info.summary.max_players})" }
                p { class: "muted", "Si gioca a {info.summary.target} punti. La partita parte appena si siede il secondo giocatore." }
                ul { for s in info.seats.iter() { li { key: "{s.username}", "{s.username}" } } }
                button { class: "btn ghost", onclick: move |_| live.send(ClientMsg::LeaveRoom), "Chiudi il tavolo" }
            }
            aside { ChatBox { title: "Chat del tavolo" } }
        }
    }
}

#[component]
fn LobbyView() -> Element {
    let live = use_context::<Live>();
    let mut name = use_signal(String::new);
    let mut target = use_signal(|| DEFAULT_TARGET);
    let rooms = live.rooms.read().clone();

    rsx! {
        div { class: "play-layout",
            div {
                section { class: "card-panel",
                    h2 { "Nuovo tavolo di Scopa" }
                    form { class: "inline",
                        onsubmit: move |ev: FormEvent| {
                            ev.prevent_default();
                            live.send(ClientMsg::CreateRoom { name: name(), game: GameKind::Scopa, target: target() });
                            name.set(String::new());
                        },
                        input { value: "{name}", maxlength: 40, placeholder: "Nome del tavolo (opzionale)",
                            oninput: move |e| name.set(e.value()) }
                        select {
                            onchange: move |e| target.set(e.value().parse().unwrap_or(DEFAULT_TARGET)),
                            for t in [11u32, 16, 21, 31] {
                                option { value: "{t}", selected: t == target(), "a {t} punti" }
                            }
                        }
                        button { class: "btn", r#type: "submit", "Crea" }
                    }
                }
                section { class: "card-panel",
                    div { class: "row",
                        h2 { "Tavoli" }
                        button { class: "btn ghost small", onclick: move |_| live.send(ClientMsg::ListRooms), "Aggiorna" }
                    }
                    if rooms.is_empty() {
                        p { class: "muted", "Nessun tavolo aperto: creane uno!" }
                    }
                    ul { class: "rooms",
                        for r in rooms.iter() {
                            li { key: "{r.id}",
                                div {
                                    b { "{r.name}" }
                                    span { class: "muted", {format!(" · {} a {} · {}", r.game.name(), r.target, r.players.join(", "))} }
                                }
                                if r.status == RoomStatus::Waiting && r.players.len() < r.max_players as usize {
                                    button { class: "btn small",
                                        onclick: { let id = r.id; move |_| live.send(ClientMsg::JoinRoom { room_id: id }) },
                                        "Siediti"
                                    }
                                } else {
                                    span { class: "tag", "In corso" }
                                }
                            }
                        }
                    }
                }
            }
            aside { ChatBox { title: "Chat della lobby" } }
        }
    }
}
