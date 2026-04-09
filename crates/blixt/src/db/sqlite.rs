use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

use crate::config::Config;
use crate::error::{Error, Result};

pub async fn create_pool(config: &Config) -> Result<SqlitePool> {
    let url = config
        .database_url()
        .ok_or_else(|| Error::Internal("DATABASE_URL not configured".into()))?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;

    sqlx::query("SELECT 1").execute(&pool).await?;

    tracing::info!("database connection pool established");
    Ok(pool)
}
