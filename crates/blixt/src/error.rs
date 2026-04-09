use axum::http::StatusCode;
use axum::http::header::RETRY_AFTER;
use axum::response::{IntoResponse, Response};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Rate limited")]
    RateLimited { retry_after_secs: Option<u64> },

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
