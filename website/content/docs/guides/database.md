+++
title = "Database"
weight = 3
description = "Connect to Postgres or SQLite, run migrations, and query with compile-time safe macros."
+++

# Database

Blixt uses [SQLx](https://docs.rs/sqlx) for async database access with compile-time query checking. It supports PostgreSQL and SQLite through feature flags.

## Configuration

Set `DATABASE_URL` in your `.env` file:

```bash
# PostgreSQL
DATABASE_URL=postgres://user:password@localhost:5432/myapp

# SQLite
DATABASE_URL=sqlite:data.db
```

`Config::from_env()` reads this into a `SecretString` -- it is redacted in debug output and never accidentally logged.

## Creating a connection pool

Use `blixt::db::create_pool()` to establish a connection pool from your config:

```rust
let config = Config::from_env()?;
let pool = blixt::db::create_pool(&config).await?;
```

For PostgreSQL, the pool is configured with:
- 10 max connections
- 5 second acquire timeout
- 300 second idle timeout
- A connectivity check (`SELECT 1`) on startup

Returns `Error::Internal("DATABASE_URL not configured")` if the env var is missing.

## The DbPool type

`DbPool` is a type alias that resolves at compile time based on your feature flag:

```rust
// With feature = "postgres"
pub type DbPool = sqlx::PgPool;

// With feature = "sqlite"
pub type DbPool = sqlx::SqlitePool;
```

You must enable exactly one: `postgres` or `sqlite`. Enabling both (or neither) is a compile error.

Pass `DbPool` to handlers through `AppContext`:

```rust
let ctx = AppContext::new(pool, config.clone());
```

Handlers access it via `ctx.db`:

```rust
async fn index(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let todos = query_as!(Todo, "SELECT id, title FROM todos")
        .fetch_all(&ctx.db)
        .await?;
    render!(IndexPage { todos })
}
```

## Query macros

Blixt provides three query macros that wrap SQLx with a compile-time literal enforcement. They only accept string literals -- runtime-constructed strings are rejected at compile time, preventing SQL injection from string concatenation.

### query!

Executes a raw SQL query:

```rust
query!("UPDATE todos SET completed = NOT completed WHERE id = ?")
    .bind(id)
    .execute(&ctx.db)
    .await?;
```

### query_as!

Deserializes rows into a struct implementing `FromRow`:

```rust
let todos = query_as!(Todo, "SELECT id, title, completed FROM todos")
    .fetch_all(&ctx.db)
    .await?;
```

### query_scalar!

Returns a single scalar value:

```rust
let count: i64 = query_scalar!("SELECT COUNT(*) FROM todos")
    .fetch_one(&ctx.db)
    .await?;
```

### Compile-time safety

All three macros reject non-literal SQL at compile time:

```rust
// This compiles:
query!("SELECT * FROM users WHERE id = ?").bind(id);

// This does NOT compile:
let sql = format!("SELECT * FROM users WHERE name = '{}'", name);
query!(sql); // error: expected a string literal
```

Use `.bind()` for all dynamic values. Use the [query builder](@/docs/guides/query-builder.md) when you need to construct queries dynamically.

## Running migrations

### In application code

Call `blixt::db::migrate()` at startup to apply pending migrations from the `./migrations` directory:

```rust
let pool = blixt::db::create_pool(&config).await?;
blixt::db::migrate(&pool).await?;
```

### With the CLI

The CLI provides three migration commands:

```bash
blixt db migrate     # Apply all pending migrations
blixt db rollback    # Revert the most recently applied migration
blixt db status      # Show applied/pending status for each migration
```

The CLI reads `DATABASE_URL` from `.env` and connects via `sqlx::AnyPool`, so it works with both Postgres and SQLite.

### Migration files

Migrations live in `migrations/` as plain SQL files with a timestamp prefix:

```
migrations/
  20250315120000_create_todos.sql
  20250316090000_add_priority_to_todos.sql
```

## Generating models

The CLI generates a model struct and migration in one step:

```bash
blixt generate model todo title:string completed:bool priority:int
```

This creates:

1. **`src/models/todo.rs`** -- a struct with `FromRow`, `Serialize`, `Deserialize` derives and CRUD methods using the query builder
2. **`migrations/{timestamp}_create_todos.sql`** -- a CREATE TABLE migration

### Supported field types

| Argument | Rust type | Postgres type | SQLite type |
|----------|-----------|---------------|-------------|
| `string` / `text` | `String` | `TEXT NOT NULL` | `TEXT NOT NULL` |
| `int` / `integer` | `i64` | `BIGINT NOT NULL` | `INTEGER NOT NULL` |
| `bool` / `boolean` | `bool` | `BOOLEAN NOT NULL` | `BOOLEAN NOT NULL` |
| `float` | `f64` | `DOUBLE PRECISION NOT NULL` | `REAL NOT NULL` |

Every generated table includes `id`, `created_at`, and `updated_at` columns automatically.

### Generated model

The generated model includes five methods:

```rust
impl Todo {
    pub async fn find_by_id(pool: &DbPool, id: i64) -> Result<Self> { ... }
    pub async fn find_all(pool: &DbPool) -> Result<Vec<Self>> { ... }
    pub async fn create(pool: &DbPool, title: &str, completed: bool) -> Result<Self> { ... }
    pub async fn update(pool: &DbPool, id: i64, title: &str, completed: bool) -> Result<Self> { ... }
    pub async fn delete(pool: &DbPool, id: i64) -> Result<()> { ... }
}
```

`find_by_id` returns `Error::NotFound` (HTTP 404) when no row matches. `create` and `update` use `RETURNING` to give back the full row.

## Pagination

`Paginated<T>` runs a query with automatic `LIMIT`/`OFFSET` and total count:

```rust
async fn list(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {
    let page = Paginated::<Todo>::query(
        "SELECT id, title, completed FROM todos ORDER BY id DESC",
        &ctx.db,
        &pagination,
    ).await?;
    render!(IndexPage { page })
}
```

`PaginationParams` extracts `page` and `per_page` from the query string. It defaults to page 1, 25 per page, with `per_page` clamped to 1-100.

`Paginated<T>` provides: `items`, `page`, `per_page`, `total`, `total_pages`, `has_next`, and `has_prev`.
