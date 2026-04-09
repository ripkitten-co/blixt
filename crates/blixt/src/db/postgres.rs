use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::config::Config;
use crate::error::{Error, Result};

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
