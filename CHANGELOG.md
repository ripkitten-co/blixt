# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-04-11

### Added

- `render!()` macro for ergonomic template responses — eliminates `.render().map_err(...)` + `Html(...)` boilerplate
- `blixt::db::migrate()` one-line migration runner — replaces raw `sqlx::migrate!()` in user code
- `Signals` builder for type-safe SSE signal payloads — `Signals::clear(&["title", "body"])` replaces `json!({})`
- `Flash` messages with cookie-based read-once semantics — `Flash::success("Created")`, `Flash::error("Failed")`, `Flash::info("Note")`
- `Redirect` response with optional flash attachment — `Redirect::to("/posts").with_flash(Flash::success("Done"))`
- `Form<T>` extractor with automatic CSRF validation — checks `x-csrf-token` header or `_csrf` hidden form field
- `CsrfToken` extractor for embedding CSRF tokens in HTML form templates
- Type-safe query builder: `Select`, `Insert`, `Update`, `Delete` with dialect-aware SQL generation (`$1` for Postgres, `?` for SQLite)
- `Select` supports `where_eq/gt/lt/gte/lte/ne`, `order_by`, `limit`, `offset`, `fetch_all/one/optional`
- `Insert` with two-phase API: `.returning::<T>()` transitions to `InsertReturning<T>` for compile-time safety
- `Update` with `set_timestamp()` for portable `CURRENT_TIMESTAMP` injection
- `Delete` logs warning when executed without WHERE conditions
- Scaffold CLI: `blixt generate scaffold post title:string body:text published:bool` with field arguments
- Scaffold generates full CRUD: 6 handlers (index, show, create, update, destroy, page_handler) using Paginated, DatastarSignals, Validator, SseResponse, SseFragment
- Scaffold generates 5 templates: index page, create form, paginated list, item row, show/edit page — all with Datastar signal bindings
- Scaffold generates `models/mod.rs` and `controllers/mod.rs` with `pub mod` entries
- DB-dialect-aware migration generation: Postgres (`BIGSERIAL`, `TIMESTAMPTZ`) vs SQLite (`INTEGER PRIMARY KEY AUTOINCREMENT`, `DATETIME`)
- Pluralization helper for generated table names and routes (`category` → `categories`, `status` → `statuses`)
- SQL reserved word validation in field name parser (rejects `select`, `order`, `table`, etc.)
- Datastar SHA-256 checksum verification on download (supply chain security)
- CSRF `Secure` flag on cookie in production environments
- Trusted proxy configuration for rate limiter `X-Forwarded-For` handling
- Rate limiter stale entry eviction to prevent memory exhaustion under IP rotation attacks
- Type-safe `Validator`: `str_field()` returns `StrFieldValidator`, `i64_field()` returns `I64FieldValidator` — mismatched rules are compile errors
- Feature-flagged database backends: `postgres` (default) and `sqlite` with `--db` flag on `blixt new`
- `blixt db migrate/rollback/status` works for both Postgres and SQLite via `AnyPool`

### Changed

- Scaffold controller uses `render!()` macro instead of manual template rendering
- Scaffold controller uses `Signals::clear()` instead of `serde_json::json!()`
- Generated models use the query builder instead of raw SQL with manual placeholder management
- Todo example updated to use `render!()`, `blixt::db::migrate()`, `Signals::clear()`, and query builder
- Hello example updated to use `render!()` with proper error handling
- Workspace dependencies centralized: `secrecy`, `uuid`, `chrono`, `sha2`, `dirs` moved to root `Cargo.toml`
- Shared `with_env_vars` test helper extracted from duplicated code in config and mailer modules
- `.env` loaded before reading `BLIXT_ENV` for correct configuration precedence

### Fixed

- Generated SQL uses dialect-aware placeholders (`$1` for Postgres, `?` for SQLite) — previously all models used `?` which broke Postgres
- `blixt db` commands work with SQLite projects (was hardcoded to `PgPool`)
- Scaffold route hints use plural paths matching template URLs (was singular)
- Generated `update` handler wrapped in `Ok()` (was missing, causing compile error)
- Delete on last page falls back to previous page instead of showing empty state

