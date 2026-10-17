# syntax=docker/dockerfile:1
# Build multi-stage: frontend WASM + backend Axum in un'unica immagine leggera.

ARG RUST_VERSION=1.90

FROM rust:${RUST_VERSION}-bookworm AS chef
WORKDIR /app
RUN rustup target add wasm32-unknown-unknown

# --- Frontend (Dioxus -> WASM) ---
FROM chef AS frontend
# La versione deve coincidere con quella di wasm-bindgen in Cargo.lock.
ARG WASM_BINDGEN_VERSION=0.2.129
RUN cargo install wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} --locked
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build -p frontend --release --target wasm32-unknown-unknown \
 && wasm-bindgen --target web --no-typescript --out-dir /dist \
      target/wasm32-unknown-unknown/release/frontend.wasm \
 && cp frontend/index.html /dist/index.html

# --- Backend ---
FROM chef AS backend
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build -p backend --release \
 && cp target/release/backend /backend

# --- Runtime ---
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 app
WORKDIR /app
COPY --from=backend /backend /app/backend
COPY --from=frontend /dist /app/dist
ENV BIND_ADDR=0.0.0.0:3000 STATIC_DIR=/app/dist RUST_LOG=backend=info,tower_http=warn
USER app
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --retries=5 CMD curl -fsS http://localhost:3000/api/health || exit 1
CMD ["/app/backend"]
