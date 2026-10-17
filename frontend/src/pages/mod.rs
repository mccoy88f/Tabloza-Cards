mod auth;
mod home;
mod leaderboard;
mod play;
mod profile;

pub use auth::{Login, Register};
pub use home::{Home, NotFound};
pub use leaderboard::Leaderboard;
pub use play::Play;
pub use profile::Profile;
