#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

/// Type-safe query builder for CRUD operations.
pub mod builder;
mod macros;

#[cfg(feature = "postgres")]
pub use self::postgres::create_pool;
#[cfg(feature = "sqlite")]
pub use self::sqlite::create_pool;

/// Connection pool for the PostgreSQL backend.
#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type DbPool = sqlx::PgPool;
/// Connection pool for the SQLite backend.
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type DbPool = sqlx::SqlitePool;

/// Run all pending database migrations from the `./migrations` directory.
pub async fn migrate(pool: &DbPool) -> crate::error::Result<()> {
    migrate_from(pool, std::path::Path::new("./migrations")).await
}

/// Run migrations from a specific directory.
pub async fn migrate_from(pool: &DbPool, path: &std::path::Path) -> crate::error::Result<()> {
    let migrator = sqlx::migrate::Migrator::new(path)
        .await
        .map_err(|e| crate::error::Error::Internal(format!("failed to load migrations: {e}")))?;
    migrator
        .run(pool)
        .await
        .map_err(|e| crate::error::Error::Internal(format!("migration failed: {e}")))?;
    tracing::info!("database migrations applied");
    Ok(())
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

        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("DATABASE_URL not configured"),
            "expected missing DATABASE_URL error, got: {msg}"
        );
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn create_pool_connects_with_valid_url() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            blixt_env: Environment::Test,
            database_url: Some(secrecy::SecretString::from("sqlite::memory:".to_string())),
            jwt_secret: None,
        };

        let result = create_pool(&config).await;
        assert!(result.is_ok(), "pool should connect: {:?}", result.err());
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn create_pool_fails_with_invalid_url() {
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            blixt_env: Environment::Test,
            database_url: Some(secrecy::SecretString::from("not-a-valid-url".to_string())),
            jwt_secret: None,
        };

        let result = create_pool(&config).await;
        assert!(result.is_err());
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn migrate_succeeds_with_empty_migrations_dir() {
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        let migrations_dir = tmp.path().join("migrations");
        std::fs::create_dir(&migrations_dir).expect("create dir");

        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            blixt_env: Environment::Test,
            database_url: Some(secrecy::SecretString::from("sqlite::memory:".to_string())),
            jwt_secret: None,
        };
        let pool = create_pool(&config).await.expect("pool");
        let result = migrate_from(&pool, &migrations_dir).await;
        assert!(result.is_ok(), "migrate failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn migrate_fails_with_missing_directory() {
        let result = sqlx::migrate::Migrator::new(std::path::Path::new("/nonexistent/path")).await;
        assert!(result.is_err());
    }
}
