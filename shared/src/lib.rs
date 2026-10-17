//! Tipi condivisi tra backend (Axum) e frontend (Dioxus/WASM).

pub mod cards;
pub mod dto;
pub mod protocol;
pub mod scopa;

pub use cards::{full_deck, Card, Suit};
