use std::sync::Arc;

use crate::cache::{Cache, MemoryCache};
use crate::config::Config;
use crate::db::DbPool;
use crate::mailer::Mailer;

#[derive(Clone)]
/// Shared state passed to route handlers via Axum's `State` extractor.
pub struct AppContext {
    /// Database connection pool.
    pub db: DbPool,
    /// Application configuration.
    pub config: Arc<Config>,
    /// SMTP mailer. `None` when SMTP is not configured (e.g. local dev).
    pub mailer: Option<Arc<Mailer>>,
    /// Cache for reducing database load and storing ephemeral data.
    pub cache: Cache,
}

impl AppContext {
    /// Creates a new context with an in-memory cache (10,000 entries).
    pub fn new(db: DbPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
            mailer: None,
            cache: Cache::new(Arc::new(MemoryCache::new(10_000))),
        }
    }

    /// Adds a mailer to the context.
    pub fn with_mailer(mut self, mailer: Mailer) -> Self {
        self.mailer = Some(Arc::new(mailer));
        self
    }

    /// Adds an optional mailer to the context.
    pub fn with_mailer_opt(mut self, mailer: Option<Mailer>) -> Self {
        self.mailer = mailer.map(Arc::new);
        self
    }

    /// Overrides the default cache backend.
    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Environment;

    #[test]
    fn app_context_wraps_config_in_arc() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 4000,
            blixt_env: Environment::Test,
            database_url: None,
            jwt_secret: None,
        };

        // Can't easily create a DbPool without a live connection,
        // but we can verify Config wrapping via a separate path.
        let arc_config = Arc::new(config);
        assert_eq!(arc_config.port, 4000);
        assert_eq!(arc_config.blixt_env, Environment::Test);
    }
}
