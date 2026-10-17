use crate::api;
use dioxus::prelude::*;

#[component]
pub fn Leaderboard() -> Element {
    let board = use_resource(|| async { api::leaderboard().await });
    rsx! {
        section { class: "card-panel",
            h2 { "Classifica" }
            match &*board.read() {
                None => rsx! { p { class: "muted", "Caricamento…" } },
                Some(Err(e)) => rsx! { p { class: "error", "{e}" } },
                Some(Ok(rows)) if rows.is_empty() => rsx! { p { class: "muted", "Nessun giocatore ancora." } },
                Some(Ok(rows)) => rsx! {
                    table { class: "data",
                        thead { tr { th { "#" } th { "Giocatore" } th { "Elo" } th { "Partite" } th { "Vinte" } } }
                        tbody {
                            for (i, r) in rows.iter().enumerate() {
                                tr { key: "{r.username}",
                                    td { "{i + 1}" } td { "{r.username}" } td { "{r.rating}" }
                                    td { "{r.games_played}" } td { "{r.games_won}" }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
