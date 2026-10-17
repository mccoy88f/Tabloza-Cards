use crate::{use_session, Route};
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    let logged = use_session().read().is_some();
    rsx! {
        section { class: "hero",
            h1 { "Scopa online, all'italiana" }
            p { "Sfida altri giocatori in tempo reale con il mazzo da 40 carte. Registrati, crea un tavolo o siediti a uno libero." }
            div { class: "actions",
                if logged {
                    Link { class: "btn", to: Route::Play {}, "Vai alla lobby" }
                } else {
                    Link { class: "btn", to: Route::Register {}, "Crea un account" }
                    Link { class: "btn ghost", to: Route::Login {}, "Ho già un account" }
                }
            }
        }
        section { class: "rules card-panel",
            h2 { "Regole in breve" }
            ul {
                li { "Si gioca in due, 3 carte a testa e 4 scoperte in tavola." }
                li { "Prendi la carta dello stesso valore oppure una combinazione che somma al valore giocato (la presa singola ha la precedenza)." }
                li { "Svuotare la tavola vale una " b { "scopa" } " (tranne all'ultima giocata)." }
                li { "A fine smazzata: un punto per carte, denari, settebello, primiera, più le scope." }
                li { "Vince chi arriva per primo al punteggio del tavolo (di solito 11)." }
            }
        }
    }
}

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        section { class: "card-panel narrow",
            h2 { "Pagina non trovata" }
            p { {format!("/{}", segments.join("/"))} }
            Link { class: "btn", to: Route::Home {}, "Torna alla home" }
        }
    }
}
