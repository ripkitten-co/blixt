+++
title = "Caching"
weight = 14
description = "In-memory and Redis caching with typed get/set and TTL support."
+++

# Caching

Blixt provides a `Cache` with in-memory (default) and Redis backends. Values
are serialized via JSON — you work with typed Rust values, not raw bytes.

## Using the cache

The cache is always available on `AppContext`:

```rust
use blixt::prelude::*;
use std::time::Duration;

async fn get_user(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse> {
    let cache_key = format!("user:{id}");

    // try cache first
    if let Some(user) = ctx.cache.get::<User>(&cache_key).await? {
        return render!(UserPage { user });
    }

    // miss — fetch from DB and cache for 5 minutes
    let user = User::find_by_id(&ctx.db, id).await?;
    ctx.cache.set(&cache_key, &user, Duration::from_secs(300)).await?;

    render!(UserPage { user })
}
```

## API

```rust
ctx.cache.get::<T>(key).await?        // Option<T> — None on miss
ctx.cache.set(key, &value, ttl).await? // store with TTL
ctx.cache.delete(key).await?           // remove
ctx.cache.exists(key).await?           // bool
```

Any type implementing `Serialize` + `DeserializeOwned` works.

## Backends

### In-memory (default)

Uses [moka](https://crates.io/crates/moka) for concurrent LRU eviction with
per-entry TTL. No configuration needed — works out of the box.

### Redis

Enable the `redis` cargo feature and set environment variables:

```toml
# Cargo.toml
blixt = { version = "...", features = ["redis"] }
```

```bash
# .env
CACHE_BACKEND=redis
REDIS_URL=redis://localhost:6379
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `CACHE_BACKEND` | `memory` | `memory` or `redis` |
| `CACHE_MAX_ENTRIES` | `10000` | Max entries for in-memory backend |
| `REDIS_URL` | *(required for redis)* | Redis connection URL |
| `REDIS_POOL_SIZE` | `8` | Redis connection pool size |

## Custom backend

Override the default cache on `AppContext`:

```rust
let cache = blixt::cache::from_env()?;
let ctx = AppContext::new(pool, config).with_cache(cache);
```

Implement `CacheBackend` for a custom backend:

```rust
use blixt::cache::CacheBackend;

struct MyCache;

#[async_trait::async_trait]
impl CacheBackend for MyCache {
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>> { ... }
    async fn set_bytes(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> { ... }
    async fn delete(&self, key: &str) -> Result<()> { ... }
    async fn exists(&self, key: &str) -> Result<bool> { ... }
}
```
