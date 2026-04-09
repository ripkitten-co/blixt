#[cfg(all(feature = "postgres", feature = "sqlite"))]
compile_error!("Enable exactly one database backend: `postgres` or `sqlite`, not both.");

#[cfg(not(any(feature = "postgres", feature = "sqlite")))]
compile_error!("Enable at least one database backend feature: `postgres` or `sqlite`.");

pub mod app;
pub mod auth;
pub mod config;
pub mod context;
pub mod datastar;
pub mod db;
pub mod error;
pub mod jobs;
pub mod logging;
pub mod mailer;
pub mod middleware;
pub mod redact;

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
