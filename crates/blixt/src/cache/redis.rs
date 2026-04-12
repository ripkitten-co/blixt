use std::time::Duration;

use deadpool_redis::{Config, Pool, Runtime, redis::AsyncCommands};

/// Redis-backed cache using a connection pool.
///
/// Stores values as byte strings with `SET key value EX ttl`. Suitable for
/// multi-instance deployments where all nodes share cache state.
pub struct RedisCache {
    pool: Pool,
}

impl RedisCache {
    /// Creates a new Redis cache connecting to the given URL.
    ///
    /// `pool_size` controls the maximum number of concurrent connections.
    pub fn new(url: &str, pool_size: usize) -> crate::error::Result<Self> {
        let cfg = Config::from_url(url);
        let pool = cfg
            .builder()
            .map_err(|e| crate::error::Error::Internal(format!("Redis pool config: {e}")))?
            .max_size(pool_size)
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(|e| crate::error::Error::Internal(format!("Redis pool build: {e}")))?;
        tracing::info!(url = %url, pool_size, "Redis cache initialized");
        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl super::CacheBackend for RedisCache {
    async fn get_bytes(&self, key: &str) -> crate::error::Result<Option<Vec<u8>>> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis connection: {e}")))?;
        let val: Option<Vec<u8>> = conn
            .get(key)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis GET: {e}")))?;
        Ok(val)
    }

    async fn set_bytes(&self, key: &str, value: &[u8], ttl: Duration) -> crate::error::Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis connection: {e}")))?;
        let ttl_secs = ttl.as_secs().max(1);
        conn.set_ex::<_, _, ()>(key, value, ttl_secs)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis SET: {e}")))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> crate::error::Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis connection: {e}")))?;
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis DEL: {e}")))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> crate::error::Result<bool> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis connection: {e}")))?;
        let val: bool = conn
            .exists(key)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("Redis EXISTS: {e}")))?;
        Ok(val)
    }
}
