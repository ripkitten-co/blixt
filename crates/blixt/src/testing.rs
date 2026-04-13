//! Test utilities for Blixt applications.
//!
//! Enable via `[dev-dependencies]`:
//! ```toml
//! blixt = { version = "0.5", features = ["test-helpers"] }
//! ```
//!
//! Provides [`TestClient`] for HTTP integration tests, [`TestPool`] for
//! database tests with automatic migration, and re-exports the [`fake`]
//! crate for generating test data.

pub use fake;

use std::sync::Mutex;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;

use crate::config::{Config, Environment};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Sets environment variables for the duration of the closure, then
/// restores previous values. Serialized via mutex to avoid races.
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

/// Creates a [`Config`] suitable for testing: binds to localhost on port 0
/// (OS-assigned), test environment, no database or JWT secret.
pub fn test_config() -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        blixt_env: Environment::Test,
        database_url: None,
        jwt_secret: None,
    }
}

/// Returns the value of `TEST_DATABASE_URL` if set.
pub fn test_db_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").ok()
}

/// Skips the current test if `TEST_DATABASE_URL` is not set.
#[macro_export]
macro_rules! require_test_db {
    () => {
        if $crate::testing::test_db_url().is_none() {
            eprintln!("skipping: TEST_DATABASE_URL not set");
            return;
        }
    };
}

/// HTTP test client for integration testing Blixt handlers.
///
/// Wraps an Axum `Router` and provides a fluent API for sending
/// requests and asserting on responses.
///
/// ```rust,ignore
/// use blixt::testing::TestClient;
///
/// let client = TestClient::new(app_router());
/// let resp = client.get("/health").await;
/// resp.assert_status(StatusCode::OK);
/// ```
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

    /// Start building a PATCH request.
    pub fn patch(&self, uri: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.app.clone(), Method::PATCH, uri)
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

/// Builder for test requests with headers, JSON bodies, or Datastar signals.
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

    /// Assert a header exists with the given value. Returns self for chaining.
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

// -- TestPool: shared database pool with automatic migration --

#[cfg(feature = "postgres")]
mod pool {
    use std::ops::Deref;

    use tokio::sync::OnceCell;

    use crate::db::DbPool;

    static SHARED_POOL: OnceCell<DbPool> = OnceCell::const_new();

    /// A shared database pool for integration tests.
    ///
    /// On first call, connects to `TEST_DATABASE_URL` and runs all pending
    /// migrations. Subsequent calls reuse the same pool. Implements
    /// `Deref<Target = DbPool>` so it works directly with the query builder.
    ///
    /// ```rust,ignore
    /// use blixt::testing::TestPool;
    ///
    /// #[tokio::test]
    /// async fn creates_user() {
    ///     blixt::require_test_db!();
    ///     let pool = TestPool::new().await;
    ///     // use &*pool or &pool with query builder methods
    /// }
    /// ```
    pub struct TestPool {
        pool: DbPool,
    }

    impl TestPool {
        /// Connect to the test database and run migrations (once per process).
        ///
        /// Panics if `TEST_DATABASE_URL` is not set or the connection fails.
        pub async fn new() -> Self {
            let pool = SHARED_POOL
                .get_or_init(|| async {
                    let url = std::env::var("TEST_DATABASE_URL")
                        .expect("TEST_DATABASE_URL must be set for database tests");
                    let pool = sqlx::PgPool::connect(&url)
                        .await
                        .expect("failed to connect to test database");
                    crate::db::migrate(&pool)
                        .await
                        .expect("failed to run test migrations");
                    pool
                })
                .await
                .clone();
            Self { pool }
        }
    }

    impl Deref for TestPool {
        type Target = DbPool;
        fn deref(&self) -> &DbPool {
            &self.pool
        }
    }
}

#[cfg(feature = "postgres")]
pub use pool::TestPool;
