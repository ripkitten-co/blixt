//! Caching abstraction with pluggable backends.
//!
//! Provides a `CacheBackend` trait with in-memory and optional Redis backends.
//! The `Cache` wrapper adds typed access via serde. The memory backend is
//! always available; Redis requires the `redis` cargo feature.

mod memory;
#[cfg(feature = "redis")]
mod redis;

#[cfg(feature = "redis")]
pub use self::redis::RedisCache;
pub use memory::MemoryCache;

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Low-level cache backend operating on raw bytes.
///
/// Implement this for new backends. Use [`Cache`] for the typed API.
#[async_trait::async_trait]
pub trait CacheBackend: Send + Sync {
    /// Retrieves raw bytes by key, returning `None` on miss.
    async fn get_bytes(&self, key: &str) -> crate::error::Result<Option<Vec<u8>>>;

    /// Stores raw bytes with the given TTL.
    async fn set_bytes(&self, key: &str, value: &[u8], ttl: Duration) -> crate::error::Result<()>;

    /// Removes a key.
    async fn delete(&self, key: &str) -> crate::error::Result<()>;

    /// Returns `true` if the key exists and has not expired.
    async fn exists(&self, key: &str) -> crate::error::Result<bool>;
}

/// Typed cache wrapper around a [`CacheBackend`].
///
/// Handles JSON serialization/deserialization so callers work with
/// concrete types rather than raw bytes.
#[derive(Clone)]
pub struct Cache {
    backend: Arc<dyn CacheBackend>,
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache").finish_non_exhaustive()
    }
}

impl Cache {
    /// Wraps a backend in a typed cache.
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self { backend }
    }

    /// Retrieves a typed value by key.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> crate::error::Result<Option<T>> {
        let Some(bytes) = self.backend.get_bytes(key).await? else {
            return Ok(None);
        };
        let value = serde_json::from_slice(&bytes)
            .map_err(|e| crate::error::Error::Internal(format!("cache deserialize: {e}")))?;
        Ok(Some(value))
    }

    /// Stores a typed value with the given TTL.
    pub async fn set<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> crate::error::Result<()> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| crate::error::Error::Internal(format!("cache serialize: {e}")))?;
        self.backend.set_bytes(key, &bytes, ttl).await
    }

    /// Removes a key.
    pub async fn delete(&self, key: &str) -> crate::error::Result<()> {
        self.backend.delete(key).await
    }

    /// Returns `true` if the key exists and has not expired.
    pub async fn exists(&self, key: &str) -> crate::error::Result<bool> {
        self.backend.exists(key).await
    }
}

/// Creates a [`Cache`] from environment variables.
///
/// Reads `CACHE_BACKEND` (`memory` or `redis`, default `memory`) and
/// backend-specific variables.
pub fn from_env() -> crate::error::Result<Cache> {
    let backend = std::env::var("CACHE_BACKEND").unwrap_or_else(|_| "memory".into());
    match backend.as_str() {
        "memory" => {
            let max_entries: u64 = std::env::var("CACHE_MAX_ENTRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000);
            Ok(Cache::new(Arc::new(MemoryCache::new(max_entries))))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = std::env::var("REDIS_URL").map_err(|_| {
                crate::error::Error::Internal(
                    "CACHE_BACKEND=redis requires REDIS_URL to be set".into(),
                )
            })?;
            let pool_size: usize = std::env::var("REDIS_POOL_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8);
            let cache = RedisCache::new(&url, pool_size)?;
            Ok(Cache::new(Arc::new(cache)))
        }
        #[cfg(not(feature = "redis"))]
        "redis" => Err(crate::error::Error::Internal(
            "CACHE_BACKEND=redis requires the `redis` cargo feature".into(),
        )),
        other => Err(crate::error::Error::Internal(format!(
            "Unknown CACHE_BACKEND: '{other}'. Use 'memory' or 'redis'."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::with_env_vars;

    #[test]
    fn from_env_defaults_to_memory() {
        with_env_vars(&[("CACHE_BACKEND", None)], || {
            let cache = super::from_env().expect("should create memory cache");
            // verify it works by doing a roundtrip
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                cache
                    .set("test", &"value", std::time::Duration::from_secs(10))
                    .await
                    .unwrap();
                let val: Option<String> = cache.get("test").await.unwrap();
                assert_eq!(val.as_deref(), Some("value"));
            });
        });
    }

    #[test]
    fn from_env_explicit_memory() {
        with_env_vars(
            &[
                ("CACHE_BACKEND", Some("memory")),
                ("CACHE_MAX_ENTRIES", Some("500")),
            ],
            || {
                let cache = super::from_env().expect("should create memory cache");
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    assert!(!cache.exists("nope").await.unwrap());
                });
            },
        );
    }

    #[test]
    fn from_env_unknown_backend_errors() {
        with_env_vars(&[("CACHE_BACKEND", Some("memcached"))], || {
            let result = super::from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("memcached"),
                "error should mention the backend: {err}"
            );
        });
    }

    #[cfg(not(feature = "redis"))]
    #[test]
    fn from_env_redis_without_feature_errors() {
        with_env_vars(&[("CACHE_BACKEND", Some("redis"))], || {
            let result = super::from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("redis"), "error should mention redis: {err}");
        });
    }
}
