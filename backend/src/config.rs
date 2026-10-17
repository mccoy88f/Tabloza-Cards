use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub bind_addr: String,
    pub static_dir: String,
    pub cors_origin: Option<String>,
    pub token_ttl_hours: i64,
    /// Indirizzo pubblico del sito, usato nei link delle email.
    pub public_url: String,
    /// `None` = email disabilitate: account attivi subito, niente recupero password.
    pub smtp: Option<SmtpConfig>,
}

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// `starttls` (default), `tls` (implicito, porta 465) oppure `none` (solo test).
    pub tls: String,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_secret = std::env::var("JWT_SECRET").context("JWT_SECRET mancante")?;
        anyhow::ensure!(
            jwt_secret.len() >= 32,
            "JWT_SECRET deve avere almeno 32 caratteri"
        );
        Ok(Config {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL mancante")?,
            jwt_secret,
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into()),
            static_dir: std::env::var("STATIC_DIR").unwrap_or_else(|_| "dist".into()),
            cors_origin: std::env::var("CORS_ORIGIN").ok().filter(|s| !s.is_empty()),
            token_ttl_hours: std::env::var("TOKEN_TTL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24 * 7),
            public_url: var("PUBLIC_URL").unwrap_or_else(|| "http://localhost:3000".into()),
            smtp: smtp_from_env()?,
        })
    }
}

fn smtp_from_env() -> anyhow::Result<Option<SmtpConfig>> {
    let Some(host) = var("SMTP_HOST") else {
        return Ok(None);
    };
    let tls = var("SMTP_TLS")
        .unwrap_or_else(|| "starttls".into())
        .to_lowercase();
    anyhow::ensure!(
        matches!(tls.as_str(), "starttls" | "tls" | "none"),
        "SMTP_TLS deve essere starttls, tls oppure none"
    );
    let default_port = if tls == "tls" { 465 } else { 587 };
    Ok(Some(SmtpConfig {
        host,
        port: match var("SMTP_PORT") {
            Some(p) => p.parse().context("SMTP_PORT non valida")?,
            None => default_port,
        },
        user: var("SMTP_USER").context("SMTP_USER mancante")?,
        password: var("SMTP_PASSWORD").context("SMTP_PASSWORD mancante")?,
        tls,
    }))
}
