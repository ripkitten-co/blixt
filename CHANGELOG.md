# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[unreleased]: https://github.com/ripkitten-co/blixt/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ripkitten-co/blixt/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ripkitten-co/blixt/releases/tag/v0.1.0
