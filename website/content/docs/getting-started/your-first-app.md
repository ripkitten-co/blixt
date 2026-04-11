+++
title = "Your First App"
weight = 2
description = "Understand the project structure and add your first route"
+++

## How main.rs works

Every Blixt application starts from `src/main.rs`. Here is the generated entry point:

```rust
use blixt::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let config = Config::from_env()?;
    let app = App::new(config)
        .router(routes())
        .static_dir("static");

    app.serve().await
}

fn routes() -> Router {
    Router::new()
        .route("/", get(controllers::home::index))
        .route("/api/status", get(controllers::api::status))
        .route("/fragments/time", get(controllers::api::time_fragment))
        .route("/fragments/status", get(controllers::api::status_fragment))
}

mod controllers;
```

Walking through it:

1. **`init_tracing()`** sets up structured logging via the `tracing` crate
2. **`Config::from_env()`** reads `.env` and environment variables (`HOST`, `PORT`, `DATABASE_URL`, `JWT_SECRET`, `BLIXT_ENV`)
3. **`App::new(config)`** creates the application builder, which layers on middleware automatically: tracing, request IDs, security headers (CSP, HSTS, X-Frame-Options, etc.), and gzip compression
4. **`.router(routes())`** attaches your route definitions
5. **`.static_dir("static")`** serves files from `static/` at the `/static/` URL prefix, with cache headers and dotfile blocking
6. **`.serve().await`** binds to the configured address and starts accepting connections

## The prelude

`blixt::prelude::*` re-exports everything you need for typical handler code:

- **App, Config, AppContext** -- application setup and shared state
- **Router, get, post, put, delete** -- Axum routing
- **Path, Query, State** -- Axum extractors
- **Template** -- Askama's `#[derive(Template)]`
- **Html, IntoResponse** -- response types
- **Serialize, Deserialize, FromRow** -- serde and SQLx derives
- **render!** -- macro for rendering templates into HTTP responses
- **DatastarSignals, SseFragment, SseResponse, Signals** -- Datastar SSE types
- **Paginated, PaginationParams** -- pagination support
- **Insert, Select, Update, Delete** -- type-safe query builder
- **Error, Result** -- typed error handling
- **info!, warn!, error!, debug!** -- tracing macros

## Project structure

```
my_app/
  src/
    main.rs              # entry point, route registration
    controllers/
      mod.rs             # declares controller modules
      home.rs            # page handlers
      api.rs             # JSON API + SSE fragment handlers
  templates/
    layouts/
      app.html           # base HTML layout (head, body, scripts)
    pages/
      home.html          # full-page templates (extend a layout)
    fragments/           # partial HTML for SSE patching
      time.html
      status.html
    components/          # reusable template partials
    emails/              # email templates
  static/
    css/
      app.css            # Tailwind source (imports + @source directives)
      output.css         # compiled CSS (gitignored, built by Tailwind)
    js/
      datastar.js        # Datastar runtime (downloaded at project creation)
  migrations/            # timestamped SQL files
  .env.example           # environment variable template
```

Templates follow Askama conventions. Layouts use `{% block %}` inheritance, pages extend layouts with `{% extends "layouts/app.html" %}`, and fragments are standalone HTML snippets used for Datastar SSE patching.

## Adding a controller

Create a new file at `src/controllers/about.rs`:

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/about.html")]
struct AboutPage {
    version: String,
}

pub async fn index() -> Result<impl IntoResponse> {
    render!(AboutPage {
        version: "0.1.0".to_string(),
    })
}
```

The `render!` macro calls `.render()` on the Askama template and wraps the result in `Html(...)`, propagating template errors through `blixt::Error`.

## Create the template

Add `templates/pages/about.html`:

```html
{% extends "layouts/app.html" %}
{% block title %}About{% endblock %}
{% block content %}
<main class="min-h-screen flex items-center justify-center">
  <div class="text-center">
    <h1 class="text-lg font-medium text-zinc-200">About</h1>
    <p class="text-sm text-zinc-400 mt-2">Version {{ version }}</p>
  </div>
</main>
{% endblock %}
```

## Register the module

Add the module to `src/controllers/mod.rs`:

```rust
pub mod api;
pub mod home;
pub mod about;
```

## Register the route

Add the route in `src/main.rs`:

```rust
fn routes() -> Router {
    Router::new()
        .route("/", get(controllers::home::index))
        .route("/about", get(controllers::about::index))
        .route("/api/status", get(controllers::api::status))
        .route("/fragments/time", get(controllers::api::time_fragment))
        .route("/fragments/status", get(controllers::api::status_fragment))
}
```

## Using blixt dev

With the dev server running (`blixt dev`), save any of these files and the server automatically restarts:

- **`.rs` files** in `src/` -- triggers a full recompile and restart
- **`.html` files** in `templates/` -- triggers a restart (Askama templates are compiled into the binary)
- **`.toml` files** -- triggers a restart on dependency changes

Tailwind CSS runs in parallel watch mode. When you change template classes, the CSS recompiles instantly and the browser picks up the new stylesheet via the `data-blixt-css` attribute on the link tag.

The file watcher debounces rapid changes (300ms) to avoid unnecessary rebuilds.

## Adding database access

When your app needs a database, expand `main.rs` to create a connection pool and run migrations:

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
```

`AppContext` holds the `DbPool` and `Config`. Pass it to handlers with Axum's `State` extractor:

```rust
pub async fn index(
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let items = Select::from("posts")
        .columns(&["id", "title"])
        .fetch_all::<Post>(&ctx.db)
        .await?;
    render!(PostIndex { items })
}
```
