//! Tavolo da gioco della Scopa.

use crate::components::{CardBack, CardView};
use crate::live::Live;
use dioxus::prelude::*;
use shared::protocol::{ClientMsg, GameView};
use shared::scopa::{capture_options, Phase, RoundScore};
use shared::Card;

fn cards_text(cards: &[Card]) -> String {
    cards
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" + ")
}

#[component]
pub fn GameTable(view: GameView) -> Element {
    let live = use_context::<Live>();
    // Carta in mano selezionata quando ci sono più prese possibili.
    let mut pending = use_signal(|| None::<Card>);

    let me = view.you;
    let opp = 1 - me;
    let my_turn = view.phase == Phase::Playing && view.turn == me;
    let table = view.table.clone();

    // Se la mano cambia (turno passato, nuova smazzata) annulla la selezione.
    let hand_key = format!("{:?}{}", view.hand, view.turn);
    use_effect(use_reactive!(|hand_key| {
        let _ = hand_key;
        pending.set(None);
    }));

    let play_card = {
        let table = table.clone();
        move |card: Card| {
            let options = capture_options(card, &table);
            match options.len() {
                0 => live.send(ClientMsg::PlayCard {
                    card,
                    capture: None,
                }),
                1 => live.send(ClientMsg::PlayCard {
                    card,
                    capture: Some(options[0].clone()),
                }),
                _ => pending.set(Some(card)),
            }
        }
    };

    let options = pending()
        .map(|c| capture_options(c, &table))
        .unwrap_or_default();
    let highlighted: Vec<Card> = options.iter().flatten().copied().collect();
    let p_me = &view.players[me];
    let p_opp = &view.players[opp];

    let status = match &view.phase {
        Phase::Playing if my_turn => "Tocca a te!".to_string(),
        Phase::Playing => format!("Turno di {}", p_opp.username),
        Phase::RoundOver => "Smazzata conclusa".to_string(),
        Phase::GameOver { .. } => "Partita conclusa".to_string(),
    };

    let last = view.last_move.as_ref().map(|m| {
        let who = &view.players[m.player].username;
        if m.captured.is_empty() {
            format!("{who} cala {}", m.card)
        } else if m.scopa {
            format!(
                "{who} fa SCOPA con {} prendendo {}",
                m.card,
                cards_text(&m.captured)
            )
        } else {
            format!("{who} prende {} con {}", cards_text(&m.captured), m.card)
        }
    });

    rsx! {
        div { class: "game",
            div { class: "scorebar",
                span { "Smazzata {view.round} · a {view.target} punti" }
                span { "Mazzo: {view.deck_remaining}" }
                span { class: if my_turn { "turn mine" } else { "turn" }, "{status}" }
            }

            PlayerStrip { name: p_opp.username.clone(), connected: p_opp.connected, score: p_opp.score,
                captured: p_opp.captured_count, scope: p_opp.scope, dealer: view.dealer == opp }
            div { class: "hand opponent",
                for i in 0..p_opp.hand_count { CardBack { key: "{i}", small: true } }
            }

            div { class: "felt",
                if table.is_empty() { p { class: "muted", "Tavola vuota" } }
                for c in table.iter().copied() {
                    CardView { key: "{c:?}", card: c, highlight: highlighted.contains(&c) }
                }
            }
            if let Some(l) = last { p { class: "last-move", "{l}" } }

            if let Some(card) = pending() {
                div { class: "capture-choice card-panel",
                    p { "Con {card} puoi prendere:" }
                    for (i, opt) in options.iter().enumerate() {
                        button { key: "{i}", class: "btn small",
                            onclick: {
                                let opt = opt.clone();
                                move |_| {
                                    live.send(ClientMsg::PlayCard { card, capture: Some(opt.clone()) });
                                    pending.set(None);
                                }
                            },
                            "{cards_text(opt)}"
                        }
                    }
                    button { class: "btn ghost small", onclick: move |_| pending.set(None), "Annulla" }
                }
            }

            div { class: "hand mine",
                for c in view.hand.iter().copied() {
                    CardView { key: "{c:?}", card: c, selected: pending() == Some(c), disabled: !my_turn,
                        onclick: play_card.clone() }
                }
            }
            PlayerStrip { name: format!("{} (tu)", p_me.username), connected: true, score: p_me.score,
                captured: p_me.captured_count, scope: p_me.scope, dealer: view.dealer == me }

            match &view.phase {
                Phase::RoundOver => rsx! {
                    RoundSummary { view: view.clone(), final_: false }
                },
                Phase::GameOver { .. } => rsx! {
                    RoundSummary { view: view.clone(), final_: true }
                },
                Phase::Playing => rsx! {},
            }
        }
    }
}

#[component]
fn PlayerStrip(
    name: String,
    connected: bool,
    score: u32,
    captured: u8,
    scope: u32,
    dealer: bool,
) -> Element {
    rsx! {
        div { class: "player-strip",
            span { class: if connected { "dot on" } else { "dot off" } }
            b { "{name}" }
            if dealer { span { class: "tag", "mazziere" } }
            span { "Punti: {score}" }
            span { "Prese: {captured}" }
            span { "Scope: {scope}" }
        }
    }
}

#[component]
fn RoundSummary(view: GameView, final_: bool) -> Element {
    let live = use_context::<Live>();
    let me = view.you;
    let rows: Vec<RoundScore> = view.last_round.clone().unwrap_or_default();
    let names: Vec<String> = view.players.iter().map(|p| p.username.clone()).collect();
    let mark = |b: bool| if b { "✔" } else { "" };
    let title = match view.phase {
        Phase::GameOver { winner } if winner == me => "Hai vinto! 🎉".to_string(),
        Phase::GameOver { winner } => format!("Ha vinto {}", names[winner]),
        _ => "Fine smazzata".to_string(),
    };
    let i_am_ready = view.players[me].ready_next;

    rsx! {
        div { class: "overlay",
            div { class: "card-panel summary",
                h2 { "{title}" }
                if view.forfeit { p { class: "muted", "Partita terminata per abbandono." } }
                if !rows.is_empty() {
                    table { class: "data",
                        thead { tr {
                            th { "" }
                            for n in names.iter() { th { "{n}" } }
                        } }
                        tbody {
                            tr { td { "Carte" } for r in rows.iter() { td { "{r.cards} {mark(r.point_cards)}" } } }
                            tr { td { "Denari" } for r in rows.iter() { td { "{r.denari} {mark(r.point_denari)}" } } }
                            tr { td { "Settebello" } for r in rows.iter() { td { "{mark(r.settebello)}" } } }
                            tr { td { "Primiera" } for r in rows.iter() { td { "{r.primiera} {mark(r.point_primiera)}" } } }
                            tr { td { "Scope" } for r in rows.iter() { td { "{r.scope}" } } }
                            tr { class: "total", td { "Smazzata" } for r in rows.iter() { td { "+{r.total}" } } }
                            tr { class: "total", td { "Totale" } for p in view.players.iter() { td { "{p.score}" } } }
                        }
                    }
                }
                if final_ {
                    button { class: "btn", onclick: move |_| live.send(ClientMsg::LeaveRoom), "Torna alla lobby" }
                } else if i_am_ready {
                    p { class: "muted", "In attesa dell'avversario…" }
                } else {
                    button { class: "btn", onclick: move |_| live.send(ClientMsg::NextRound), "Prossima smazzata" }
                }
            }
        }
    }
}
