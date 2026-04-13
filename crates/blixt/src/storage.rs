//! File storage abstraction with local filesystem and optional S3 backends.
//!
//! Uses [opendal](https://docs.rs/opendal) as the storage engine. The local
//! backend is always available; S3 requires the `s3` cargo feature.

use std::time::Duration;

use opendal::Operator;

use crate::error::{Error, Result};

/// File storage backed by opendal.
///
/// Provides bytes and streaming APIs for file operations, plus presigned
/// URL generation. Always present in `AppContext` — defaults to local
/// filesystem at `./uploads`.
#[derive(Clone)]
pub struct Storage {
    op: Operator,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage").finish_non_exhaustive()
    }
}

impl Storage {
    /// Wraps an opendal `Operator`.
    pub fn new(op: Operator) -> Self {
        Self { op }
    }

    /// Creates a local filesystem storage rooted at the given directory.
    pub fn local(root: &str) -> Result<Self> {
        let builder = opendal::services::Fs::default().root(root);
        let op = Operator::new(builder)
            .map_err(|e| Error::Internal(format!("local storage init: {e}")))?
            .finish();
        tracing::info!(root = %root, "Local file storage initialized");
        Ok(Self { op })
    }

    /// Creates an S3-compatible storage backend.
    #[cfg(feature = "s3")]
    pub fn s3(
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
        endpoint: Option<&str>,
    ) -> Result<Self> {
        let mut builder = opendal::services::S3::default()
            .bucket(bucket)
            .region(region)
            .access_key_id(access_key)
            .secret_access_key(secret_key);
        if let Some(ep) = endpoint {
            builder = builder.endpoint(ep);
        }
        let op = Operator::new(builder)
            .map_err(|e| Error::Internal(format!("S3 storage init: {e}")))?
            .finish();
        tracing::info!(bucket = %bucket, region = %region, "S3 storage initialized");
        Ok(Self { op })
    }

    // --- Bytes API ---

    /// Stores file contents at the given path.
    pub async fn put(&self, path: &str, data: Vec<u8>) -> Result<()> {
        self.op
            .write(path, data)
            .await
            .map_err(|e| map_opendal_error("put", e))
    }

    /// Retrieves file contents by path.
    pub async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.op
            .read(path)
            .await
            .map(|buf| buf.to_vec())
            .map_err(|e| map_opendal_error("get", e))
    }

    /// Deletes a file. No error if the file doesn't exist.
    pub async fn delete(&self, path: &str) -> Result<()> {
        self.op
            .delete(path)
            .await
            .map_err(|e| map_opendal_error("delete", e))
    }

    /// Returns `true` if the file exists.
    pub async fn exists(&self, path: &str) -> Result<bool> {
        self.op
            .exists(path)
            .await
            .map_err(|e| map_opendal_error("exists", e))
    }

    // --- Streaming API ---

    /// Writes a file from chunks. Suitable for large files.
    pub async fn put_stream(&self, path: &str, chunks: &[bytes::Bytes]) -> Result<()> {
        let mut writer = self
            .op
            .writer(path)
            .await
            .map_err(|e| map_opendal_error("put_stream", e))?;

        for chunk in chunks {
            writer
                .write(chunk.clone())
                .await
                .map_err(|e| map_opendal_error("put_stream write", e))?;
        }

        writer
            .close()
            .await
            .map_err(|e| map_opendal_error("put_stream close", e))?;
        Ok(())
    }

    /// Reads a file as a byte vector via the streaming reader. Suitable for
    /// large files where you want to process chunks incrementally.
    pub async fn reader(&self, path: &str) -> Result<opendal::Reader> {
        self.op
            .reader(path)
            .await
            .map_err(|e| map_opendal_error("reader", e))
    }

    // --- URLs ---

    /// Generates a presigned URL valid for the given duration.
    ///
    /// For S3 backends, uses native presigning. For local storage, returns
    /// an error — serve local files through your app's routes instead.
    pub async fn presigned_url(&self, path: &str, expires: Duration) -> Result<String> {
        let url = self
            .op
            .presign_read(path, expires)
            .await
            .map_err(|e| map_opendal_error("presigned_url", e))?;
        Ok(url.uri().to_string())
    }
}

fn map_opendal_error(op: &str, err: opendal::Error) -> Error {
    match err.kind() {
        opendal::ErrorKind::NotFound => Error::NotFound,
        _ => Error::Internal(format!("storage {op}: {err}")),
    }
}

