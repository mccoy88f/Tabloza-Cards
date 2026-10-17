use crate::live::Live;
use dioxus::prelude::*;
use shared::protocol::ClientMsg;

#[component]
pub fn ChatBox(title: String) -> Element {
    let live = use_context::<Live>();
    let mut text = use_signal(String::new);
    let lines = live.chat.read().clone();
    rsx! {
        div { class: "chat card-panel",
            h3 { "{title}" }
            div { class: "chat-lines",
                if lines.is_empty() { p { class: "muted", "Nessun messaggio." } }
                for (i, l) in lines.iter().enumerate().rev() {
                    p { key: "{i}", b { "{l.from}: " } "{l.text}" }
                }
            }
            form {
                onsubmit: move |ev: FormEvent| {
                    ev.prevent_default();
                    let t = text();
                    if !t.trim().is_empty() {
                        live.send(ClientMsg::Chat { text: t });
                        text.set(String::new());
                    }
                },
                input { value: "{text}", maxlength: 300, placeholder: "Scrivi un messaggio…",
                    oninput: move |e| text.set(e.value()) }
            }
        }
    }
}
