+++
title = "Routing"
weight = 1
description = "Define routes, extract parameters, and build your application with the App builder."
+++

# Routing

Blixt uses [Axum](https://docs.rs/axum) for routing. The `prelude` re-exports everything you need: `Router`, `get`, `post`, `put`, `delete`, plus extractors like `Path`, `Query`, and `State`.

```rust
use blixt::prelude::*;
```

## Defining routes

Register routes with Axum's `Router::new().route()` method. Each route takes a path pattern and a method handler.

```rust
fn routes() -> Router<AppContext> {
    Router::new()
        .route("/", get(index))
        .route("/posts", get(list).post(create))
        .route("/posts/{id}", get(show).put(update).delete(remove))
}
```

Path parameters use `{name}` syntax. Multiple HTTP methods can be chained on the same path.

## Handlers

A handler is an async function that returns `Result<impl IntoResponse>`. Use the `render!` macro for HTML responses.

```rust
async fn index() -> Result<impl IntoResponse> {
    render!(HomePage { title: "Welcome".to_string() })
}
```

## Path parameters

Extract path segments with `Path`:

```rust
async fn show(Path(id): Path<i64>) -> Result<impl IntoResponse> {
    // id is extracted from /posts/{id}
    render!(ShowPage { id })
}
```

For multiple parameters, destructure into a tuple:

```rust
async fn comment(
    Path((post_id, comment_id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse> {
    // from /posts/{post_id}/comments/{comment_id}
    todo!()
}
```

## Query parameters

Extract query strings with `Query` and a `Deserialize` struct:

```rust
#[derive(Deserialize)]
struct Filters {
    status: Option<String>,
    sort: Option<String>,
}

async fn list(Query(filters): Query<Filters>) -> Result<impl IntoResponse> {
    // /posts?status=published&sort=newest
    todo!()
}
```

For pagination, Blixt provides `PaginationParams` as a built-in extractor that reads `page` and `per_page` from the query string:

```rust
async fn list(pagination: PaginationParams) -> Result<impl IntoResponse> {
    // pagination.page() defaults to 1
    // pagination.per_page() defaults to 25, clamped to 1..=100
    todo!()
}
```

## State extraction

Share application state across handlers with `State`. Blixt provides `AppContext` which holds the database pool and configuration:

```rust
async fn index(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let posts = Select::from("posts")
        .columns(&["id", "title"])
        .fetch_all::<Post>(&ctx.db)
        .await?;
    render!(IndexPage { posts })
}
```

Create the context during startup:

```rust
let config = Config::from_env()?;
let pool = blixt::db::create_pool(&config).await?;
let ctx = AppContext::new(pool, config.clone());
```

## Route groups

Factor routes into separate functions per resource:

```rust
fn post_routes() -> Router<AppContext> {
    Router::new()
        .route("/posts", get(list).post(create))
        .route("/posts/{id}", get(show).put(update).delete(remove))
}

fn comment_routes() -> Router<AppContext> {
    Router::new()
        .route("/posts/{post_id}/comments", get(list_comments).post(add_comment))
}
```

Merge them into a single router:

```rust
fn routes() -> Router<AppContext> {
    post_routes().merge(comment_routes())
}
```

## The App builder

`App` assembles your routes with the middleware stack and starts the server. The builder has three methods:

| Method | Purpose |
|--------|---------|
| `.router(router)` | Sets the application router with user-defined routes |
| `.static_dir(path)` | Serves static files from `path` at `/static/` |
| `.serve()` | Binds to `host:port` from config and starts accepting connections |

```rust
#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;
    let config = Config::from_env()?;
    let pool = blixt::db::create_pool(&config).await?;
    blixt::db::migrate(&pool).await?;
    let ctx = AppContext::new(pool, config.clone());

    App::new(config)
        .router(routes().with_state(ctx))
        .static_dir("static")
        .serve()
        .await
}
```

When you call `.with_state(ctx)` on your router, Axum makes the context available to all handlers via `State<AppContext>`.

## Built-in middleware

`App` applies middleware in this order (outermost first):

1. **Tracing** -- request/response logging
2. **Request ID** -- adds a unique ID to each request
3. **Security headers** -- CSP, HSTS, X-Frame-Options, etc.
4. **Compression** -- gzip/brotli response compression

Static files served via `.static_dir()` also get immutable cache headers (`Cache-Control: public, max-age=31536000, immutable`) and dotfile/path-traversal protection.

## Minimal example

A complete application without a database:

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/home.html")]
struct HomePage {
    greeting: String,
}

async fn index() -> Result<impl IntoResponse> {
    render!(HomePage {
        greeting: "Hello from Blixt!".to_string(),
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;
    let config = Config::from_env()?;

    App::new(config)
        .router(Router::new().route("/", get(index)))
        .static_dir("static")
        .serve()
        .await
}
```
