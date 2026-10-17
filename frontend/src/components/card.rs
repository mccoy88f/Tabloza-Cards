use dioxus::prelude::*;
use shared::{Card, Suit};

fn suit_class(s: Suit) -> &'static str {
    match s {
        Suit::Denari => "denari",
        Suit::Coppe => "coppe",
        Suit::Spade => "spade",
        Suit::Bastoni => "bastoni",
    }
}

/// Carta scoperta. `selected`/`highlight` cambiano solo lo stile.
#[component]
pub fn CardView(
    card: Card,
    #[props(default)] selected: bool,
    #[props(default)] highlight: bool,
    #[props(default)] disabled: bool,
    #[props(default)] small: bool,
    onclick: Option<EventHandler<Card>>,
) -> Element {
    let mut class = format!("card {}", suit_class(card.suit));
    if selected {
        class.push_str(" selected");
    }
    if highlight {
        class.push_str(" highlight");
    }
    if small {
        class.push_str(" small");
    }
    if onclick.is_some() && !disabled {
        class.push_str(" clickable");
    }
    let label = card.short_label();
    let symbol = card.suit.symbol();
    rsx! {
        button {
            class: "{class}",
            title: "{card}",
            disabled: disabled || onclick.is_none(),
            onclick: move |_| {
                if let Some(h) = &onclick {
                    h.call(card);
                }
            },
            span { class: "corner", "{label}" }
            span { class: "pip", "{symbol}" }
            span { class: "corner bottom", "{label}" }
        }
    }
}

#[component]
pub fn CardBack(#[props(default)] small: bool) -> Element {
    rsx! { div { class: if small { "card back small" } else { "card back" } } }
}
