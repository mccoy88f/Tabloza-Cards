mod auth;
mod config;
mod db;
mod error;
mod hub;
mod mailer;
mod routes;
mod state;
mod ws;

use crate::auth::JwtKeys;
use crate::config::Config;
use crate::hub::Hub;
use crate::state::AppState;
use axum::http::{header, HeaderValue, Method};
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=info,tower_http=info".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    let db = connect_db(&cfg.database_url).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("migrazioni applicate");

    // Salvataggio asincrono delle partite concluse.
    let (finished_tx, mut finished_rx) = mpsc::unbounded_channel::<db::FinishedMatch>();
    {
        let db = db.clone();
        tokio::spawn(async move {
            while let Some(m) = finished_rx.recv().await {
                if let Err(e) = db::record_match(&db, &m).await {
                    tracing::error!(error = %e, "salvataggio partita fallito");
                }
            }
        });
    }

    let hub = Arc::new(Hub::new(finished_tx));
    {
        let hub = hub.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(60));
            loop {
                t.tick().await;
                hub.sweep();
            }
        });
    }

    let mailer = match &cfg.smtp {
        Some(smtp) => {
            let m = mailer::Mailer::new(smtp, &cfg.public_url)?;
            if m.check().await {
                tracing::info!(host = %smtp.host, "SMTP pronto");
            } else {
                tracing::warn!(host = %smtp.host, "server SMTP non raggiungibile: le email potrebbero non partire");
            }
            Some(Arc::new(m))
        }
        None => {
            tracing::info!("SMTP non configurato: conferma email e recupero password disattivati");
            None
        }
    };

    let state = AppState {
        db,
        jwt: Arc::new(JwtKeys::new(&cfg.jwt_secret, cfg.token_ttl_hours)),
        hub,
        mailer,
    };

    let index = format!("{}/index.html", cfg.static_dir);
    let spa = ServeDir::new(&cfg.static_dir).fallback(ServeFile::new(index));

    let mut app = Router::new()
        .nest("/api", routes::router())
        .route("/ws", get(ws::ws_handler))
        .fallback_service(spa)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if let Some(origin) = &cfg.cors_origin {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(origin.parse::<HeaderValue>()?)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    tracing::info!("in ascolto su http://{}", cfg.bind_addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// Il database in Docker può impiegare qualche secondo ad avviarsi.
async fn connect_db(url: &str) -> anyhow::Result<sqlx::PgPool> {
    let mut attempt = 0;
    loop {
        match PgPoolOptions::new().max_connections(10).connect(url).await {
            Ok(p) => return Ok(p),
            Err(e) if attempt < 30 => {
                attempt += 1;
                tracing::warn!(error = %e, attempt, "database non pronto, riprovo");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("arresto in corso");
}
