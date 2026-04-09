#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "postgres")]
pub use self::postgres::create_pool;
#[cfg(feature = "sqlite")]
pub use self::sqlite::create_pool;

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
pub type DbPool = sqlx::PgPool;
#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub type DbPool = sqlx::SqlitePool;

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
}
