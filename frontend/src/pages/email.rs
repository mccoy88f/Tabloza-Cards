//! Conferma dell'email e recupero password.

use crate::api::{self, Session};
use crate::{use_session, Route};
use dioxus::prelude::*;
use shared::dto::validate_password;

/// Mostrata al posto della lobby finché l'email non è confermata.
#[component]
pub fn VerifyNotice() -> Element {
    let mut session = use_session();
    let mut info = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);
    let email = session
        .read()
        .as_ref()
        .map(|s| s.user.email.clone())
        .unwrap_or_default();

    let resend = move |_| async move {
        let Some(token) = session.peek().as_ref().map(|s| s.token.clone()) else {
            return;
        };
        busy.set(true);
        info.set(Some(
            api::resend_verification(&token).await.map(|r| r.message),
        ));
        busy.set(false);
    };

    // "Ho confermato": ricarica il profilo dal server.
    let check = move |_| async move {
        let Some(token) = session.peek().as_ref().map(|s| s.token.clone()) else {
            return;
        };
        busy.set(true);
        match api::me(&token).await {
            Ok(user) if user.email_verified => {
                let s = Session { token, user };
                api::save_session(Some(&s));
                session.set(Some(s));
            }
            Ok(_) => info.set(Some(Err(
                "Email non ancora confermata: controlla la posta (anche lo spam)".into(),
            ))),
            Err(e) => info.set(Some(Err(e))),
        }
        busy.set(false);
    };

    rsx! {
        section { class: "card-panel narrow",
            h2 { "Conferma la tua email" }
            p { "Ti abbiamo inviato un link di conferma a " b { "{email}" } "." }
            p { class: "muted", "Aprilo per attivare l'account e iniziare a giocare. Il link vale 48 ore." }
            match info() {
                Some(Ok(m)) => rsx! { p { class: "success", "{m}" } },
                Some(Err(e)) => rsx! { p { class: "error", "{e}" } },
                None => rsx! {},
            }
            div { class: "actions-row",
                button { class: "btn", disabled: busy(), onclick: check, "Ho confermato" }
                button { class: "btn ghost", disabled: busy(), onclick: resend, "Invia di nuovo" }
            }
        }
    }
}

/// Atterraggio del link `/verify?token=…` ricevuto via email.
#[component]
pub fn VerifyEmail(token: String) -> Element {
    let mut session = use_session();
    let result = use_resource(move || {
        let token = token.clone();
        async move { api::verify_email(&token).await.map(|r| r.message) }
    });

    // Se l'utente è loggato in questo browser, aggiorniamo subito la sessione.
    use_effect(move || {
        if matches!(&*result.read(), Some(Ok(_))) {
            let current = session.peek().clone();
            if let Some(mut s) = current {
                if !s.user.email_verified {
                    s.user.email_verified = true;
                    api::save_session(Some(&s));
                    session.set(Some(s));
                }
            }
        }
    });

    let logged = session.read().is_some();
    rsx! {
        section { class: "card-panel narrow",
            h2 { "Conferma email" }
            match &*result.read() {
                None => rsx! { p { class: "muted", "Verifica in corso…" } },
                Some(Ok(m)) => rsx! {
                    p { class: "success", "{m}" }
                    if logged {
                        Link { class: "btn", to: Route::Play {}, "Vai alla lobby" }
                    } else {
                        Link { class: "btn", to: Route::Login {}, "Accedi" }
                    }
                },
                Some(Err(e)) => rsx! {
                    p { class: "error", "{e}" }
                    p { class: "muted", "Accedi e usa \"Invia di nuovo\" per ricevere un nuovo link." }
                },
            }
        }
    }
}

#[component]
pub fn ForgotPassword() -> Element {
    let mut email = use_signal(String::new);
    let mut info = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);

    let submit = move |ev: FormEvent| async move {
        ev.prevent_default();
        busy.set(true);
        info.set(Some(
            api::forgot_password(&email()).await.map(|r| r.message),
        ));
        busy.set(false);
    };

    rsx! {
        section { class: "card-panel narrow",
            h2 { "Password dimenticata" }
            p { class: "muted", "Inserisci l'email dell'account: ti invieremo un link per sceglierne una nuova." }
            form { onsubmit: submit,
                label { "Email"
                    input { r#type: "email", value: "{email}", autocomplete: "email", required: true,
                        oninput: move |e| email.set(e.value()) }
                }
                match info() {
                    Some(Ok(m)) => rsx! { p { class: "success", "{m}" } },
                    Some(Err(e)) => rsx! { p { class: "error", "{e}" } },
                    None => rsx! {},
                }
                button { class: "btn", r#type: "submit", disabled: busy(), "Invia il link" }
            }
            p { class: "muted", Link { to: Route::Login {}, "Torna all'accesso" } }
        }
    }
}

#[component]
pub fn ResetPassword(token: String) -> Element {
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut info = use_signal(|| None::<Result<String, String>>);
    let mut busy = use_signal(|| false);
    let done = matches!(info(), Some(Ok(_)));

    let submit = move |ev: FormEvent| {
        let token = token.clone();
        async move {
            ev.prevent_default();
            if password() != confirm() {
                info.set(Some(Err("Le due password non coincidono".into())));
                return;
            }
            if let Err(e) = validate_password(&password()) {
                info.set(Some(Err(e)));
                return;
            }
            busy.set(true);
            info.set(Some(
                api::reset_password(&token, &password())
                    .await
                    .map(|r| r.message),
            ));
            busy.set(false);
        }
    };

    rsx! {
        section { class: "card-panel narrow",
            h2 { "Nuova password" }
            if done {
                if let Some(Ok(m)) = info() { p { class: "success", "{m}" } }
                Link { class: "btn", to: Route::Login {}, "Accedi" }
            } else {
                form { onsubmit: submit,
                    label { "Nuova password (min. 8 caratteri)"
                        input { r#type: "password", value: "{password}", autocomplete: "new-password", required: true,
                            oninput: move |e| password.set(e.value()) }
                    }
                    label { "Ripeti la password"
                        input { r#type: "password", value: "{confirm}", autocomplete: "new-password", required: true,
                            oninput: move |e| confirm.set(e.value()) }
                    }
                    if let Some(Err(e)) = info() { p { class: "error", "{e}" } }
                    button { class: "btn", r#type: "submit", disabled: busy(), "Salva" }
                }
            }
        }
    }
}
