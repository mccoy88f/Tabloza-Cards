use crate::{api, use_session};
use dioxus::prelude::*;

#[component]
pub fn Profile() -> Element {
    let session = use_session();
    let token = session
        .read()
        .as_ref()
        .map(|s| s.token.clone())
        .unwrap_or_default();
    let data = use_resource(move || {
        let token = token.clone();
        async move {
            let me = api::me(&token).await?;
            let matches = api::my_matches(&token).await?;
            Ok::<_, String>((me, matches))
        }
    });

    rsx! {
        match &*data.read() {
            None => rsx! { p { class: "muted", "Caricamento…" } },
            Some(Err(e)) => rsx! { p { class: "error", "{e}" } },
            Some(Ok((me, matches))) => rsx! {
                section { class: "card-panel",
                    h2 { "{me.username}" }
                    div { class: "stats",
                        div { span { "Elo" } b { "{me.rating}" } }
                        div { span { "Partite" } b { "{me.games_played}" } }
                        div { span { "Vinte" } b { "{me.games_won}" } }
                    }
                    p { class: "muted", "Iscritto dal " {me.created_at.format("%d/%m/%Y").to_string()} }
                }
                section { class: "card-panel",
                    h3 { "Ultime partite" }
                    if matches.is_empty() {
                        p { class: "muted", "Nessuna partita giocata." }
                    } else {
                        table { class: "data",
                            thead { tr { th { "Data" } th { "Avversario" } th { "Risultato" } th { "Punti" } } }
                            tbody {
                                for m in matches.iter() {
                                    tr { key: "{m.id}",
                                        td { {m.finished_at.format("%d/%m %H:%M").to_string()} }
                                        td { "{m.opponent}" }
                                        td { class: if m.won { "win" } else { "loss" },
                                            if m.won { "Vinta" } else { "Persa" }
                                            if m.forfeit { " (abbandono)" }
                                        }
                                        td { "{m.my_score} – {m.opponent_score}" }
                                    }
                                }
                            }
                        }
                    }
                }
            },
        }
    }
}
