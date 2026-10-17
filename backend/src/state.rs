use crate::auth::JwtKeys;
use crate::hub::Hub;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt: Arc<JwtKeys>,
    pub hub: Arc<Hub>,
}
