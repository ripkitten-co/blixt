//! Multipart file upload handling with validation and Storage integration.

use axum::extract::FromRequest;
use axum::extract::multipart::{Field, Multipart};
use axum::http::{Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::error::{Error, Result};
use crate::middleware::csrf::{CSRF_COOKIE_NAME, constant_time_eq, extract_cookie_value};
use crate::storage::Storage;

/// A file extracted from a multipart form field.
#[derive(Debug)]
pub struct UploadedFile {
    filename: Option<String>,
    content_type: Option<String>,
    data: Vec<u8>,
}

impl UploadedFile {
    /// Reads a multipart field into an `UploadedFile`.
    pub async fn from_field(field: Field<'_>) -> Result<Self> {
        let filename = field.file_name().map(|s| s.to_owned());
        let content_type = field.content_type().map(|s| s.to_owned());
        let data = field
            .bytes()
            .await
            .map_err(|e| Error::BadRequest(format!("Failed to read upload: {e}")))?
            .to_vec();
        Ok(Self {
            filename,
            content_type,
            data,
        })
    }

    /// The original filename from the client, if provided.
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    /// The MIME content type, if provided by the client.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// File size in bytes.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Raw file bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consume the file and return the raw bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// Start a validation chain.
    pub fn validate(self) -> FileValidator {
        FileValidator {
            file: self,
            error: None,
        }
    }

    /// Save directly to storage without validation.
    pub async fn save(self, storage: &Storage, path: &str) -> Result<()> {
        storage.put(path, self.data).await
    }
}

/// Builder for validating an uploaded file before saving.
pub struct FileValidator {
    file: UploadedFile,
    error: Option<String>,
}

impl FileValidator {
    /// Reject files larger than `max` bytes.
    pub fn max_size(mut self, max: usize) -> Self {
        if self.error.is_none() && self.file.size() > max {
            let mb = max / (1024 * 1024);
            self.error = Some(format!("File too large (max {}MB)", mb.max(1)));
        }
        self
    }

    /// Reject files whose content type is not in the allowed list.
    pub fn allowed_types(mut self, types: &[&str]) -> Self {
        if self.error.is_none() {
            let ct = self
                .file
                .content_type()
                .unwrap_or("application/octet-stream");
            if !types.contains(&ct) {
                self.error = Some(format!("File type {ct} not allowed"));
            }
        }
        self
    }

    /// Validate and save to storage. Returns `Error::BadRequest` on failure.
    pub async fn save(self, storage: &Storage, path: &str) -> Result<()> {
        if let Some(msg) = self.error {
            return Err(Error::BadRequest(msg));
        }
        storage.put(path, self.file.data).await
    }

    /// Validate without saving. Returns the file on success.
    pub fn finish(self) -> Result<UploadedFile> {
        if let Some(msg) = self.error {
            return Err(Error::BadRequest(msg));
        }
        Ok(self.file)
    }
}

// --- MultipartForm extractor ---

/// Axum extractor for multipart forms with automatic CSRF validation.
///
/// Wraps Axum's `Multipart` and validates the CSRF token from either
/// the `_csrf` form field (consumed before yielding other fields) or
/// the `x-csrf-token` header. Use this instead of raw `Multipart` for
/// forms that include file uploads.
///
/// Default max body size: 10MB. Override with `MultipartForm::max_size()`.
pub struct MultipartForm {
    inner: Multipart,
}

impl MultipartForm {
    /// Get the next field from the multipart stream.
    ///
    /// The `_csrf` field is consumed automatically during extraction
    /// and will not appear here.
    pub async fn next_field(&mut self) -> Result<Option<Field<'_>>> {
        self.inner
            .next_field()
            .await
            .map_err(|e| Error::BadRequest(format!("Multipart error: {e}")))
    }
}

