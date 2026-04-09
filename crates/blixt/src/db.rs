use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::error::{Error, Result};

/// Creates a PostgreSQL connection pool from the application configuration.
///
/// Verifies connectivity by executing a simple query before returning.
/// Returns `Error::Internal` if `DATABASE_URL` is not configured.
pub async fn create_pool(config: &Config) -> Result<PgPool> {
    let url = config
        .database_url()
        .ok_or_else(|| Error::Internal("DATABASE_URL not configured".into()))?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect(url)
        .await?;

    sqlx::query("SELECT 1").execute(&pool).await?;

    tracing::info!("database connection pool established");
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Environment};

    #[tokio::test]
    async fn create_pool_fails_without_database_url() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            blixt_env: Environment::Test,
            database_url: None,
            jwt_secret: None,
        };

        let result = create_pool(&config).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("DATABASE_URL not configured"),
            "expected missing DATABASE_URL error, got: {msg}"
        );
    }
}
