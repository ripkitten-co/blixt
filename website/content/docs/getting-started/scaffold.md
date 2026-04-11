+++
title = "Scaffold a CRUD App"
weight = 3
description = "Generate a full CRUD application in one command"
+++

## Generate a scaffold

The `blixt generate scaffold` command creates a complete CRUD resource -- model, controller, templates, and migration -- in one step:

```bash
blixt generate scaffold post title:string body:text published:bool
```

The first argument is the resource name (singular). After that, list fields as `name:type` pairs.

### Supported field types

| Type | Aliases | Rust type | Postgres SQL | SQLite SQL |
|------|---------|-----------|-------------|------------|
| `string` | `text` | `String` | `TEXT NOT NULL` | `TEXT NOT NULL` |
| `int` | `integer` | `i64` | `BIGINT NOT NULL DEFAULT 0` | `INTEGER NOT NULL DEFAULT 0` |
| `bool` | `boolean` | `bool` | `BOOLEAN NOT NULL DEFAULT FALSE` | `BOOLEAN NOT NULL DEFAULT 0` |
| `float` | | `f64` | `DOUBLE PRECISION NOT NULL DEFAULT 0` | `REAL NOT NULL DEFAULT 0` |

Every table automatically gets `id`, `created_at`, and `updated_at` columns. You cannot use these as field names, and SQL reserved words (`select`, `order`, `table`, etc.) are also rejected.

## What gets generated

For `blixt generate scaffold post title:string body:text published:bool`, the CLI creates:

```
src/
  models/
    mod.rs              # pub mod post;
    post.rs             # Post struct with CRUD methods
  controllers/
    mod.rs              # pub mod post; (appended)
    post.rs             # index, show, create, update, destroy handlers
templates/
  pages/
    post/
      index.html        # list page with form and list includes
      show.html         # detail page with edit form and delete
  fragments/
    post/
      form.html         # create form with Datastar signal bindings
      list.html         # paginated list with prev/next navigation
      item.html         # single row in the list
migrations/
  {timestamp}_create_posts.sql
```

## The migration

The generated SQL migration creates the table with your fields plus automatic timestamps:

```sql
CREATE TABLE IF NOT EXISTS posts (
    id BIGSERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

Run it with:

```bash
blixt db migrate
```

Other database commands:

```bash
blixt db status      # check which migrations have run
blixt db rollback    # undo the last migration
```

## The model

The generated model (`src/models/post.rs`) defines the struct and CRUD operations using Blixt's query builder:

```rust
use blixt::prelude::*;
use sqlx::types::chrono::{DateTime, Utc};