impl<S> FromRequest<S> for MultipartForm
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(
        request: Request<axum::body::Body>,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let (parts, body) = request.into_parts();

        // Check CSRF on state-changing methods
        let needs_csrf = !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);

        let cookie_token = if needs_csrf {
            extract_cookie_value(&parts.headers, CSRF_COOKIE_NAME)
        } else {
            None
        };

        let header_token = parts
            .headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let request = Request::from_parts(parts, body);
        let mut multipart = Multipart::from_request(request, state)
            .await
            .map_err(|e| e.into_response())?;

        if needs_csrf {
            // Try header first
            if let (Some(header), Some(cookie)) = (&header_token, &cookie_token) {
                if constant_time_eq(header, cookie) {
                    return Ok(Self { inner: multipart });
                }
            }

            // Try reading _csrf from the first multipart field
            let field = multipart
                .next_field()
                .await
                .map_err(|_| StatusCode::BAD_REQUEST.into_response())?
                .ok_or_else(|| StatusCode::FORBIDDEN.into_response())?;

            if field.name() != Some("_csrf") {
                return Err(StatusCode::FORBIDDEN.into_response());
            }

            let token = field
                .text()
                .await
                .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;

            match &cookie_token {
                Some(cookie) if constant_time_eq(&token, cookie) => {}
                _ => return Err(StatusCode::FORBIDDEN.into_response()),
            }
        }

        Ok(Self { inner: multipart })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploaded_file_accessors() {
        let file = UploadedFile {
            filename: Some("avatar.png".into()),
            content_type: Some("image/png".into()),
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(file.filename(), Some("avatar.png"));
        assert_eq!(file.content_type(), Some("image/png"));
        assert_eq!(file.size(), 4);
        assert_eq!(file.data(), &[1, 2, 3, 4]);
    }

    #[test]
    fn uploaded_file_into_bytes() {
        let file = UploadedFile {
            filename: None,
            content_type: None,
            data: vec![10, 20, 30],
        };
        assert_eq!(file.into_bytes(), vec![10, 20, 30]);
    }

    #[test]
    fn validator_max_size_passes() {
        let file = UploadedFile {
            filename: None,
            content_type: None,
            data: vec![0; 100],
        };
        let result = file.validate().max_size(1024).finish();
        assert!(result.is_ok());
    }

    #[test]
    fn validator_max_size_rejects() {
        let file = UploadedFile {
            filename: None,
            content_type: None,
            data: vec![0; 2 * 1024 * 1024],
        };
        let result = file.validate().max_size(1024 * 1024).finish();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too large"), "error: {err}");
    }

    #[test]
    fn validator_allowed_types_passes() {
        let file = UploadedFile {
            filename: None,
            content_type: Some("image/png".into()),
            data: vec![0],
        };
        let result = file
            .validate()
            .allowed_types(&["image/png", "image/jpeg"])
            .finish();
        assert!(result.is_ok());
    }

    #[test]
    fn validator_allowed_types_rejects() {
        let file = UploadedFile {
            filename: None,
            content_type: Some("image/gif".into()),
            data: vec![0],
        };
        let result = file
            .validate()
            .allowed_types(&["image/png", "image/jpeg"])
            .finish();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("image/gif"), "error: {err}");
    }

    #[test]
    fn validator_chain_stops_at_first_error() {
        let file = UploadedFile {
            filename: None,
            content_type: Some("text/plain".into()),
            data: vec![0; 2 * 1024 * 1024],
        };
        let result = file
            .validate()
            .max_size(1024 * 1024)
            .allowed_types(&["image/png"])
            .finish();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // first error wins (size)
        assert!(err.contains("too large"), "error: {err}");
    }

    #[test]
    fn validator_no_content_type_treated_as_octet_stream() {
        let file = UploadedFile {
            filename: None,
            content_type: None,
            data: vec![0],
        };
        let result = file.validate().allowed_types(&["image/png"]).finish();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("application/octet-stream"), "error: {err}");
    }
}