/// Creates a [`Storage`] from environment variables.
///
/// | Variable | Default | Description |
/// |----------|---------|-------------|
/// | `STORAGE_BACKEND` | `local` | `local` or `s3` |
/// | `STORAGE_LOCAL_DIR` | `./uploads` | Directory for local backend |
/// | `S3_BUCKET` | *(required)* | S3 bucket name |
/// | `S3_REGION` | `us-east-1` | AWS region |
/// | `S3_ENDPOINT` | *(none)* | Custom endpoint (MinIO, R2) |
/// | `S3_ACCESS_KEY` | *(required)* | AWS access key |
/// | `S3_SECRET_KEY` | *(required)* | AWS secret key |
pub fn from_env() -> Result<Storage> {
    let backend = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".into());
    match backend.as_str() {
        "local" => {
            let dir = std::env::var("STORAGE_LOCAL_DIR").unwrap_or_else(|_| "./uploads".into());
            Storage::local(&dir)
        }
        #[cfg(feature = "s3")]
        "s3" => {
            let bucket = require_env("S3_BUCKET")?;
            let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into());
            let access_key = require_env("S3_ACCESS_KEY")?;
            let secret_key = require_env("S3_SECRET_KEY")?;
            let endpoint = std::env::var("S3_ENDPOINT").ok();
            Storage::s3(
                &bucket,
                &region,
                &access_key,
                &secret_key,
                endpoint.as_deref(),
            )
        }
        #[cfg(not(feature = "s3"))]
        "s3" => Err(Error::Internal(
            "STORAGE_BACKEND=s3 requires the `s3` cargo feature".into(),
        )),
        other => Err(Error::Internal(format!(
            "Unknown STORAGE_BACKEND: '{other}'. Use 'local' or 's3'."
        ))),
    }
}

#[cfg(feature = "s3")]
fn require_env(key: &str) -> Result<String> {
    std::env::var(key)
        .map_err(|_| Error::Internal(format!("Missing required environment variable: {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::with_env_vars;

    fn local_storage(dir: &std::path::Path) -> Storage {
        Storage::local(dir.to_str().unwrap()).expect("local storage")
    }

    #[tokio::test]
    async fn put_and_get_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        storage.put("test.txt", b"hello".to_vec()).await.unwrap();
        let data = storage.get("test.txt").await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn get_missing_file_returns_not_found() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        let result = storage.get("missing.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn exists_reflects_presence() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        assert!(!storage.exists("file.txt").await.unwrap());
        storage.put("file.txt", b"data".to_vec()).await.unwrap();
        assert!(storage.exists("file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        storage.put("file.txt", b"data".to_vec()).await.unwrap();
        storage.delete("file.txt").await.unwrap();
        assert!(!storage.exists("file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        let result = storage.delete("nope.txt").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn overwrite_replaces_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        storage.put("file.txt", b"first".to_vec()).await.unwrap();
        storage.put("file.txt", b"second".to_vec()).await.unwrap();
        let data = storage.get("file.txt").await.unwrap();
        assert_eq!(data, b"second");
    }

    #[tokio::test]
    async fn nested_paths_work() {
        let tmp = tempfile::TempDir::new().unwrap();
        let storage = local_storage(tmp.path());

        storage
            .put("avatars/user-42.jpg", b"image".to_vec())
            .await
            .unwrap();
        let data = storage.get("avatars/user-42.jpg").await.unwrap();
        assert_eq!(data, b"image");
    }

    #[test]
    fn from_env_defaults_to_local() {
        let tmp = tempfile::TempDir::new().unwrap();
        with_env_vars(
            &[
                ("STORAGE_BACKEND", None),
                ("STORAGE_LOCAL_DIR", Some(tmp.path().to_str().unwrap())),
            ],
            || {
                let storage = from_env().expect("should create local storage");
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    storage.put("test.bin", b"ok".to_vec()).await.unwrap();
                    assert!(storage.exists("test.bin").await.unwrap());
                });
            },
        );
    }

    #[test]
    fn from_env_unknown_backend_errors() {
        with_env_vars(&[("STORAGE_BACKEND", Some("gcs"))], || {
            let result = from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("gcs"), "error should mention backend: {err}");
        });
    }

    #[cfg(not(feature = "s3"))]
    #[test]
    fn from_env_s3_without_feature_errors() {
        with_env_vars(&[("STORAGE_BACKEND", Some("s3"))], || {
            let result = from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("s3"), "error should mention s3: {err}");
        });
    }
}
