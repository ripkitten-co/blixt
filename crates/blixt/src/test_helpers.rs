#![cfg(test)]
#![allow(unused)]

use crate::config::{Config, Environment};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Sets environment variables for the duration of the closure, then
/// restores previous values. Serialized via `ENV_LOCK` to avoid races.
///
/// # Safety
/// Callers must ensure no other threads read env vars concurrently.
/// We enforce this via ENV_LOCK.
pub fn with_env_vars<F, R>(vars: &[(&str, Option<&str>)], f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = ENV_LOCK.lock().expect("env lock poisoned");

    let mut previous: Vec<(&str, Option<String>)> = Vec::new();
    for &(key, value) in vars {
        previous.push((key, std::env::var(key).ok()));
        // SAFETY: protected by ENV_LOCK mutex; tests run serially
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    let result = f();

    for (key, prev) in previous {
        // SAFETY: protected by ENV_LOCK mutex; restoring original values
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    result
}

pub fn test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        blixt_env: Environment::Test,
        database_url: None,
        jwt_secret: None,
    }
}

pub fn test_db_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").ok()
}

macro_rules! require_db {
    () => {
        if $crate::test_helpers::test_db_url().is_none() {
            eprintln!("skipping: TEST_DATABASE_URL not set");
            return;
        }
    };
}

pub(crate) use require_db;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;

/// HTTP test client for integration testing Blixt handlers.
///
/// Wraps an Axum Router and provides a fluent API for sending
/// requests and asserting on responses without boilerplate.
pub struct TestClient {
    app: Router,
}

impl TestClient {
    /// Create a new test client wrapping the given router.
    pub fn new(app: Router) -> Self {
        Self { app }
    }

    /// Send a GET request to the given URI.
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Method::GET, uri, Body::empty()).await
    }

    /// Start building a POST request.
    pub fn post(&self, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.app.clone(), Method::POST, uri)
    }

    /// Start building a PUT request.
    pub fn put(&self, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.app.clone(), Method::PUT, uri)
    }

    /// Start building a DELETE request.
    pub fn delete(&self, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.app.clone(), Method::DELETE, uri)
    }

    async fn request(&self, method: Method, uri: &str, body: Body) -> TestResponse {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(body)
            .expect("failed to build request");
        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("request failed");
        TestResponse { inner: response }
    }
}

/// Builder for test requests with headers, JSON bodies, or
/// Datastar signals.
pub struct TestRequestBuilder {
    app: Router,
    method: Method,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl TestRequestBuilder {
    fn new(app: Router, method: Method, uri: &str) -> Self {
        Self {
            app,
            method,
            uri: uri.to_owned(),
            headers: vec![],
            body: None,
        }
    }

    /// Set a JSON request body.
    pub fn json<T: serde::Serialize>(mut self, data: &T) -> Self {
        self.body = Some(serde_json::to_string(data).expect("failed to serialize JSON"));
        self.headers
            .push(("content-type".to_owned(), "application/json".to_owned()));
        self
    }

    /// Set Datastar signals as the request body (JSON format).
    pub fn signals(self, signals: &serde_json::Value) -> Self {
        self.json(signals)
    }

    /// Add a request header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Send the request and return the response.
    pub async fn send(self) -> TestResponse {
        let body = match self.body {
            Some(text) => Body::from(text),
            None => Body::empty(),
        };
        let mut builder = Request::builder().method(self.method).uri(&self.uri);
        for (name, value) in &self.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder.body(body).expect("failed to build request");
        let response = self.app.oneshot(request).await.expect("request failed");
        TestResponse { inner: response }
    }
}

/// Response wrapper with assertion helpers for test code.
pub struct TestResponse {
    inner: Response,
}

impl TestResponse {
    /// Get the response status code.
    pub fn status(&self) -> StatusCode {
        self.inner.status()
    }

    /// Get a response header value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.inner.headers().get(name)?.to_str().ok()
    }

    /// Read the response body as a string.
    pub async fn text(self) -> String {
        let bytes = axum::body::to_bytes(self.inner.into_body(), 1024 * 1024)
            .await
            .expect("failed to read body");
        String::from_utf8(bytes.to_vec()).expect("body is not valid UTF-8")
    }

    /// Deserialize the response body as JSON.
    pub async fn json<T: serde::de::DeserializeOwned>(self) -> T {
        let text = self.text().await;
        serde_json::from_str(&text).expect("failed to parse JSON response")
    }

    /// Assert the status code matches. Returns self for chaining.
    pub fn assert_status(self, expected: StatusCode) -> Self {
        assert_eq!(
            self.inner.status(),
            expected,
            "expected status {expected}, got {}",
            self.inner.status()
        );
        self
    }

    /// Assert a header exists with the given value. Returns self
    /// for chaining.
    pub fn assert_header(self, name: &str, expected: &str) -> Self {
        let actual = self
            .header(name)
            .unwrap_or_else(|| panic!("expected header '{name}' to exist"));
        assert_eq!(
            actual, expected,
            "header '{name}': expected '{expected}', got '{actual}'"
        );
        self
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;
    use axum::Json;
    use axum::routing::get;

    fn test_app() -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route(
                "/json",
                get(|| async { Json(serde_json::json!({"status": "ok"})) }),
            )
            .route(
                "/echo",
                axum::routing::post(|body: String| async move { body }),
            )
    }

    #[tokio::test]
    async fn get_returns_status_and_body() {
        let client = TestClient::new(test_app());
        let response = client.get("/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await, "ok");
    }

    #[tokio::test]
    async fn get_json_response() {
        let client = TestClient::new(test_app());
        let response = client.get("/json").await;
        let data: serde_json::Value = response.json().await;
        assert_eq!(data["status"], "ok");
    }

    #[tokio::test]
    async fn post_with_json_body() {
        let client = TestClient::new(test_app());
        let response = client
            .post("/echo")
            .json(&serde_json::json!({"hello": "world"}))
            .send()
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await;
        assert!(body.contains("hello"));
    }

    #[tokio::test]
    async fn assert_status_passes_on_match() {
        let client = TestClient::new(test_app());
        let response = client.get("/health").await;
        response.assert_status(StatusCode::OK);
    }

    #[tokio::test]
    async fn not_found_for_unknown_route() {
        let client = TestClient::new(test_app());
        let response = client.get("/nonexistent").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn custom_header_sent() {
        let app = Router::new().route(
            "/check",
            axum::routing::post(|request: axum::http::Request<Body>| async move {
                request
                    .headers()
                    .get("x-custom")
                    .map(|header_value| header_value.to_str().unwrap_or("").to_owned())
                    .unwrap_or_default()
            }),
        );
        let client = TestClient::new(app);
        let response = client
            .post("/check")
            .header("x-custom", "test-value")
            .send()
            .await;
        assert_eq!(response.text().await, "test-value");
    }
}
