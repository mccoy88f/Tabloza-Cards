//! Client HTTP verso le API REST e gestione della sessione (token in localStorage).

use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use shared::dto::{
    ApiError, AuthResponse, LeaderboardEntry, LoginRequest, MatchRecord, RegisterRequest,
    UserProfile,
};

const STORAGE_KEY: &str = "tabloza.session";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub user: UserProfile,
}

/// Base delle API. Vuota = stessa origine (deploy Docker). In sviluppo con `dx serve`
/// si può compilare con `TABLOZA_API=http://localhost:3000`.
pub fn api_base() -> String {
    option_env!("TABLOZA_API")
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string()
}

pub fn ws_url(token: &str) -> String {
    let base = api_base();
    let origin = if base.is_empty() {
        let loc = web_sys::window().expect("window").location();
        let proto = if loc.protocol().unwrap_or_default() == "https:" {
            "wss:"
        } else {
            "ws:"
        };
        format!("{proto}//{}", loc.host().unwrap_or_default())
    } else {
        base.replacen("http", "ws", 1)
    };
    format!("{origin}/ws?token={}", js_sys::encode_uri_component(token))
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn load_session() -> Option<Session> {
    let raw = storage()?.get_item(STORAGE_KEY).ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn save_session(s: Option<&Session>) {
    if let Some(st) = storage() {
        match s {
            Some(s) => {
                let _ = st.set_item(STORAGE_KEY, &serde_json::to_string(s).unwrap_or_default());
            }
            None => {
                let _ = st.remove_item(STORAGE_KEY);
            }
        }
    }
}

async fn parse<T: DeserializeOwned>(resp: Response) -> Result<T, String> {
    if resp.ok() {
        return resp.json::<T>().await.map_err(|e| e.to_string());
    }
    if resp.status() == 401 {
        return Err("Credenziali non valide o sessione scaduta".into());
    }
    match resp.json::<ApiError>().await {
        Ok(e) => Err(e.error),
        Err(_) => Err(format!("Errore {}", resp.status())),
    }
}

async fn post<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let resp = Request::post(&format!("{}{path}", api_base()))
        .json(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|_| "Server non raggiungibile".to_string())?;
    parse(resp).await
}

async fn get<T: DeserializeOwned>(path: &str, token: Option<&str>) -> Result<T, String> {
    let mut req = Request::get(&format!("{}{path}", api_base()));
    if let Some(t) = token {
        req = req.header("Authorization", &format!("Bearer {t}"));
    }
    let resp = req
        .send()
        .await
        .map_err(|_| "Server non raggiungibile".to_string())?;
    parse(resp).await
}

pub async fn register(req: &RegisterRequest) -> Result<AuthResponse, String> {
    post("/api/auth/register", req).await
}

pub async fn login(req: &LoginRequest) -> Result<AuthResponse, String> {
    post("/api/auth/login", req).await
}

pub async fn me(token: &str) -> Result<UserProfile, String> {
    get("/api/me", Some(token)).await
}

pub async fn my_matches(token: &str) -> Result<Vec<MatchRecord>, String> {
    get("/api/me/matches?limit=20", Some(token)).await
}

pub async fn leaderboard() -> Result<Vec<LeaderboardEntry>, String> {
    get("/api/leaderboard?limit=50", None).await
}
