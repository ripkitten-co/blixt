#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "postgres")]
pub use self::postgres::*;
#[cfg(feature = "sqlite")]
pub use self::sqlite::*;

#[cfg(feature = "postgres")]
pub type DbPool = sqlx::PgPool;
#[cfg(feature = "sqlite")]
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
}
