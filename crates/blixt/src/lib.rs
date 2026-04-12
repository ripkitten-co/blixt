//! Blixt is a Rust web framework built on Axum, Askama, and SQLx.
//!
//! It provides compile-time safety for templates and SQL queries, SSE-based
//! interactivity via Datastar, and automatic Tailwind CSS integration — with
//! zero JavaScript build steps.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use blixt::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     init_tracing()?;
//!     let config = Config::from_env()?;
//!     App::new(config).router(Router::new()).serve().await
//! }
//! ```

#![warn(missing_docs)]

#[cfg(all(feature = "postgres", feature = "sqlite", not(docsrs)))]
compile_error!("Enable exactly one database backend: `postgres` or `sqlite`, not both.");

#[cfg(not(any(feature = "postgres", feature = "sqlite")))]
compile_error!("Enable at least one database backend feature: `postgres` or `sqlite`.");

/// Application builder and server.
pub mod app;
/// Authentication: JWT, password hashing, extractors.
pub mod auth;
/// Caching with in-memory and optional Redis backends.
pub mod cache;
/// Environment-aware configuration.
pub mod config;
/// Shared application state.
pub mod context;
/// Datastar SSE responses and signals.
pub mod datastar;
/// Database connection pools.
pub mod db;
/// Error types and HTTP status mapping.
pub mod error;
/// Flash messages and redirects.
pub mod flash;
/// Form extraction with CSRF validation.
pub mod form;
/// Health check endpoints.
pub mod health;
/// Background job runner.
pub mod jobs;
/// Structured logging setup.
pub mod logging;
/// SMTP email sending.
pub mod mailer;
/// HTTP middleware (CSRF, rate limiting, security headers).
pub mod middleware;
/// Pagination support for database queries.
pub mod paginate;
/// Secret-safe wrapper that redacts values in logs.
pub mod redact;
/// Input validation.
pub mod validate;

#[cfg(test)]
pub(crate) mod test_helpers;

/// Renders an Askama template and wraps it in an HTML response.
///
/// Converts template rendering errors to `Error::Internal` and returns
/// `Ok(Html(html))`, suitable for handlers returning `Result<impl IntoResponse>`.
///
/// ```rust,ignore
/// pub async fn index() -> Result<impl IntoResponse> {
///     render!(HomePage { title: "Welcome" })
/// }
/// ```
#[macro_export]
macro_rules! render {
    ($template:expr) => {{
        let __blixt_html = $template
            .render()
            .map_err(|e| $crate::error::Error::Internal(e.to_string()))?;
        Ok($crate::prelude::Html(__blixt_html))
    }};
}

/// Common re-exports for Blixt applications.
pub mod prelude {
    pub use crate::app::App;
    pub use crate::auth::cookie as auth_cookie;
    pub use crate::auth::{AuthUser, Claims, OptionalAuth};
    pub use crate::cache::Cache;
    pub use crate::config::{Config, Environment};
    pub use crate::context::AppContext;
    pub use crate::datastar::{
        DatastarSignals, Signals, SseFragment, SseResponse, SseSignals, SseStream,
    };
    pub use crate::db::DbPool;
    pub use crate::db::builder::{Delete, Insert, Order, Select, Update, Value};
    pub use crate::error::{Error, Result};
    pub use crate::flash::{Flash, Redirect};
    pub use crate::form::{CsrfToken, Form};
    pub use crate::jobs::{Job, JobRunner, job_fn};
    pub use crate::logging::init_tracing;
    pub use crate::mailer::{Mailer, MailerConfig};
    pub use crate::paginate::{Paginated, PaginationParams};
    pub use crate::redact::Redact;
    pub use crate::validate::Validator;
    pub use askama::Template;
    pub use axum::{
        Router,
        extract::{Path, Query, State},
        response::{Html, IntoResponse},
        routing::{delete, get, post, put},
    };
    pub use serde::{Deserialize, Serialize};
    pub use sqlx::FromRow;
    pub use tracing::{debug, error, info, warn};

    pub use crate::{query, query_as, query_scalar, render};
}

#[cfg(test)]
mod render_tests {
    use crate::prelude::*;

    #[derive(askama::Template)]
    #[template(source = "<h1>{{ title }}</h1>", ext = "html")]
    struct TestTemplate {
        title: String,
    }

    #[test]
    fn render_macro_produces_html_response() {
        fn handler() -> Result<Html<String>> {
            render!(TestTemplate {
                title: "Hello".to_string()
            })
        }
        let result = handler();
        assert!(result.is_ok());
        assert!(result.unwrap().0.contains("<h1>Hello</h1>"));
    }

    #[derive(askama::Template)]
    #[template(source = "{{ value }}", ext = "html")]
    struct EscapeTemplate {
        value: String,
    }

    #[test]
    fn render_macro_escapes_html() {
        fn handler() -> Result<Html<String>> {
            render!(EscapeTemplate {
                value: "<script>alert('xss')</script>".to_string()
            })
        }
        let html = handler().unwrap().0;
        assert!(!html.contains("<script>"));
    }
}
