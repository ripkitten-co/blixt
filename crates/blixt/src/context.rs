use std::sync::Arc;

use crate::config::Config;
use crate::db::DbPool;

#[derive(Clone)]
pub struct AppContext {
    pub db: DbPool,
    pub config: Arc<Config>,
}

impl AppContext {
    pub fn new(db: DbPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }
}
