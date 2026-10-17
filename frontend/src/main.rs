mod api;
mod components;
mod live;
mod pages;

use api::Session;
use dioxus::prelude::*;
use pages::{
    ForgotPassword, Home, Leaderboard, Login, NotFound, Play, Profile, Register, ResetPassword,
    VerifyEmail, VerifyNotice,
};

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
        #[route("/verify?:token")]
        VerifyEmail { token: String },
        #[route("/forgot")]
        ForgotPassword {},
        #[route("/reset?:token")]
        ResetPassword { token: String },
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

/// Layout delle pagine che richiedono login: aggiorna il profilo e apre il WebSocket
/// solo se l'email è confermata.
#[component]
fn Online() -> Element {
    let mut session = use_session();
    let nav = use_navigator();
    let token = session.read().as_ref().map(|s| s.token.clone());

    // Profilo aggiornato dal server (es. email appena confermata o token scaduto).
    let _refresh = use_resource(move || async move {
        let Some(token) = session.peek().as_ref().map(|s| s.token.clone()) else {
            return;
        };
        match api::me(&token).await {
            Ok(user) => {
                let changed = session.peek().as_ref().is_some_and(|s| s.user != user);
                if changed {
                    let s = Session { token, user };
                    api::save_session(Some(&s));
                    session.set(Some(s));
                }
            }
            Err(e) if e == api::UNAUTHORIZED => {
                api::save_session(None);
                session.set(None);
            }
            Err(_) => {}
        }
    });

    let verified = session
        .read()
        .as_ref()
        .is_some_and(|s| s.user.email_verified);
    match token {
        Some(_) if !verified => rsx! { VerifyNotice {} },
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
