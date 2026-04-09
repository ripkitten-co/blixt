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
/// Background job runner.
pub mod jobs;
/// Structured logging setup.
pub mod logging;
/// SMTP email sending.
pub mod mailer;
/// HTTP middleware (CSRF, rate limiting, security headers).
pub mod middleware;
/// Secret-safe wrapper that redacts values in logs.
pub mod redact;

#[cfg(test)]
pub(crate) mod test_helpers;

/// Common re-exports for Blixt applications.
pub mod prelude {
    pub use crate::app::App;
    pub use crate::auth::{AuthUser, Claims, OptionalAuth};
    pub use crate::config::{Config, Environment};
    pub use crate::context::AppContext;
    pub use crate::db::DbPool;
    pub use crate::error::{Error, Result};
    pub use crate::jobs::{Job, JobRunner, job_fn};
    pub use crate::logging::init_tracing;
    pub use crate::mailer::{Mailer, MailerConfig};
    pub use crate::redact::Redact;
    pub use askama::Template;
    pub use axum::{
        Router,
        extract::{Path, Query, State},
        response::IntoResponse,
        routing::{delete, get, post, put},
    };
    pub use serde::{Deserialize, Serialize};
    pub use sqlx::FromRow;
    pub use tracing::{debug, error, info, warn};
}
