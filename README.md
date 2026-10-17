# Tabloza-Cards

Piattaforma online per giocare a carte all'italiana contro altri giocatori, con registrazione.
Il primo gioco disponibile è la **Scopa** (2 giocatori, mazzo da 40).

Full-stack in Rust, monorepo Cargo workspace:

| Crate      | Ruolo                                                                           |
|------------|----------------------------------------------------------------------------------|
| `shared`   | Carte, motore di gioco della Scopa, DTO REST, protocollo WebSocket (Serde)       |
| `backend`  | Axum + Tokio: API REST, WebSocket in tempo reale, PostgreSQL (sqlx), JWT, Argon2 |
| `frontend` | Dioxus 0.7 (WASM/Web): login, registrazione, lobby, tavolo, classifica, profilo  |

## Architettura

```
Browser (Dioxus/WASM) ──HTTP JSON──▶ /api/*        (registrazione, login, profilo, classifica)
                      ◀─WS binario─▶ /ws?token=…   (lobby, stanze, partite, chat)
                                         │
                               Axum ── Hub in memoria (stanze + partite)
                                         │  partite concluse (canale mpsc)
                                         ▼
                                     PostgreSQL (utenti, partite, Elo)
```

- **WebSocket leggeri**: i messaggi (`ClientMsg` / `ServerMsg` in `shared/src/protocol.rs`)
  viaggiano come frame **binari `postcard`**: una giocata pesa ~10 byte invece di ~150 in JSON.
  Gli stessi tipi Rust sono usati da server e client, quindi niente disallineamenti.
- **Server autoritativo**: il server valida ogni mossa con `shared::scopa::ScopaGame` e invia
  a ciascun giocatore una `GameView` personalizzata (la mano avversaria resta nascosta).
- Il frontend usa la stessa funzione `capture_options` per proporre le prese possibili.
- **Riconnessione**: ricaricando la pagina si torna al proprio tavolo; un tavolo senza giocatori
  connessi per 5 minuti viene chiuso. Lasciare una partita in corso = sconfitta a tavolino.
- Ping WebSocket ogni 20 s; il client si riconnette con backoff esponenziale.
- Le password sono salvate con **Argon2**, l'autenticazione usa **JWT** (HS256).
- A fine partita vengono salvati risultato, statistiche e punteggio **Elo** (K=32).
- **Email (opzionale)**: con l'SMTP configurato, alla registrazione parte un link di conferma
  (valido 48 ore) e finché l'email non è confermata non si può giocare; c'è anche il recupero
  password con link valido 1 ora. I token sono monouso e nel database ne è salvato solo l'hash
  SHA-256; si può richiedere al massimo un'email al minuto per tipo. Senza SMTP gli account sono
  attivi subito e il recupero password non è disponibile.

### Regole implementate

3 carte a testa e 4 in tavola (si ridistribuisce con 3+ Re in tavola); presa obbligatoria
della carta di pari valore (ha la precedenza sulla somma), altrimenti presa per somma;
scopa quando si svuota la tavola (non all'ultima giocata); le carte rimaste vanno all'ultimo
che ha preso. Punti: carte, denari, settebello, primiera, scope. Si gioca a 11/16/21/31.

## API

| Metodo | Percorso                | Auth   | Descrizione                         |
|--------|-------------------------|--------|-------------------------------------|
| POST   | `/api/auth/register`    | –      | `{username, email, password}`       |
| POST   | `/api/auth/login`       | –      | `{login, password}` (username/email)|
| POST   | `/api/auth/verify`      | –      | `{token}` dal link di conferma      |
| POST   | `/api/auth/resend-verification` | Bearer | Reinvia l'email di conferma |
| POST   | `/api/auth/forgot-password` | –  | `{email}`: invia il link di reset   |
| POST   | `/api/auth/reset-password`  | –  | `{token, password}`                 |
| GET    | `/api/config`           | –      | `{email_enabled}`                   |
| GET    | `/api/me`               | Bearer | Profilo utente                      |
| GET    | `/api/me/matches`       | Bearer | Ultime partite                      |
| GET    | `/api/leaderboard`      | –      | Classifica Elo                      |
| GET    | `/api/health`           | –      | Health check                        |
| GET    | `/ws?token=<jwt>`       | JWT    | WebSocket di gioco (frame binari)   |

## Avvio con Docker Compose

```bash
cp .env.example .env
# imposta JWT_SECRET (es. `openssl rand -hex 32`)
docker compose -f docker-compose.yml -f docker-compose.local.yml up --build
```

Apri <http://localhost:3000>. `docker-compose.local.yml` pubblica le porte sull'host (app su
`APP_PORT`, database su `127.0.0.1:5432`); in produzione dietro un reverse proxy (es. Coolify,
dominio sul servizio `app`, porta 3000) basta `docker-compose.yml`. L'immagine compila il frontend in WASM (`wasm-bindgen`) e il
backend, poi serve entrambi da un unico container; PostgreSQL ha un volume persistente e le
migrazioni (`backend/migrations`) vengono applicate all'avvio.

### Variabili d'ambiente

| Variabile | Obbligatoria | Descrizione |
|-----------|--------------|-------------|
| `JWT_SECRET` | sì | Almeno 32 caratteri casuali |
| `POSTGRES_PASSWORD` | consigliata | Password del database (default `tabloza`) |
| `POSTGRES_USER`, `POSTGRES_DB` | no | Default `tabloza` |
| `PUBLIC_URL` | con SMTP | Indirizzo pubblico, es. `https://scopa.tuodominio.it` |
| `SMTP_HOST` | no | Server SMTP; vuoto = email disattivate |
| `SMTP_PORT` | no | Default 587 (465 con `SMTP_TLS=tls`) |
| `SMTP_USER` | con SMTP | Account SMTP, usato anche come mittente |
| `SMTP_PASSWORD` | con SMTP | Password SMTP |
| `SMTP_TLS` | no | `starttls` (default), `tls` oppure `none` |
| `TOKEN_TTL_HOURS` | no | Durata del login, default 168 |

## Sviluppo locale

```bash
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d db   # solo il database
cp .env.example .env                        # DATABASE_URL punta a localhost

# Frontend: build una tantum servita dal backend
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # = versione in Cargo.lock
cargo build -p frontend --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir dist \
  target/wasm32-unknown-unknown/release/frontend.wasm
cp frontend/index.html dist/

cargo run -p backend                        # http://localhost:3000
```

In alternativa, con hot-reload: `CORS_ORIGIN=http://localhost:8080` nel `.env` del backend e
`cd frontend && TABLOZA_API=http://localhost:3000 dx serve` (Dioxus CLI 0.7).

Test: `cargo test` (motore di gioco, protocollo, hub, Elo).

## Aggiungere altri giochi

`GameKind` in `shared/src/protocol.rs` identifica il gioco di una stanza. Per un nuovo gioco
(Briscola, Tressette…) si aggiunge il suo motore in `shared`, una variante a `GameKind` e la
relativa vista/tavolo nel frontend; hub, lobby, persistenza e classifica restano invariati.
