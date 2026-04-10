//! Pagination support for database queries.
//!
//! Provides [`PaginationParams`] as an Axum extractor and [`Paginated<T>`]
//! for executing paginated queries with metadata (total count, page info).
//!
//! # Example
//!
//! ```rust,ignore
//! use blixt::prelude::*;
//!
//! async fn list(
//!     State(ctx): State<AppContext>,
//!     pagination: PaginationParams,
//! ) -> Result<impl IntoResponse> {
//!     let page = Paginated::<Todo>::query(
//!         "SELECT id, title FROM todos ORDER BY id DESC",
//!         &ctx.db,
//!         &pagination,
//!     ).await?;
//!     // page.items, page.total, page.has_next, etc.
//!     Ok(Html(TodoList { page }.render()?))
//! }
//! ```

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::db::DbPool;
use crate::error::Result;

/// Database backend type, resolved at compile time by feature flag.
#[cfg(feature = "postgres")]
type Db = sqlx::Postgres;
/// Database backend type, resolved at compile time by feature flag.
#[cfg(feature = "sqlite")]
type Db = sqlx::Sqlite;

/// Pagination parameters extracted from the query string.
///
/// Defaults: `page = 1`, `per_page = 25`.
/// Bounds: `page >= 1`, `per_page` clamped to `1..=100`.
///
/// Implements [`FromRequestParts`] so it can be used directly as an
/// Axum handler parameter. Missing or malformed query parameters
/// silently fall back to defaults.
#[derive(Debug, Clone, Deserialize)]
pub struct PaginationParams {
    page: Option<u32>,
    per_page: Option<u32>,
}

impl PaginationParams {
    /// Create pagination params with explicit values.
    pub fn new(page: u32, per_page: u32) -> Self {
        Self {
            page: Some(page),
            per_page: Some(per_page),
        }
    }

    /// Returns the current page number (1-indexed, minimum 1).
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    /// Returns the number of items per page (default 25, clamped to 1..=100).
    pub fn per_page(&self) -> u32 {
        self.per_page.unwrap_or(25).clamp(1, 100)
    }

    /// Returns the offset for the current page: `(page - 1) * per_page`.
    pub fn offset(&self) -> u32 {
        (self.page() - 1) * self.per_page()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for PaginationParams {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or_default();
        let page = extract_query_param(query, "page");
        let per_page = extract_query_param(query, "per_page");
        Ok(PaginationParams { page, per_page })
    }
}

/// Extracts a `u32` value from a URL query string by key name.
fn extract_query_param(query: &str, key: &str) -> Option<u32> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == key { v.parse().ok() } else { None }
    })
}

/// A page of query results with pagination metadata.
#[derive(Debug, Serialize)]
pub struct Paginated<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Current page number (1-indexed).
    pub page: u32,
    /// Items per page.
    pub per_page: u32,
    /// Total number of items across all pages.
    pub total: i64,
    /// Total number of pages.
    pub total_pages: u32,
    /// Whether there is a next page.
    pub has_next: bool,
    /// Whether there is a previous page.
    pub has_prev: bool,
}

impl<T> Paginated<T>
where
    T: for<'r> FromRow<'r, <Db as sqlx::Database>::Row> + Send + Unpin,
    T: Serialize,
{
    /// Execute a paginated query.
    ///
    /// `base_sql` is a SELECT query **without** `LIMIT`/`OFFSET` clauses --
    /// they are appended automatically. A `COUNT(*)` subquery is run to
    /// determine the total number of matching rows.
    ///
    /// The base SQL is a `&'static str` to encourage compile-time constant
    /// queries. The `LIMIT` and `OFFSET` values are always bound via
    /// parameterized placeholders.
    pub async fn query(
        base_sql: &'static str,
        pool: &DbPool,
        params: &PaginationParams,
    ) -> Result<Self> {
        let page = params.page();
        let per_page = params.per_page();
        let offset = params.offset();

        let total = count_total(base_sql, pool).await?;

        let items = fetch_page(base_sql, pool, per_page, offset).await?;

        let total_pages = if total == 0 {
            1
        } else {
            (total as u32).div_ceil(per_page)
        };

        Ok(Self {
            items,
            page,
            per_page,
            total,
            total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
        })
    }
}

