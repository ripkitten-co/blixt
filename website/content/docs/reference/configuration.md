+++
title = "Configuration"
weight = 2
description = "Environment variables, .env file behavior, database feature flags, and the Config struct."
+++

# Configuration

Blixt loads all configuration from environment variables. The `Config` struct is the central entry point, created via `Config::from_env()`.

## Loading configuration

```rust
use blixt::prelude::*;

let config = Config::from_env()?;
```

`Config::from_env()` performs the following steps:

1. Checks whether `BLIXT_ENV` is already set to `production` in the real environment.
2. If not production, loads `.env` via [dotenvy](https://crates.io/crates/dotenvy). Missing `.env` files are silently ignored.
3. Reads all configuration values from the environment.

This ordering lets `.env` files set `BLIXT_ENV=development` for local work, while production deployments that set `BLIXT_ENV=production` in the process environment skip `.env` entirely.

## Environment variables

### Core variables

| Variable | Default | Description |
|----------|---------|-------------|
| `BLIXT_ENV` | `development` | Runtime environment: `development`, `production`, or `test` |
| `HOST` | `127.0.0.1` | Bind address for the HTTP server |
| `PORT` | `3000` | Listen port for the HTTP server |
| `DATABASE_URL` | *(none)* | Database connection string (PostgreSQL or SQLite) |
| `JWT_SECRET` | *(none)* | HMAC secret for JWT token signing |

### SMTP variables (for the mailer)

These are loaded separately by `MailerConfig::from_env()` and are only required when your application sends email.

| Variable | Description | Example |
|----------|-------------|---------|
| `SMTP_HOST` | SMTP server hostname | `smtp.mailgun.org` |
| `SMTP_PORT` | SMTP server port | `587` |
| `SMTP_USER` | SMTP authentication username | `postmaster@mg.example.com` |
| `SMTP_PASSWORD` | SMTP authentication password | `secret` |
| `FROM_NAME` | Display name for the From header | `My App` |
| `FROM_EMAIL` | Email address for the From header | `noreply@example.com` |

### Cache variables

Loaded by `cache::from_env()`. In-memory caching works with no configuration.

| Variable | Default | Description |
|----------|---------|-------------|
| `CACHE_BACKEND` | `memory` | `memory` or `redis` |
| `CACHE_MAX_ENTRIES` | `10000` | Max entries for in-memory backend |
| `REDIS_URL` | *(required for redis)* | Redis connection string |
| `REDIS_POOL_SIZE` | `8` | Redis connection pool size |

### Storage variables

Loaded by `storage::from_env()`. Local filesystem works with no configuration.

| Variable | Default | Description |
|----------|---------|-------------|
| `STORAGE_BACKEND` | `local` | `local` or `s3` |
| `STORAGE_LOCAL_DIR` | `./uploads` | Directory for local backend |
| `S3_BUCKET` | *(required for s3)* | S3 bucket name |
| `S3_REGION` | `us-east-1` | AWS region |
| `S3_ENDPOINT` | *(none)* | Custom endpoint (MinIO, R2) |
| `S3_ACCESS_KEY` | *(required for s3)* | AWS access key ID |
| `S3_SECRET_KEY` | *(required for s3)* | AWS secret access key |

## .env file

For local development, place a `.env` file in your project root:

```
BLIXT_ENV=development
HOST=127.0.0.1
PORT=3000
DATABASE_URL=postgres://localhost/myapp_dev
JWT_SECRET=dev-secret-change-in-production
```

The `.env` file is loaded by `dotenvy` unless `BLIXT_ENV=production` is already set in the process environment. This means:

- **Development/test**: `.env` is loaded, allowing you to keep configuration in a single file.
- **Production**: `.env` is skipped. Set environment variables through your deployment platform (Docker env, systemd, cloud provider secrets, etc.).

## Environments

The `Environment` enum has three variants:

| Value | Meaning |
|-------|---------|
| `Development` | Local development. Default when `BLIXT_ENV` is unset or unrecognized. |
| `Production` | Production deployment. Skips `.env` loading. Warns if `HOST=0.0.0.0`. |
| `Test` | Automated test runs. |

Check the environment at runtime:

```rust
if config.is_production() {
    // production-specific behavior
}
```

### Production warnings

When `BLIXT_ENV=production` and `HOST=0.0.0.0`, the framework logs a warning that the server is exposed on all network interfaces. This helps catch misconfiguration in production deployments.

## Secret handling

`DATABASE_URL` and `JWT_SECRET` are stored as `Option<SecretString>` from the [secrecy](https://crates.io/crates/secrecy) crate. The `Debug` implementation for `Config` prints `[REDACTED]` instead of the actual values.

Access secrets through dedicated accessor methods:

```rust
let db_url: Option<&str> = config.database_url();
let jwt: Option<&str> = config.jwt_secret();
```

These call `expose_secret()` internally and return `Option<&str>`. Never use `format!` or `to_string()` on `SecretString` directly.

## Database feature flags

The `blixt` crate requires exactly one database backend feature to be enabled at compile time:

| Feature | Backend |
|---------|---------|
| `postgres` | PostgreSQL via SQLx |
| `sqlite` | SQLite via SQLx |

Enabling both or neither causes a compile error:

```
// Cargo.toml in your project
[dependencies]
blixt = { version = "...", features = ["postgres"] }
```

The CLI's `blixt new` command sets the correct feature flag in the generated `Cargo.toml` based on the database selection.

The `DATABASE_URL` format determines the dialect:

- PostgreSQL: `postgres://user:pass@host/dbname` or `postgresql://...`
- SQLite: `sqlite://./data.db` or `sqlite:data.db?mode=rwc`

## The Config struct

```rust
pub struct Config {
    pub host: String,
    pub port: u16,
    pub blixt_env: Environment,
    pub database_url: Option<SecretString>,
    pub jwt_secret: Option<SecretString>,
}
```

Pass `Config` into `App::new()` to start the server:

```rust
use blixt::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;
    let config = Config::from_env()?;

    App::new(config)
        .router(routes())
        .static_dir("static")
        .serve()
        .await
}
```

### AppContext with mailer

When using authentication with email support, create an `AppContext` with an
optional mailer:

```rust
let config = Config::from_env()?;
let pool = blixt::db::create_pool(&config).await?;
let mailer = MailerConfig::from_env().ok().and_then(|c| Mailer::new(c).ok());
let ctx = AppContext::new(pool, config).with_mailer_opt(mailer);
```

`with_mailer_opt` accepts `Option<Mailer>` -- when SMTP variables are not set,
`MailerConfig::from_env()` returns `Err` and the mailer is `None`. Password
reset links are logged to stdout instead of emailed.
