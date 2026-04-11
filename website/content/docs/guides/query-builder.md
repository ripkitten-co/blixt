+++
title = "Query Builder"
weight = 4
description = "Build SELECT, INSERT, UPDATE, and DELETE queries with a type-safe fluent API."
+++

# Query Builder

Blixt includes a query builder for dynamic CRUD operations. It handles parameterized placeholders for both Postgres (`$1`, `$2`) and SQLite (`?`) automatically based on your feature flag.

All builders are available from the prelude:

```rust
use blixt::prelude::*;
// Brings in: Select, Insert, Update, Delete, Order, Value
```

## Select

### Basic query

```rust
let posts = Select::from("posts")
    .columns(&["id", "title", "published"])
    .fetch_all::<Post>(&pool)
    .await?;
```

Omitting `.columns()` selects `*`.

### Filtering

Chain `.where_*()` methods to add conditions. Multiple conditions are joined with `AND`.

```rust
let posts = Select::from("posts")
    .columns(&["id", "title"])
    .where_eq("published", true)
    .where_gt("score", 10i64)
    .fetch_all::<Post>(&pool)
    .await?;
```

Available filter methods:

| Method | SQL operator |
|--------|-------------|
| `.where_eq(col, val)` | `=` |
| `.where_ne(col, val)` | `!=` |
| `.where_gt(col, val)` | `>` |
| `.where_lt(col, val)` | `<` |
| `.where_gte(col, val)` | `>=` |
| `.where_lte(col, val)` | `<=` |

Values are automatically converted via the `Value` enum. Supported types: `&str`, `String`, `i32`, `i64`, `f32`, `f64`, and `bool`.

### Ordering, limit, offset

```rust
let recent = Select::from("posts")
    .columns(&["id", "title"])
    .where_eq("published", true)
    .order_by("created_at", Order::Desc)
    .limit(10)
    .offset(20)
    .fetch_all::<Post>(&pool)
    .await?;
```

### Fetch methods

| Method | Returns | On no rows |
|--------|---------|------------|
| `.fetch_all::<T>(pool)` | `Vec<T>` | Empty vec |
| `.fetch_one::<T>(pool)` | `T` | `Error::NotFound` (HTTP 404) |
| `.fetch_optional::<T>(pool)` | `Option<T>` | `None` |

The target type `T` must implement `sqlx::FromRow`.

## Insert

### With RETURNING

```rust
let post = Insert::into("posts")
    .set("title", "Hello world")
    .set("published", false)
    .returning::<Post>(&["id", "title", "published", "created_at", "updated_at"])
    .execute(&pool)
    .await?;
```

`.returning()` adds a `RETURNING` clause and deserializes the inserted row into `T`.

### Without RETURNING

```rust
Insert::into("posts")
    .set("title", "Hello world")
    .set("published", false)
    .execute_no_return(&pool)
    .await?;
```

## Update

### With RETURNING

```rust
let post = Update::table("posts")
    .set("title", "Updated title")
    .set_timestamp("updated_at")
    .where_eq("id", 1i64)
    .returning::<Post>(&["id", "title", "published", "created_at", "updated_at"])
    .execute(&pool)
    .await?;
```

`.set_timestamp(col)` sets a column to `CURRENT_TIMESTAMP` without binding a value. Use it for `updated_at` columns.

### Without RETURNING

```rust
Update::table("posts")
    .set("score", 0i64)
    .where_gt("score", 10i64)
    .execute_no_return(&pool)
    .await?;
```

Update supports the same `.where_*()` methods as Select.

## Delete

```rust
Delete::from("posts")
    .where_eq("id", post_id)
    .execute(&pool)
    .await?;
```

A `DELETE` without any `.where_*()` conditions logs a warning, since deleting all rows is usually unintentional.

Delete supports the same `.where_*()` methods as Select.

## Builder vs query! macros

Use the **query builder** when:
- The query structure varies at runtime (optional filters, dynamic sorting)
- You want CRUD operations without writing raw SQL
- You are building model methods (`find_by_id`, `create`, `update`, `delete`)

Use **query! macros** when:
- The query is a fixed string known at compile time
- You need joins, subqueries, aggregations, or other complex SQL
- You want SQLx compile-time verification against your database schema

Both approaches use parameterized binding -- neither is vulnerable to SQL injection.

## Complete model example

A model with all five standard CRUD methods using the query builder:

```rust
use blixt::prelude::*;
use sqlx::types::chrono::{DateTime, Utc};

const TABLE: &str = "posts";
const COLUMNS: &[&str] = &["id", "title", "published", "created_at", "updated_at"];

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Post {
    pub id: i64,
    pub title: String,
    pub published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Post {
    pub async fn find_by_id(pool: &DbPool, id: i64) -> Result<Self> {
        Select::from(TABLE).columns(COLUMNS).where_eq("id", id)
            .fetch_one::<Self>(pool).await
    }

    pub async fn find_all(pool: &DbPool) -> Result<Vec<Self>> {
        Select::from(TABLE).columns(COLUMNS).order_by("id", Order::Desc)
            .fetch_all::<Self>(pool).await
    }

    pub async fn create(pool: &DbPool, title: &str, published: bool) -> Result<Self> {
        Insert::into(TABLE)
            .set("title", title)
            .set("published", published)
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }

    pub async fn update(pool: &DbPool, id: i64, title: &str, published: bool) -> Result<Self> {
        Update::table(TABLE)
            .set("title", title)
            .set("published", published)
            .set_timestamp("updated_at")
            .where_eq("id", id)
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }

    pub async fn delete(pool: &DbPool, id: i64) -> Result<()> {
        Delete::from(TABLE).where_eq("id", id).execute(pool).await
    }
}
```

This is the same pattern that `blixt generate model` produces. The `TABLE` and `COLUMNS` constants keep column lists in sync between methods. `find_by_id` returns `Error::NotFound` (HTTP 404) when no row matches, while `find_all` returns an empty vec.
