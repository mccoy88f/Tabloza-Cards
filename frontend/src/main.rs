mod api;
mod components;
mod live;
mod pages;

use api::Session;
use dioxus::prelude::*;
use pages::{Home, Leaderboard, Login, NotFound, Play, Profile, Register};

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/login")]
        Login {},
        #[route("/register")]
        Register {},
        #[route("/leaderboard")]
        Leaderboard {},
        #[layout(Online)]
            #[route("/play")]
            Play {},
            #[route("/profile")]
            Profile {},
        #[end_layout]
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

/// Sessione utente condivisa (None = non autenticato).
pub type SessionSignal = Signal<Option<Session>>;

pub fn use_session() -> SessionSignal {
    use_context::<SessionSignal>()
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| Signal::new(api::load_session()));
    rsx! {
        document::Title { "Tabloza · Scopa online" }
        style { {include_str!("../assets/style.css")} }
        Router::<Route> {}
    }
}

#[component]
fn Shell() -> Element {
    let mut session = use_session();
    let nav = use_navigator();
    let logged = session.read().clone();
    rsx! {
        header { class: "topbar",
            Link { class: "brand", to: Route::Home {}, "🃏 Tabloza" }
            nav {
                Link { to: Route::Play {}, "Gioca" }
                Link { to: Route::Leaderboard {}, "Classifica" }
                if let Some(s) = logged {
                    Link { to: Route::Profile {}, "👤 {s.user.username}" }
                    button {
                        class: "link",
                        onclick: move |_| {
                            api::save_session(None);
                            session.set(None);
                            nav.push(Route::Home {});
                        },
                        "Esci"
                    }
                } else {
                    Link { to: Route::Login {}, "Accedi" }
                    Link { class: "btn small", to: Route::Register {}, "Registrati" }
                }
            }
        }
        main { class: "content", Outlet::<Route> {} }
    }
}

/// Layout delle pagine che richiedono login: apre il WebSocket.
#[component]
fn Online() -> Element {
    let session = use_session();
    let nav = use_navigator();
    let token = session.read().as_ref().map(|s| s.token.clone());
    match token {
        Some(token) => rsx! { OnlineInner { token } },
        None => {
            nav.replace(Route::Login {});
            rsx! {}
        }
    }
}

#[component]
fn OnlineInner(token: String) -> Element {
    let live = live::use_live(token);
    let status = *live.status.read();
    rsx! {
        if status != live::ConnStatus::Online {
            div { class: "banner",
                if status == live::ConnStatus::Connecting { "Connessione al server…" } else { "Connessione persa, riprovo…" }
            }
        }
        Outlet::<Route> {}
    }
}
