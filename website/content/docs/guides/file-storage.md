+++
title = "File Storage"
weight = 15
description = "Store and retrieve files with local filesystem or S3-compatible backends."
+++

# File Storage

Blixt provides a `Storage` abstraction for file operations. Local filesystem
is the default — switch to S3 (or MinIO, Cloudflare R2) in production without
changing application code.

## Using storage

Storage is always available on `AppContext`:

```rust
use blixt::prelude::*;

async fn save_export(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let data = generate_csv().await?;
    ctx.storage.put("exports/report.csv", data).await?;
    Ok("saved")
}

async fn download(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let data = ctx.storage.get("exports/report.csv").await?;
    Ok(data)
}
```

## API

```rust
ctx.storage.put(path, bytes).await?       // store file, returns WriteResult
ctx.storage.get(path).await?              // retrieve bytes
ctx.storage.delete(path).await?           // remove file
ctx.storage.exists(path).await?           // check existence
ctx.storage.put_stream(path, chunks).await? // write from chunks, returns WriteResult
ctx.storage.reader(path).await?           // streaming reader
ctx.storage.presigned_url(path, ttl).await? // signed URL (S3 only)
```

Nested paths are created automatically: `"avatars/user-42/photo.jpg"`.

### WriteResult

`put()` and `put_stream()` return a `WriteResult` with metadata from the
storage backend:

```rust
let result = ctx.storage.put("uploads/photo.jpg", data).await?;

if let Some(etag) = result.etag() {
    // use for cache headers (ETag)
}

let size = result.content_length(); // bytes written
```

If you don't need the metadata, the result is silently dropped:

```rust
ctx.storage.put("uploads/photo.jpg", data).await?; // WriteResult ignored
```

## Backends

### Local filesystem (default)

Files are stored in `./uploads` by default. Change with `STORAGE_LOCAL_DIR`.

### S3-compatible

Enable the `s3` cargo feature and set environment variables:

```toml
# Cargo.toml
blixt = { version = "...", features = ["s3"] }
```

```bash
# .env
STORAGE_BACKEND=s3
S3_BUCKET=my-app-uploads
S3_REGION=eu-north-1
S3_ACCESS_KEY=AKIA...
S3_SECRET_KEY=...
# S3_ENDPOINT=https://minio.local:9000  # for MinIO/R2
```

### Presigned URLs

On S3, generate time-limited download URLs:

```rust
use std::time::Duration;

let url = ctx.storage
    .presigned_url("exports/report.csv", Duration::from_secs(3600))
    .await?;
// https://bucket.s3.amazonaws.com/exports/report.csv?X-Amz-...
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `STORAGE_BACKEND` | `local` | `local` or `s3` |
| `STORAGE_LOCAL_DIR` | `./uploads` | Directory for local backend |
| `S3_BUCKET` | *(required)* | S3 bucket name |
| `S3_REGION` | `us-east-1` | AWS region |
| `S3_ENDPOINT` | *(none)* | Custom endpoint (MinIO, R2) |
| `S3_ACCESS_KEY` | *(required)* | AWS access key ID |
| `S3_SECRET_KEY` | *(required)* | AWS secret access key |

## Custom backend

Override the default on `AppContext`:

```rust
let storage = blixt::storage::from_env()?;
let ctx = AppContext::new(pool, config).with_storage(storage);
```
