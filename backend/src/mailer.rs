//! Invio email via SMTP (conferma account e recupero password).
//! Il mittente è l'account `SMTP_USER`, che quindi deve essere un indirizzo email.

use crate::config::SmtpConfig;
use anyhow::Context;
use lettre::message::{header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    public_url: String,
}

impl Mailer {
    pub fn new(cfg: &SmtpConfig, public_url: &str) -> anyhow::Result<Self> {
        let address = cfg
            .user
            .parse()
            .context("SMTP_USER deve essere un indirizzo email (è usato come mittente)")?;
        let builder = match cfg.tls.as_str() {
            // TLS implicito (di solito porta 465)
            "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?,
            // Solo per test in locale (es. Mailpit)
            "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host),
            // STARTTLS (di solito porta 587)
            _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)?,
        };
        let transport = builder
            .port(cfg.port)
            .credentials(Credentials::new(cfg.user.clone(), cfg.password.clone()))
            .build();
        Ok(Mailer {
            transport,
            from: Mailbox::new(Some("Tabloza".into()), address),
            public_url: public_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn check(&self) -> bool {
        self.transport.test_connection().await.unwrap_or(false)
    }

    async fn send(&self, to: &str, subject: &str, body: String) -> anyhow::Result<()> {
        let msg = Message::builder()
            .from(self.from.clone())
            .to(to.parse().context("destinatario non valido")?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)?;
        self.transport.send(msg).await?;
        Ok(())
    }

    pub async fn send_verification(
        &self,
        to: &str,
        username: &str,
        token: &str,
    ) -> anyhow::Result<()> {
        let link = format!("{}/verify?token={token}", self.public_url);
        self.send(
            to,
            "Conferma il tuo account Tabloza",
            format!(
                "Ciao {username},\n\nbenvenuto su Tabloza! Per confermare il tuo indirizzo email e \
                 iniziare a giocare apri questo link:\n\n{link}\n\nIl link scade tra 48 ore.\n\
                 Se non ti sei registrato tu, ignora questa email.\n"
            ),
        )
        .await
    }

    pub async fn send_reset(&self, to: &str, username: &str, token: &str) -> anyhow::Result<()> {
        let link = format!("{}/reset?token={token}", self.public_url);
        self.send(
            to,
            "Reimposta la password di Tabloza",
            format!(
                "Ciao {username},\n\nabbiamo ricevuto una richiesta di reimpostare la tua password. \
                 Per sceglierne una nuova apri questo link:\n\n{link}\n\nIl link scade tra 1 ora.\n\
                 Se non hai fatto tu la richiesta, ignora questa email: la password resta invariata.\n"
            ),
        )
        .await
    }
}
