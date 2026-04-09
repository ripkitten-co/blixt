use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

/// Shared application state passed to Axum handlers via `State<AppContext>`.
///
/// Wraps `Config` in `Arc` because `Config` contains `SecretString` fields
/// and does not implement `Clone` directly.
#[derive(Clone)]
pub struct AppContext {
    pub db: PgPool,
    pub config: Arc<Config>,
}

impl AppContext {
    /// Creates a new application context from a database pool and configuration.
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }
}
