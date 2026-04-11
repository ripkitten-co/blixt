use axum::Router;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::Config;
use crate::db::DbPool;
use crate::error::Result;
use crate::middleware::request_id::request_id;
use crate::middleware::security_headers::security_headers;

/// Application builder that assembles routes and middleware.
pub struct App {
    config: Config,
    router: Router,
    static_dir: Option<String>,
    db: Option<DbPool>,
}

impl App {
    /// Creates a new application with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            router: Router::new(),
            static_dir: None,
            db: None,
        }
    }

    /// Register a database pool for health checks.
    ///
    /// When set, the `/_health` endpoint verifies database connectivity.
    pub fn db(mut self, pool: DbPool) -> Self {
        self.db = Some(pool);
        self
    }

    /// Sets the application router containing user-defined routes.
    pub fn router(mut self, router: Router) -> Self {
        self.router = router;
        self
    }

    /// Enables static file serving from the given directory at `/static/`.
    ///
    /// Dotfile requests (paths containing `/..` or segments starting with `.`)
    /// are blocked and return 404.
    pub fn static_dir(mut self, path: impl Into<String>) -> Self {
        self.static_dir = Some(path.into());
        self
    }

    /// Builds the final router with all middleware layers applied.
    fn build_router(self) -> Router {
        let router = attach_static_files(self.router, self.static_dir);

        // Health endpoints are merged before middleware layers so they
        // bypass tracing, CSRF, and rate limiting.
        let health_routes = axum::Router::new()
            .route("/_ping", axum::routing::get(crate::health::ping))
            .route("/_health", axum::routing::get(crate::health::check))
            .with_state(self.db);

        router
            .merge(health_routes)
            .layer(CompressionLayer::new())
            .layer(middleware::from_fn(security_headers))
            .layer(middleware::from_fn(request_id))
            .layer(TraceLayer::new_for_http())
    }

    /// Binds to the configured address and starts accepting connections.
    pub async fn serve(self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let router = self.build_router();
        let listener = TcpListener::bind(&addr).await?;
        info!("Blixt server running on http://{addr}");
        axum::serve(listener, router).await?;
        Ok(())
    }
}

/// Attaches static file serving if a directory was configured.
fn attach_static_files(router: Router, static_dir: Option<String>) -> Router {
    match static_dir {
        Some(dir) => {
            let cache_header = SetResponseHeaderLayer::overriding(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            let serve_dir = ServeDir::new(dir);
            let static_router = Router::new()
                .fallback_service(serve_dir)
                .layer(cache_header)
                .layer(middleware::from_fn(block_dotfiles));
            router.nest("/static", static_router)
        }
        None => router,
    }
}

/// Middleware that blocks requests to dotfiles and path traversal attempts.
///
/// Returns 404 for paths containing `/..` or path segments starting with `.`.
async fn block_dotfiles(
    request: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    if is_dotfile_path(request.uri().path()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}

/// Checks whether a URI path references a dotfile or traversal.
fn is_dotfile_path(path: &str) -> bool {
    path.contains("/..")
        || path
            .split('/')
            .any(|segment| segment.starts_with('.') && !segment.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::test_config;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt;

    fn build_test_app(static_dir: Option<&str>) -> Router {
        let routes = Router::new().route("/health", get(|| async { "ok" }));
        let mut app = App::new(test_config()).router(routes);
        if let Some(dir) = static_dir {
            app = app.static_dir(dir);
        }
        app.build_router()
    }

    #[tokio::test]
    async fn response_includes_all_security_headers() {
        let app = build_test_app(None);

        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("failed to send request");

        let headers = response.headers();
        assert!(headers.contains_key("content-security-policy"));
        assert!(headers.contains_key("strict-transport-security"));
        assert!(headers.contains_key("x-content-type-options"));
        assert!(headers.contains_key("x-frame-options"));
        assert!(headers.contains_key("referrer-policy"));
        assert!(headers.contains_key("permissions-policy"));
    }

    #[tokio::test]
    async fn dotfile_request_returns_404() {
        let app = build_test_app(Some("tests/fixtures/static"));

        let request = Request::builder()
            .uri("/static/.env")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("failed to send request");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn path_traversal_returns_404() {
        let app = build_test_app(Some("tests/fixtures/static"));

        let request = Request::builder()
            .uri("/static/../Cargo.toml")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("failed to send request");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn valid_static_file_returns_200() {
        let app = build_test_app(Some("tests/fixtures/static"));

        let request = Request::builder()
            .uri("/static/css/test.css")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("failed to send request");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn is_dotfile_path_detects_dotfiles() {
        assert!(is_dotfile_path("/.env"));
        assert!(is_dotfile_path("/css/.hidden"));
        assert!(is_dotfile_path("/../etc/passwd"));
        assert!(!is_dotfile_path("/css/style.css"));
        assert!(!is_dotfile_path("/js/app.js"));
    }
}
