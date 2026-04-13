+++
title = "Deployment"
weight = 18
description = "Docker generation, production builds, auto-migrations, and health checks."
+++

# Deployment

Blixt includes CLI tooling to generate production-ready Docker files and
an `App` builder option to run migrations on startup.

## Generating Docker files

```bash
blixt generate docker
```

This detects your project configuration and creates three files:

- **Dockerfile** — multi-stage build with `cargo-chef` for dependency caching
- **docker-compose.yml** — app, PostgreSQL 17, and optional Redis 7
- **.dockerignore** — excludes `target/`, `.git/`, `.env`, etc.

Redis is auto-detected if your `Cargo.toml` includes `features = ["redis"]`
for the blixt dependency, or if `.env.example` contains `REDIS_URL`.

### Build and run

```bash
docker compose up -d          # or: podman compose up -d
```

The compose file includes health checks on all services. The app container
waits for Postgres (and Redis if present) to be healthy before starting.

### What the Dockerfile does

```
Stage 1: chef     — installs cargo-chef on rust:1-slim
Stage 2: planner  — extracts dependency recipe (cargo chef prepare)
Stage 3: builder  — compiles deps from recipe, compiles app, builds Tailwind
Stage 4: runtime  — debian:bookworm-slim with binary, static files, migrations
```

The `cargo-chef` pattern caches dependency compilation between builds. Only
your application code recompiles when source changes.

## Auto-migrations on startup

For containerized deployments where the app manages its own schema, enable
auto-migration:

```rust
App::new(config)
    .db(pool)
    .run_migrations()
    .router(router)
    .serve()
    .await
```

When `.run_migrations()` is set, `serve()` runs all pending migrations
before binding the listener. This is opt-in — dev mode users typically
run `blixt db migrate` separately.

## Production build

```bash
blixt build
```

This compiles Tailwind CSS with `--minify` and runs `cargo build --release`.
The output binary is at `target/release/<your-app>`.

## Health check endpoints

Every Blixt app exposes two health endpoints (no middleware applied):

| Endpoint | Response | Use |
|----------|----------|-----|
| `/_ping` | `pong` (200) | Load balancer liveness probe |
| `/_health` | JSON with database status (200/503) | Readiness probe |

`/_health` checks database connectivity when a pool is configured via
`App::db(pool)`. Without a pool, it returns `"database not configured"`.

## Environment variables

Key variables for production:

| Variable | Default | Description |
|----------|---------|-------------|
| `BLIXT_ENV` | `development` | Set to `production` to skip `.env` loading and enable JSON logs |
| `HOST` | `127.0.0.1` | Bind address (`0.0.0.0` for containers) |
| `PORT` | `3000` | Listen port |
| `DATABASE_URL` | — | PostgreSQL connection string |
| `JWT_SECRET` | — | HMAC secret for token signing |

The generated `docker-compose.yml` sets these automatically.

## Security in production

When `BLIXT_ENV=production`:

- `.env` files are **not** loaded — use real environment variables
- Structured JSON logging is enabled
- A warning is emitted if `HOST` is bound to `0.0.0.0`
- Security headers (CSP, HSTS, X-Frame-Options, etc.) are applied on all responses
