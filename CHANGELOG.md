# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

[unreleased]: https://github.com/ripkitten-co/blixt/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ripkitten-co/blixt/releases/tag/v0.1.0
