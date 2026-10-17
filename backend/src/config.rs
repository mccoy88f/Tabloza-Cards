use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub bind_addr: String,
    pub static_dir: String,
    pub cors_origin: Option<String>,
    pub token_ttl_hours: i64,
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
        })
    }
}
