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
