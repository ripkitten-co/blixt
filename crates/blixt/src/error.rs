use std::collections::HashMap;

use axum::http::StatusCode;
use axum::http::header::RETRY_AFTER;
use axum::response::{IntoResponse, Response};

/// Per-field validation error messages.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationErrors {
    /// Map of field names to lists of error messages.
    pub errors: HashMap<String, Vec<String>>,
}

impl ValidationErrors {
    /// Creates an empty validation error set.
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
        }
    }

    /// Adds an error message for a field.
    pub fn add(&mut self, field: &str, message: String) {
        self.errors
            .entry(field.to_owned())
            .or_default()
            .push(message);
    }

    /// Returns true if there are no errors.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Default for ValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Validation failed: ")?;
        let mut fields: Vec<&str> = self.errors.keys().map(String::as_str).collect();
        fields.sort_unstable();
        write!(f, "{}", fields.join(", "))
    }
}

/// Blixt result type using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Unified error type with HTTP status code mapping.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Database error.
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// 404 Not Found.
    #[error("Not found")]
    NotFound,

    /// 401 Unauthorized.
    #[error("Unauthorized")]
    Unauthorized,

    /// 403 Forbidden.
    #[error("Forbidden")]
    Forbidden,

    /// 400 Bad Request with a user-visible message.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// 429 Too Many Requests.
    #[error("Rate limited")]
    RateLimited {
        /// Seconds until the client should retry, sent as `Retry-After`.
        retry_after_secs: Option<u64>,
    },

    /// 422 Unprocessable Entity with per-field validation errors.
    #[error("{0}")]
    Validation(ValidationErrors),

    /// Catch-all for internal failures (logged, never exposed to clients).
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()).into_response(),
            Self::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()).into_response()
            }
            Self::Forbidden => (StatusCode::FORBIDDEN, "Forbidden".to_string()).into_response(),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            Self::Validation(ref errors) => {
                let body = serde_json::to_string(errors)
                    .unwrap_or_else(|_| r#"{"errors":{}}"#.to_string());
                (StatusCode::UNPROCESSABLE_ENTITY, body).into_response()
            }
            Self::RateLimited { retry_after_secs } => build_rate_limited_response(retry_after_secs),
            Self::Io(ref err) => {
                tracing::error!(error = %err, "IO error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
                    .into_response()
            }
            Self::Database(ref err) => {
                tracing::error!(error = %err, "Database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
                    .into_response()
            }
            Self::Internal(ref msg) => {
                tracing::error!(error = %msg, "Internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
                    .into_response()
            }
        }
    }
}

fn build_rate_limited_response(retry_after_secs: Option<u64>) -> Response {
    let body = "Too many requests".to_string();
    let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
    if let Some(secs) = retry_after_secs {
        let value = axum::http::HeaderValue::from(secs);
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn response_body(resp: Response) -> String {
        let bytes = to_bytes(resp.into_body(), 1024 * 16)
            .await
            .expect("read body");
        String::from_utf8(bytes.to_vec()).expect("valid utf-8")
    }

    #[tokio::test]
    async fn internal_error_does_not_leak_details() {
        let err = Error::Internal("secret db path /var/lib/pg".into());
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response_body(resp).await;
        assert_eq!(body, "Internal server error");
        assert!(!body.contains("secret"));
    }

    #[tokio::test]
    async fn database_error_does_not_leak_sql() {
        let sql_err = sqlx::Error::Configuration("host=secret-db.internal password=hunter2".into());
        let err = Error::Database(sql_err);
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response_body(resp).await;
        assert_eq!(body, "Internal server error");
        assert!(!body.contains("secret"));
        assert!(!body.contains("hunter2"));
    }

    #[tokio::test]
    async fn rate_limited_includes_retry_after_header() {
        let err = Error::RateLimited {
            retry_after_secs: Some(60),
        };
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            resp.headers()
                .get(RETRY_AFTER)
                .expect("Retry-After header")
                .to_str()
                .expect("valid str"),
            "60"
        );

        let body = response_body(resp).await;
        assert_eq!(body, "Too many requests");
    }

    #[tokio::test]
    async fn rate_limited_without_retry_after() {
        let err = Error::RateLimited {
            retry_after_secs: None,
        };
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp.headers().get(RETRY_AFTER).is_none());
    }

    #[tokio::test]
    async fn forbidden_returns_403() {
        let err = Error::Forbidden;
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let body = response_body(resp).await;
        assert_eq!(body, "Forbidden");
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let err = Error::NotFound;
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let body = response_body(resp).await;
        assert_eq!(body, "Not found");
    }

    #[tokio::test]
    async fn unauthorized_returns_401() {
        let err = Error::Unauthorized;
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let body = response_body(resp).await;
        assert_eq!(body, "Unauthorized");
    }

    #[tokio::test]
    async fn bad_request_shows_user_message() {
        let err = Error::BadRequest("email is required".into());
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = response_body(resp).await;
        assert_eq!(body, "email is required");
    }

    #[tokio::test]
    async fn validation_error_returns_422_with_json() {
        let mut errors = ValidationErrors::new();
        errors.add("title", "must not be empty".to_string());
        errors.add("title", "must be at most 255 characters".to_string());
        errors.add("priority", "must be between 1 and 5".to_string());
        let err = Error::Validation(errors);
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response_body(resp).await;
        assert!(body.contains("title"));
        assert!(body.contains("must not be empty"));
        assert!(body.contains("priority"));
    }

    #[tokio::test]
    async fn validation_error_does_not_leak_field_values() {
        let mut errors = ValidationErrors::new();
        errors.add("password", "must be at least 8 characters".to_string());
        let err = Error::Validation(errors);
        let resp = err.into_response();
        let body = response_body(resp).await;
        // Error message references the field name and rule, not the actual value
        assert!(!body.contains("hunter2"));
        assert!(body.contains("password"));
        assert!(body.contains("must be at least 8 characters"));
    }

    #[tokio::test]
    async fn io_error_does_not_leak_details() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "/etc/shadow: permission denied",
        );
        let err = Error::Io(io_err);
        let resp = err.into_response();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response_body(resp).await;
        assert_eq!(body, "Internal server error");
        assert!(!body.contains("shadow"));
    }
}