## [0.2.0] - 2026-04-10

### Added

- `SseResponse` multi-event builder for composing DOM patches and signal updates in a single SSE response
- `query!`, `query_as!`, `query_scalar!` macros that only accept string literals — prevents SQL injection at compile time
- Input validation module with fluent `Validator` builder (`not_empty`, `min_length`, `max_length`, `pattern`, `range`, `positive`) and `Error::Validation` (HTTP 422)
- `Paginated<T>` with `PaginationParams` extractor — paginated queries with total count, prev/next metadata, and `per_page` clamped to 1..100
- `TestClient`, `TestRequestBuilder`, `TestResponse` for fluent integration testing of Axum handlers
- `PaginationParams::new()` constructor for programmatic pagination
- `Config` now derives `Clone`
- All Datastar types (`SseFragment`, `SseSignals`, `SseResponse`, `SseStream`, `DatastarSignals`) and `Html` added to prelude
- Built-in regex patterns for email, alphanumeric, and slug validation
- `blixt generate model` now scaffolds `find_by_id`, `find_all`, `delete` methods using safe query macros
- `blixt generate scaffold` produces a DB-backed controller with index, show, and destroy handlers
- Feature-flagged database backends: `postgres` (default) and `sqlite`
- `DbPool` type alias that resolves to the active backend's pool type
- `--db` flag and interactive database selection for `blixt new`
- Shared test helpers (`test_config`, `test_context`, `require_db!` macro)
- E2e tests for CLI generate commands
- Unit tests for dev server file watcher and build command
- GitHub Actions CI pipeline (lint, unit tests, integration tests, e2e tests)
- Weekly `cargo audit` security scan
- Clippy SARIF upload for GitHub code scanning
- Dependabot configuration for Cargo and GitHub Actions
- Issue templates (bug report, feature request) and PR template
- CONTRIBUTING.md, CODE_OF_CONDUCT.md, CHANGELOG.md

### Fixed

- SSE multiline HTML collapsing now joins with spaces instead of concatenating, preventing broken HTML attributes
- Todo example: validation errors return 422 instead of silently accepting empty titles
- Todo example: delete and toggle stay on the current page instead of resetting to page 1

### Changed

- Todo example refactored to use framework helpers: `SseResponse`, `query!`, `Validator`, `Paginated<T>`
- Todo example dependencies trimmed from 11 to 6

## [0.1.0] - 2026-04-09

### Added

- Core framework library (`blixt` crate)
  - Application builder with middleware composition and static file serving
  - Environment-aware configuration with `.env` support and secret redaction
  - PostgreSQL connection pool with connectivity verification
  - JWT authentication with HS256 algorithm pinning
  - Argon2id password hashing
  - Deny-by-default auth extractors (`AuthUser`, `OptionalAuth`)
  - CSRF protection (double-submit cookie + origin validation)
  - Token-bucket rate limiting per IP
  - Security headers middleware (CSP, HSTS, X-Frame-Options, etc.)
  - Request ID middleware
  - Datastar SSE integration (`SseFragment`, `SseSignals`, `SseStream`)
  - CSS hot-reload in debug builds
  - Background job runner with bounded concurrency
  - SMTP mailer with Askama template support
  - Structured logging via tracing
  - Unified error type with HTTP status mapping
- CLI tooling (`blixt-cli` crate)
  - `blixt new` — project scaffolding with Datastar and Tailwind
  - `blixt dev` — development server with file watching and auto-restart
  - `blixt build` — production build with Tailwind minification
  - `blixt generate controller` — controller with Askama templates
  - `blixt generate model` — model with SQLx derives and migration
  - `blixt generate scaffold` — full CRUD (controller + model + list fragment)
  - `blixt db migrate/rollback/status` — database migration management
  - Automatic Tailwind v4 download with checksum verification

[unreleased]: https://github.com/ripkitten-co/blixt/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ripkitten-co/blixt/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ripkitten-co/blixt/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ripkitten-co/blixt/releases/tag/v0.1.0
