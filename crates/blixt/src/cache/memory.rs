use std::time::{Duration, Instant};

use moka::future::Cache as MokaCache;

/// In-memory cache backed by [`moka`] with per-entry TTL and LRU eviction.
///
/// Thread-safe and lock-free for concurrent reads. Suitable for
/// single-instance deployments and local development.
pub struct MemoryCache {
    inner: MokaCache<String, CacheEntry>,
}

#[derive(Clone)]
struct CacheEntry {
    data: Vec<u8>,
    expires_at: Instant,
}

impl MemoryCache {
    /// Creates a new in-memory cache with the given maximum entry count.
    ///
    /// Entries beyond this limit are evicted using an LRU policy.
    pub fn new(max_entries: u64) -> Self {
        let inner = MokaCache::builder().max_capacity(max_entries).build();
        Self { inner }
    }
}

#[async_trait::async_trait]
impl super::CacheBackend for MemoryCache {
    async fn get_bytes(&self, key: &str) -> crate::error::Result<Option<Vec<u8>>> {
        let Some(entry) = self.inner.get(key).await else {
            return Ok(None);
        };
        if Instant::now() > entry.expires_at {
            self.inner.remove(key).await;
            return Ok(None);
        }
        Ok(Some(entry.data))
    }

    async fn set_bytes(&self, key: &str, value: &[u8], ttl: Duration) -> crate::error::Result<()> {
        let entry = CacheEntry {
            data: value.to_vec(),
            expires_at: Instant::now() + ttl,
        };
        self.inner.insert(key.to_owned(), entry).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> crate::error::Result<()> {
        self.inner.remove(key).await;
        Ok(())
    }

    async fn exists(&self, key: &str) -> crate::error::Result<bool> {
        match self.inner.get(key).await {
            Some(entry) => {
                if Instant::now() > entry.expires_at {
                    self.inner.remove(key).await;
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Cache, CacheBackend};
    use super::*;
    use std::sync::Arc;

    fn test_cache() -> Cache {
        Cache::new(Arc::new(MemoryCache::new(100)))
    }

    #[tokio::test]
    async fn set_and_get_roundtrip() {
        let cache = test_cache();
        cache
            .set("key", &"hello", Duration::from_secs(60))
            .await
            .unwrap();
        let val: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(val.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let cache = test_cache();
        let val: Option<String> = cache.get("missing").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let cache = test_cache();
        cache
            .set("key", &42i64, Duration::from_secs(60))
            .await
            .unwrap();
        cache.delete("key").await.unwrap();
        let val: Option<i64> = cache.get("key").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn exists_reflects_presence() {
        let cache = test_cache();
        assert!(!cache.exists("key").await.unwrap());
        cache
            .set("key", &true, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(cache.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn expired_entry_returns_none() {
        let backend = MemoryCache::new(100);
        // insert with zero TTL
        backend
            .set_bytes("key", b"\"expired\"", Duration::from_millis(0))
            .await
            .unwrap();
        // entry should be treated as expired
        std::thread::sleep(Duration::from_millis(5));
        let val = backend.get_bytes("key").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn expired_entry_not_exists() {
        let backend = MemoryCache::new(100);
        backend
            .set_bytes("key", b"1", Duration::from_millis(0))
            .await
            .unwrap();
        std::thread::sleep(Duration::from_millis(5));
        assert!(!backend.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn complex_type_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct User {
            id: i64,
            name: String,
        }

        let cache = test_cache();
        let user = User {
            id: 42,
            name: "Alice".into(),
        };
        cache
            .set("user:42", &user, Duration::from_secs(60))
            .await
            .unwrap();
        let got: Option<User> = cache.get("user:42").await.unwrap();
        assert_eq!(got, Some(user));
    }

    #[tokio::test]
    async fn overwrite_replaces_value() {
        let cache = test_cache();
        cache
            .set("key", &"first", Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key", &"second", Duration::from_secs(60))
            .await
            .unwrap();
        let val: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(val.as_deref(), Some("second"));
    }
}
