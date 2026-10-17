use crate::api::{self, Session};
use crate::{use_session, Route};
use dioxus::prelude::*;
use shared::dto::{validate_registration, LoginRequest, RegisterRequest};

#[component]
pub fn Login() -> Element {
    let mut session = use_session();
    let nav = use_navigator();
    let mut login = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let config = use_resource(api::config);
    let email_enabled = matches!(&*config.read(), Some(Ok(c)) if c.email_enabled);

    let submit = move |ev: FormEvent| async move {
        ev.prevent_default();
        busy.set(true);
        let req = LoginRequest {
            login: login(),
            password: password(),
        };
        match api::login(&req).await {
            Ok(r) => {
                let s = Session {
                    token: r.token,
                    user: r.user,
                };
                api::save_session(Some(&s));
                session.set(Some(s));
                nav.push(Route::Play {});
            }
            Err(e) => error.set(Some(e)),
        }
        busy.set(false);
    };

    rsx! {
        section { class: "card-panel narrow",
            h2 { "Accedi" }
            form { onsubmit: submit,
                label { "Username o email"
                    input { value: "{login}", autocomplete: "username", required: true,
                        oninput: move |e| login.set(e.value()) }
                }
                label { "Password"
                    input { r#type: "password", value: "{password}", autocomplete: "current-password", required: true,
                        oninput: move |e| password.set(e.value()) }
                }
                if let Some(e) = error() { p { class: "error", "{e}" } }
                button { class: "btn", r#type: "submit", disabled: busy(), "Entra" }
            }
            if email_enabled {
                p { class: "muted", Link { to: Route::ForgotPassword {}, "Password dimenticata?" } }
            }
            p { class: "muted", "Non hai un account? " Link { to: Route::Register {}, "Registrati" } }
        }
    }
}

#[component]
pub fn Register() -> Element {
    let mut session = use_session();
    let nav = use_navigator();
    let mut username = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |ev: FormEvent| async move {
        ev.prevent_default();
        let req = RegisterRequest {
            username: username(),
            email: email(),
            password: password(),
        };
        // Stessa validazione del server, dal crate condiviso.
        if let Err(e) = validate_registration(&req) {
            error.set(Some(e));
            return;
        }
        busy.set(true);
        match api::register(&req).await {
            Ok(r) => {
                let s = Session {
                    token: r.token,
                    user: r.user,
                };
                api::save_session(Some(&s));
                session.set(Some(s));
                nav.push(Route::Play {});
            }
            Err(e) => error.set(Some(e)),
        }
        busy.set(false);
    };

    rsx! {
        section { class: "card-panel narrow",
            h2 { "Crea un account" }
            form { onsubmit: submit,
                label { "Username"
                    input { value: "{username}", autocomplete: "username", required: true, maxlength: 20,
                        oninput: move |e| username.set(e.value()) }
                }
                label { "Email"
                    input { r#type: "email", value: "{email}", autocomplete: "email", required: true,
                        oninput: move |e| email.set(e.value()) }
                }
                label { "Password (min. 8 caratteri)"
                    input { r#type: "password", value: "{password}", autocomplete: "new-password", required: true,
                        oninput: move |e| password.set(e.value()) }
                }
                if let Some(e) = error() { p { class: "error", "{e}" } }
                button { class: "btn", r#type: "submit", disabled: busy(), "Registrati" }
            }
            p { class: "muted", "Hai già un account? " Link { to: Route::Login {}, "Accedi" } }
        }
    }
}
