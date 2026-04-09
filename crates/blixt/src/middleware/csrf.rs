use axum::http::header::{COOKIE, ORIGIN, SET_COOKIE};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

static X_CSRF_TOKEN: HeaderName = HeaderName::from_static("x-csrf-token");
static CSRF_COOKIE_NAME: &str = "blixt_csrf";

/// Axum middleware implementing CSRF protection via the double-submit cookie
/// pattern with Origin header validation as defense-in-depth.
///
/// On safe methods (GET, HEAD, OPTIONS): sets a `blixt_csrf` cookie and
/// `x-csrf-token` response header with a matching random token.
///
/// On state-changing methods (POST, PUT, DELETE, PATCH): validates that the
/// `x-csrf-token` request header matches the `blixt_csrf` cookie, and that
/// the `Origin` header (if present) matches the `Host` header.
///
/// Use with `axum::middleware::from_fn(csrf_protection)`.
pub async fn csrf_protection(request: Request<axum::body::Body>, next: Next) -> Response {
    let method = request.method().clone();

    if is_safe_method(&method) {
        return handle_safe_request(request, next).await;
    }

    if !is_origin_valid(request.headers()) {
        return forbidden_response();
    }

    if !is_csrf_token_valid(request.headers()) {
        return forbidden_response();
    }

    next.run(request).await
}

/// Returns true for HTTP methods that do not modify state.
fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Handles safe requests by generating a CSRF token and attaching it to the
/// response as both a cookie and a header.
async fn handle_safe_request(request: Request<axum::body::Body>, next: Next) -> Response {
    let token = generate_token();
    let mut response = next.run(request).await;
    attach_csrf_token(&mut response, &token);
    response
}

/// Returns true if the Origin header is absent or matches the Host header.
fn is_origin_valid(headers: &HeaderMap) -> bool {
    let origin = match headers.get(ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(origin) => origin,
        None => return true,
    };

    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if origin_matches_host(origin, host) {
        true
    } else {
        tracing::warn!(origin = %origin, host = %host, "CSRF origin mismatch");
        false
    }
}

/// Checks whether the origin URL's host portion matches the expected host.
fn origin_matches_host(origin: &str, host: &str) -> bool {
    origin
        .split("//")
        .nth(1)
        .map(|after_scheme| after_scheme.split('/').next().unwrap_or(after_scheme))
        .is_some_and(|origin_host| origin_host == host)
}

/// Returns true if the CSRF token header matches the CSRF cookie.
fn is_csrf_token_valid(headers: &HeaderMap) -> bool {
    let header_token = headers
        .get(X_CSRF_TOKEN.clone())
        .and_then(|v| v.to_str().ok());

    let cookie_token = extract_cookie_value(headers, CSRF_COOKIE_NAME);

    match (header_token, cookie_token) {
        (Some(h), Some(c)) if constant_time_eq(h, &c) => true,
        _ => {
            tracing::warn!("CSRF token validation failed");
            false
        }
    }
}

/// Extracts a named cookie value from the Cookie header.
fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(str::trim)
                .find(|pair| {
                    pair.starts_with(name) && pair.as_bytes().get(name.len()) == Some(&b'=')
                })
                .map(|pair| pair[name.len() + 1..].to_string())
        })
}

/// Performs constant-time comparison of two strings.
///
/// While CSRF tokens are not long-term secrets, constant-time comparison is
/// a defense-in-depth measure against timing side-channels.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Generates a random CSRF token using UUID v7.
///
/// UUID v7 incorporates a timestamp prefix but the remaining bytes are random,
/// providing sufficient entropy for CSRF tokens.
fn generate_token() -> String {
    Uuid::now_v7().simple().to_string()
}

/// Attaches the CSRF token as a response cookie and header.
fn attach_csrf_token(response: &mut Response, token: &str) {
    let cookie = format!("{CSRF_COOKIE_NAME}={token}; Path=/; SameSite=Strict");
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(SET_COOKIE, value);
    }
    if let Ok(value) = HeaderValue::from_str(token) {
        response.headers_mut().insert(X_CSRF_TOKEN.clone(), value);
    }
}

/// Returns a 403 Forbidden response.
fn forbidden_response() -> Response {
    (StatusCode::FORBIDDEN, "Forbidden").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::{get, post};
    use tower::ServiceExt;

    fn test_router() -> Router {
        Router::new()
            .route("/form", get(|| async { "form" }))
            .route("/submit", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn(csrf_protection))
    }

    #[tokio::test]
    async fn get_response_includes_csrf_cookie_and_header() {
        let app = test_router();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/form")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("request failed");

        assert_eq!(response.status(), StatusCode::OK);

        let csrf_header = response
            .headers()
            .get("x-csrf-token")
            .expect("x-csrf-token header missing");
        let token = csrf_header.to_str().expect("invalid utf-8");
        assert_eq!(token.len(), 32, "token should be 32-char hex UUID");

        let cookie = response
            .headers()
            .get(SET_COOKIE)
            .expect("Set-Cookie header missing")
            .to_str()
            .expect("invalid utf-8");
        assert!(cookie.contains("blixt_csrf="));
        assert!(cookie.contains(token));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Path=/"));
    }

    #[tokio::test]
    async fn post_without_csrf_token_returns_403() {
        let app = test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("request failed");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_matching_token_passes() {
        let app = test_router();
        let token = "abcdef01234567890abcdef012345678";

        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("x-csrf-token", token)
            .header(COOKIE, format!("blixt_csrf={token}"))
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("request failed");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_with_mismatched_token_returns_403() {
        let app = test_router();

        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("x-csrf-token", "token-a")
            .header(COOKIE, "blixt_csrf=token-b")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("request failed");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_mismatched_origin_returns_403() {
        let app = test_router();
        let token = "abcdef01234567890abcdef012345678";

        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("x-csrf-token", token)
            .header(COOKIE, format!("blixt_csrf={token}"))
            .header(ORIGIN, "https://evil.com")
            .header(axum::http::header::HOST, "myapp.com")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("request failed");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_matching_origin_passes() {
        let app = test_router();
        let token = "abcdef01234567890abcdef012345678";

        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("x-csrf-token", token)
            .header(COOKIE, format!("blixt_csrf={token}"))
            .header(ORIGIN, "https://myapp.com")
            .header(axum::http::header::HOST, "myapp.com")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("request failed");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("abc123", "abc123"));
    }

    #[test]
    fn constant_time_eq_rejects_different_strings() {
        assert!(!constant_time_eq("abc123", "xyz789"));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths() {
        assert!(!constant_time_eq("short", "longer-string"));
    }

    #[test]
    fn origin_matches_host_with_scheme() {
        assert!(origin_matches_host("https://example.com", "example.com"));
        assert!(origin_matches_host(
            "http://localhost:3000",
            "localhost:3000"
        ));
    }

    #[test]
    fn origin_does_not_match_different_host() {
        assert!(!origin_matches_host("https://evil.com", "example.com"));
    }

    #[test]
    fn extract_cookie_finds_named_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("session=abc; blixt_csrf=mytoken; other=xyz"),
        );
        assert_eq!(
            extract_cookie_value(&headers, "blixt_csrf"),
            Some("mytoken".to_string())
        );
    }

    #[test]
    fn extract_cookie_returns_none_when_missing() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("session=abc"));
        assert_eq!(extract_cookie_value(&headers, "blixt_csrf"), None);
    }
}