/// Runs a COUNT(*) subquery wrapping the provided base SQL.
async fn count_total(base_sql: &str, pool: &DbPool) -> Result<i64> {
    let count_sql = format!("SELECT COUNT(*) FROM ({base_sql}) AS _blixt_count");
    let row: (i64,) = sqlx::query_as(&count_sql).fetch_one(pool).await?;
    Ok(row.0)
}

/// Fetches a single page of results with LIMIT/OFFSET appended.
#[cfg(feature = "sqlite")]
async fn fetch_page<T>(base_sql: &str, pool: &DbPool, per_page: u32, offset: u32) -> Result<Vec<T>>
where
    T: for<'r> FromRow<'r, <Db as sqlx::Database>::Row> + Send + Unpin,
{
    let page_sql = format!("{base_sql} LIMIT ? OFFSET ?");
    let items: Vec<T> = sqlx::query_as(&page_sql)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;
    Ok(items)
}

/// Fetches a single page of results with LIMIT/OFFSET appended.
#[cfg(feature = "postgres")]
async fn fetch_page<T>(base_sql: &str, pool: &DbPool, per_page: u32, offset: u32) -> Result<Vec<T>>
where
    T: for<'r> FromRow<'r, <Db as sqlx::Database>::Row> + Send + Unpin,
{
    let page_sql = format!("{base_sql} LIMIT $1 OFFSET $2");
    let items: Vec<T> = sqlx::query_as(&page_sql)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await?;
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_page_to_one() {
        let params = PaginationParams {
            page: None,
            per_page: None,
        };
        assert_eq!(params.page(), 1);
    }

    #[test]
    fn defaults_per_page_to_25() {
        let params = PaginationParams {
            page: None,
            per_page: None,
        };
        assert_eq!(params.per_page(), 25);
    }

    #[test]
    fn page_minimum_is_one() {
        let params = PaginationParams {
            page: Some(0),
            per_page: None,
        };
        assert_eq!(params.page(), 1);
    }

    #[test]
    fn per_page_clamps_to_minimum_one() {
        let params = PaginationParams {
            page: None,
            per_page: Some(0),
        };
        assert_eq!(params.per_page(), 1);
    }

    #[test]
    fn per_page_clamps_to_maximum_100() {
        let params = PaginationParams {
            page: None,
            per_page: Some(200),
        };
        assert_eq!(params.per_page(), 100);
    }

    #[test]
    fn offset_calculation() {
        let params = PaginationParams {
            page: Some(3),
            per_page: Some(10),
        };
        assert_eq!(params.offset(), 20);
    }

    #[test]
    fn offset_defaults_to_zero() {
        let params = PaginationParams {
            page: None,
            per_page: None,
        };
        assert_eq!(params.offset(), 0);
    }

    #[test]
    fn extract_query_param_finds_value() {
        assert_eq!(extract_query_param("page=3&per_page=10", "page"), Some(3));
        assert_eq!(
            extract_query_param("page=3&per_page=10", "per_page"),
            Some(10)
        );
    }

    #[test]
    fn extract_query_param_returns_none_for_missing() {
        assert_eq!(extract_query_param("page=3", "per_page"), None);
    }

    #[test]
    fn extract_query_param_returns_none_for_non_numeric() {
        assert_eq!(extract_query_param("page=abc", "page"), None);
    }

    #[test]
    fn extract_query_param_handles_empty_string() {
        assert_eq!(extract_query_param("", "page"), None);
    }

    #[test]
    fn paginated_metadata_single_page() {
        let page: Paginated<()> = Paginated {
            items: vec![(), (), ()],
            page: 1,
            per_page: 10,
            total: 3,
            total_pages: 1,
            has_next: false,
            has_prev: false,
        };
        assert!(!page.has_next);
        assert!(!page.has_prev);
        assert_eq!(page.total_pages, 1);
    }

    #[test]
    fn paginated_metadata_middle_page() {
        let page: Paginated<()> = Paginated {
            items: vec![],
            page: 2,
            per_page: 10,
            total: 30,
            total_pages: 3,
            has_next: true,
            has_prev: true,
        };
        assert!(page.has_next);
        assert!(page.has_prev);
        assert_eq!(page.total_pages, 3);
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod db_tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup_test_db() -> DbPool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect to in-memory SQLite");
        sqlx::query("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create table");
        for i in 1..=30 {
            sqlx::query("INSERT INTO items (name) VALUES (?)")
                .bind(format!("item-{i}"))
                .execute(&pool)
                .await
                .expect("insert row");
        }
        pool
    }

    #[derive(Debug, FromRow, serde::Serialize)]
    struct Item {
        id: i64,
        name: String,
    }

    #[tokio::test]
    async fn paginated_query_first_page() {
        let pool = setup_test_db().await;
        let params = PaginationParams {
            page: Some(1),
            per_page: Some(10),
        };
        let result =
            Paginated::<Item>::query("SELECT id, name FROM items ORDER BY id", &pool, &params)
                .await
                .expect("query first page");

        assert_eq!(result.items.len(), 10);
        assert_eq!(result.total, 30);
        assert_eq!(result.total_pages, 3);
        assert_eq!(result.page, 1);
        assert!(result.has_next);
        assert!(!result.has_prev);
    }

    #[tokio::test]
    async fn paginated_query_last_page() {
        let pool = setup_test_db().await;
        let params = PaginationParams {
            page: Some(3),
            per_page: Some(10),
        };
        let result =
            Paginated::<Item>::query("SELECT id, name FROM items ORDER BY id", &pool, &params)
                .await
                .expect("query last page");

        assert_eq!(result.items.len(), 10);
        assert_eq!(result.total, 30);
        assert!(!result.has_next);
        assert!(result.has_prev);
    }

    #[tokio::test]
    async fn paginated_query_partial_last_page() {
        let pool = setup_test_db().await;
        let params = PaginationParams {
            page: Some(4),
            per_page: Some(8),
        };
        let result =
            Paginated::<Item>::query("SELECT id, name FROM items ORDER BY id", &pool, &params)
                .await
                .expect("query partial last page");

        assert_eq!(result.items.len(), 6);
        assert_eq!(result.total, 30);
        assert_eq!(result.total_pages, 4);
        assert!(!result.has_next);
        assert!(result.has_prev);
    }

    #[tokio::test]
    async fn paginated_query_beyond_last_page() {
        let pool = setup_test_db().await;
        let params = PaginationParams {
            page: Some(100),
            per_page: Some(10),
        };
        let result =
            Paginated::<Item>::query("SELECT id, name FROM items ORDER BY id", &pool, &params)
                .await
                .expect("query beyond last page");

        assert_eq!(result.items.len(), 0);
        assert_eq!(result.total, 30);
        assert!(!result.has_next);
        assert!(result.has_prev);
    }

    #[tokio::test]
    async fn paginated_empty_table() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect to in-memory SQLite");
        sqlx::query("CREATE TABLE empty (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("create table");

        let params = PaginationParams {
            page: Some(1),
            per_page: Some(10),
        };

        #[derive(Debug, FromRow, serde::Serialize)]
        struct Row {
            id: i64,
        }

        let result = Paginated::<Row>::query("SELECT id FROM empty", &pool, &params)
            .await
            .expect("query empty table");

        assert_eq!(result.items.len(), 0);
        assert_eq!(result.total, 0);
        assert_eq!(result.total_pages, 1);
        assert!(!result.has_next);
        assert!(!result.has_prev);
    }
}