const TABLE: &str = "posts";
const COLUMNS: &[&str] = &["id", "title", "body", "published", "created_at", "updated_at"];

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Post {
    pub id: i64,
    pub title: String,
    pub body: String,
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

    pub async fn create(pool: &DbPool, title: &str, body: &str, published: bool) -> Result<Self> {
        Insert::into(TABLE)
            .set("title", title)
            .set("body", body)
            .set("published", published)
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }

    pub async fn update(pool: &DbPool, id: i64, title: &str, body: &str, published: bool) -> Result<Self> {
        Update::table(TABLE)
            .set("title", title)
            .set("body", body)
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

The query builder (`Insert`, `Select`, `Update`, `Delete`) generates parameterized SQL, preventing injection. The `returning` clause fetches the row back after insert/update so you get the full struct including `id` and timestamps.

## The controller

The generated controller (`src/controllers/post.rs`) has six handlers:

```rust
use blixt::prelude::*;
use blixt::validate::Validator;
use blixt::datastar::Signals;
use crate::models::post::Post;

const PER_PAGE: u32 = 10;

#[derive(Template)]
#[template(path = "pages/post/index.html")]
pub struct PostIndex {
    pub page: Paginated<Post>,
}

#[derive(Template)]
#[template(path = "fragments/post/list.html")]
pub struct PostListFragment {
    pub page: Paginated<Post>,
}

#[derive(Template)]
#[template(path = "pages/post/show.html")]
pub struct PostShow {
    pub item: Post,
}
```

### index -- paginated list

```rust
pub async fn index(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {
    let page = fetch_page(&ctx.db, pagination.page()).await?;
    render!(PostIndex { page })
}
```

`PaginationParams` is an Axum extractor that reads `?page=N&per_page=N` from the query string, defaulting to page 1 with 25 items per page. The `Paginated<Post>` result carries `items`, `total`, `total_pages`, `has_next`, and `has_prev` for use in templates.

### show -- single record

```rust
pub async fn show(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse> {
    let item = Post::find_by_id(&ctx.db, id).await?;
    render!(PostShow { item })
}
```

### create -- with validation

```rust
pub async fn create(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {
    let title: String = signals.get("title")?;
    let body: String = signals.get("body")?;
    let published: bool = signals.get("published")?;

    let mut v = Validator::new();
    v.str_field(&title, "title").not_empty().max_length(255);
    v.str_field(&body, "body").not_empty().max_length(255);
    v.check()?;

    Post::create(&ctx.db, &title, &body, published).await?;
    let page = fetch_page(&ctx.db, 1).await?;
    SseResponse::new()
        .patch(PostListFragment { page })?
        .signals(&Signals::clear(&["title", "body", "published"]))
}
```

`DatastarSignals` is an Axum extractor that reads Datastar's signal state from the request body (POST/PUT/DELETE) or query string (GET). The `signals.get("title")?` call extracts a typed value, returning a `BadRequest` error if missing or wrong type.

`Validator` chains field validations. `v.check()?` returns a `422 Unprocessable Entity` with all collected errors if any fail.

After a successful create, the handler returns an `SseResponse` that does two things in one SSE stream: patches the list fragment with fresh data, and clears the form signals so the inputs reset.

### update -- edit and save

```rust
pub async fn update(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {
    let title: String = signals.get("title")?;
    let body: String = signals.get("body")?;
    let published: bool = signals.get("published")?;

    let mut v = Validator::new();
    v.str_field(&title, "title").not_empty().max_length(255);
    v.str_field(&body, "body").not_empty().max_length(255);
    v.check()?;

    let item = Post::update(&ctx.db, id, &title, &body, published).await?;
    let html = PostShow { item }
        .render()
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(SseResponse::new().patch_html(&html))
}
```

### destroy -- delete with page adjustment

```rust
pub async fn destroy(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {
    Post::delete(&ctx.db, id).await?;
    let mut page = fetch_page(&ctx.db, pagination.page()).await?;
    if page.items.is_empty() && page.page > 1 {
        page = fetch_page(&ctx.db, page.page - 1).await?;
    }
    SseFragment::new(PostListFragment { page })
}
```

If deleting the last item on a page leaves it empty, the handler steps back one page so the user doesn't see a blank list.

### page_handler -- SSE pagination

```rust
pub async fn page_handler(
    State(ctx): State<AppContext>,
    pagination: PaginationParams,
) -> Result<impl IntoResponse> {
    let page = fetch_page(&ctx.db, pagination.page()).await?;
    SseFragment::new(PostListFragment { page })
}
```

Pagination links in the template call this endpoint via Datastar to swap the list fragment without a full page reload.

## How Datastar works in the templates

Blixt uses [Datastar](https://data-star.dev) instead of a JavaScript framework. All interactivity goes through HTML attributes and SSE:

### Client-side signals

The form fragment declares reactive signals with `data-signals-*` attributes:

```html
<form
  class="mb-6 space-y-3"
  data-signals-title="''"
  data-signals-body="''"
  data-signals-published="false"
  data-on:submit="@post('/posts')"
>
  <input type="text" data-bind:title placeholder="Title" ... >
  <input type="text" data-bind:body placeholder="Body" ... >
  <label>
    <input type="checkbox" data-bind:published ... >
    Published
  </label>
  <button type="submit">Create Post</button>
</form>
```

- **`data-signals-title="''"`** -- declares a reactive signal named `title` with an empty string default
- **`data-bind:title`** -- two-way binds the input value to the `title` signal
- **`data-on:submit="@post('/posts')"`** -- on form submit, sends a POST request with all signals as the JSON body

### Server-side patching

When the server responds, it sends SSE events that Datastar processes:

```
event: datastar-patch-elements
data: elements <div id="post-list">...</div>

event: datastar-patch-signals
data: signals {"title":"","body":"","published":""}
```

- **`datastar-patch-elements`** -- replaces DOM elements by matching `id` attributes
- **`datastar-patch-signals`** -- updates client-side signal values (clears the form)

### SSE navigation

List pagination and delete buttons use `data-on:click` with `@get` and `@delete`:

```html
<button data-on:click="@get('/posts/page?page=2')">Next</button>
<button data-on:click="@delete('/posts/5?page=1')">Delete</button>
```

These send requests to the server, which responds with an `SseFragment` or `SseResponse` containing the new HTML. Datastar patches the DOM in place -- no full page reload needed.

## Wire up the routes

Add the CRUD routes and models module to `src/main.rs`:

```rust
use blixt::prelude::*;

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

fn routes() -> Router<AppContext> {
    Router::new()
        .route("/", get(controllers::home::index))
        .route("/posts", get(controllers::post::index))
        .route("/posts", post(controllers::post::create))
        .route("/posts/page", get(controllers::post::page_handler))
        .route("/posts/{id}", get(controllers::post::show))
        .route("/posts/{id}", put(controllers::post::update))
        .route("/posts/{id}", delete(controllers::post::destroy))
}

mod controllers;
mod models;
```

## Try it out

Run the migration and start the dev server:

```bash
blixt db migrate
blixt dev
```

Open [http://localhost:3000/posts](http://localhost:3000/posts). You can:

- Create posts with the form at the top
- Browse paginated results with Prev/Next buttons
- Click a post to view and edit it
- Delete posts from the list or detail page

All interactions happen over SSE -- the browser never does a full page reload after the initial load.

## Generating other resources

The scaffold command is the all-in-one option, but you can also generate pieces individually:

```bash
# Controller with index and show views (no model or migration)
blixt generate controller comment

# Model with migration (no controller or templates)
blixt generate model comment body:text author:string post_id:int
```

If you omit fields from a scaffold, it defaults to a single `name:string` field:

```bash
blixt generate scaffold tag
# equivalent to: blixt generate scaffold tag name:string
```
