use axum::extract::{FromRequest, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

use crate::middleware::csrf::{CSRF_COOKIE_NAME, constant_time_eq, extract_cookie_value};

const MAX_BODY_SIZE: usize = 64 * 1024;

/// Axum extractor for URL-encoded form data with automatic CSRF validation.
///
/// On state-changing methods (POST, PUT, PATCH, DELETE), validates the CSRF
/// token from either the `x-csrf-token` header or a `_csrf` hidden form field
/// against the `blixt_csrf` cookie.
pub struct Form<T>(pub T);

impl<T> Form<T> {
    /// Unwrap the inner form data.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Extractor for the current CSRF token, for embedding in HTML forms.
///
/// Reads the token from the `blixt_csrf` cookie set by the CSRF middleware.
/// Use this in your template struct to render a hidden `_csrf` input field.
pub struct CsrfToken(String);

impl CsrfToken {
    /// The token value to embed in a form.
    pub fn value(&self) -> &str {
        &self.0
    }
}

impl<S, T> FromRequest<S> for Form<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request(
        request: Request<axum::body::Body>,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let (parts, body) = request.into_parts();

        let body_bytes = axum::body::to_bytes(body, MAX_BODY_SIZE)
            .await
            .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;

        if !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
            let body_str = std::str::from_utf8(&body_bytes)
                .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;

            let cookie_token = extract_cookie_value(&parts.headers, CSRF_COOKIE_NAME);
            let header_token = parts
                .headers
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                .map(String::from);

            let form_token = serde_urlencoded::from_str::<Vec<(String, String)>>(body_str)
                .ok()
                .and_then(|pairs| {
                    pairs
                        .into_iter()
                        .find(|(k, _)| k == "_csrf")
                        .map(|(_, v)| v)
                });

            let submitted_token = header_token.or(form_token);

            match (submitted_token, cookie_token) {
                (Some(ref s), Some(ref c)) if constant_time_eq(s, c) => {}
                _ => return Err(StatusCode::FORBIDDEN.into_response()),
            }
        }

        let data: T = serde_urlencoded::from_bytes(&body_bytes).map_err(|_| {
            crate::error::Error::BadRequest("Invalid form data".to_string()).into_response()
        })?;
        Ok(Form(data))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CsrfToken {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let token = extract_cookie_value(&parts.headers, CSRF_COOKIE_NAME)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(CsrfToken(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::post;
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Deserialize)]
    struct TestForm {
        name: String,
        #[allow(dead_code)]
        age: u32,
    }

    fn test_router() -> Router {
        async fn handler(form: Form<TestForm>) -> String {
            let data = form.into_inner();
            format!("{}:{}", data.name, data.age)
        }
        Router::new().route("/submit", post(handler))
    }

    #[tokio::test]
    async fn form_extracts_urlencoded_body() {
        let app = test_router();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", "blixt_csrf=token123")
            .header("x-csrf-token", "token123")
            .body(Body::from("name=Alice&age=30"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn form_rejects_missing_csrf() {
        let app = test_router();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("name=Alice&age=30"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn form_accepts_csrf_from_hidden_field() {
        let app = test_router();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", "blixt_csrf=token123")
            .body(Body::from("name=Alice&age=30&_csrf=token123"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn form_rejects_mismatched_csrf() {
        let app = test_router();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", "blixt_csrf=token123")
            .header("x-csrf-token", "wrong_token")
            .body(Body::from("name=Alice&age=30"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn form_rejects_invalid_body() {
        let app = test_router();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", "blixt_csrf=token123")
            .header("x-csrf-token", "token123")
            .body(Body::from("invalid"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
