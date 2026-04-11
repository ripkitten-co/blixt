+++
title = "Datastar (SSE Interactivity)"
weight = 7
description = "Build interactive UIs with Server-Sent Events and Datastar -- no JavaScript framework required."
+++

Blixt uses [Datastar](https://data-star.dev) for browser interactivity.
Datastar is a lightweight library that patches DOM elements and updates
client-side signals via Server-Sent Events (SSE). There is no JavaScript build
step, no virtual DOM, and no client-side framework. Your server renders HTML
fragments and the browser applies them.

## Core concepts

**Signals** are reactive key-value pairs stored in the browser. They drive form
state, visibility toggles, counters, and any other client-side data.

**Fragments** are HTML snippets rendered on the server. Datastar patches them
into the DOM by matching the `id` attribute of the root element.

**SSE events** are the transport. The server sends `text/event-stream` responses
with two event types:
- `datastar-patch-elements` -- replaces DOM elements
- `datastar-patch-signals` -- updates client-side signal values

## Response types

### `SseFragment` -- single DOM patch

Renders an Askama template and sends it as a `datastar-patch-elements` event:

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "fragments/counter.html")]
struct CounterFragment {
    count: i32,
}

async fn increment() -> Result<impl IntoResponse> {
    Ok(SseFragment::new(CounterFragment { count: 42 })?)
}
```

You can also create a fragment from raw HTML:

```rust
let fragment = SseFragment::from_html("<div id=\"status\">OK</div>".to_owned());
```

### `SseSignals` -- single signal update

Serializes any `Serialize` value as JSON and sends it as a
`datastar-patch-signals` event:

```rust
use blixt::prelude::*;
use serde_json::json;

async fn reset_form() -> Result<impl IntoResponse> {
    Ok(SseSignals::new(&json!({"title": "", "body": ""}))?)
}
```

### `SseResponse` -- multiple events in one response

Handlers often need to patch a DOM fragment and update signals in a single
response. `SseResponse` is a builder that concatenates events:

```rust
use blixt::prelude::*;

async fn create_item(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {
    let title: String = signals.get("title")?;

    query!("INSERT INTO items (title) VALUES ($1)", &title)
        .execute(&ctx.db)
        .await?;

    let items = query_as!(Item, "SELECT * FROM items")
        .fetch_all(&ctx.db)
        .await?;

    Ok(SseResponse::new()
        .patch(ItemListFragment { items })?
        .signals(&Signals::clear(&["title"]))?)
}
```

Methods on `SseResponse`:
- `.patch(template)` -- append a DOM patch from an Askama template
- `.patch_html(html)` -- append a DOM patch from raw HTML
- `.signals(data)` -- append a signal update from any `Serialize` value

Events appear in the response in insertion order.

### `SseStream` -- long-lived streaming

For real-time features (live feeds, progress bars), use `SseStream` to wrap any
`Stream` of SSE events:

```rust
use blixt::prelude::*;
use axum::response::sse::Event;

async fn live_feed() -> impl IntoResponse {
    let stream = async_stream::stream! {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            yield Ok::<_, std::convert::Infallible>(
                Event::default()
                    .event("datastar-patch-elements")
                    .data(format!("<span id=\"clock\">{}</span>", chrono::Utc::now()))
            );
        }
    };
    SseStream::new(stream)
}
```

## Reading signals from requests

### `DatastarSignals` extractor

Datastar sends client-side signal state with every request. The
`DatastarSignals` extractor parses them from:
- **POST/PUT/PATCH/DELETE**: JSON request body
- **GET**: `datastar` query parameter (URL-encoded JSON)

```rust
use blixt::prelude::*;

async fn search(signals: DatastarSignals) -> Result<impl IntoResponse> {
    let query: String = signals.get("search")?;
    let page: i64 = signals.get_opt("page")?.unwrap_or(1);

    // ... fetch results ...
    Ok(SseFragment::new(ResultsFragment { results })?)
}
```

Methods on `DatastarSignals`:
- `.get::<T>(key)` -- required typed value, returns `Error::BadRequest` if
  missing
- `.get_opt::<T>(key)` -- optional typed value, returns `Ok(None)` if missing
- `.has(key)` -- check if a signal exists
- `.keys()` -- iterate over all signal keys

The extractor enforces size limits: 64 KB max body, 100 max signals, 128 char
max key length, 8 KB max per value.

## Building signal payloads

The `Signals` builder creates JSON payloads for `SseResponse::signals` and
`SseSignals::new`:

```rust
use blixt::prelude::*;

// Set specific values
let payload = Signals::new()
    .set("title", "New Post")
    .set("count", 42)
    .set("active", true);

// Clear multiple fields to empty strings
let cleared = Signals::clear(&["title", "body", "error"]);
```

`Signals::set` accepts any type that implements `Into<serde_json::Value>`:
strings, numbers, booleans.

## Template attributes

Datastar uses HTML attributes to wire up interactivity. These go on your Askama
templates:

### `data-signals`

Initialize client-side signals. Place on the `<body>` or any container element:

```html
<div data-signals="{search: '', page: 1}">
```

### `data-bind`

Two-way bind a signal to a form input:

```html
<input type="text" data-bind="search" placeholder="Search...">
```

### `data-on` with actions

Trigger server requests on events. Datastar provides shorthand actions for HTTP
methods:

```html
<!-- GET request -->
<button data-on-click="@get('/api/items')">Refresh</button>

<!-- POST request -->
<form data-on-submit.prevent="@post('/api/items')">
    <input type="text" data-bind="title">
    <button type="submit">Create</button>
</form>

<!-- PUT request -->
<button data-on-click="@put('/api/items/{{ item.id }}')">Update</button>

<!-- DELETE request -->
<button data-on-click="@delete('/api/items/{{ item.id }}')">Delete</button>
```

The `.prevent` modifier calls `preventDefault()` on the event.

Datastar automatically sends all current signals as JSON with every request.
For `@get`, signals are sent as a URL-encoded `datastar` query parameter. For
`@post`/`@put`/`@delete`, signals are sent as a JSON request body.

### `data-text`

Bind a signal value as the text content of an element:

```html
<span data-text="$count">0</span>
```

### `data-show`

Conditionally show/hide an element based on a signal:

```html
<div data-show="$error !== ''">
    <p data-text="$error"></p>
</div>
```

## Example: interactive search

**Template (`fragments/search.html`):**

```html
<div id="search-container" data-signals="{search: ''}">
    <input
        type="text"
        data-bind="search"
        data-on-input.debounce_300ms="@get('/search')"
        placeholder="Type to search..."
    >
    <div id="results">
        {% for item in results %}
            <p>{{ item.title }}</p>
        {% endfor %}
    </div>
</div>
```

**Handler:**

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "fragments/results.html")]
struct ResultsFragment {
    results: Vec<Item>,
}

async fn search(
    State(ctx): State<AppContext>,
    signals: DatastarSignals,
) -> Result<impl IntoResponse> {
    let query: String = signals.get("search")?;

    let results = query_as!(
        Item,
        "SELECT * FROM items WHERE title ILIKE $1 LIMIT 20",
        format!("%{query}%")
    )
    .fetch_all(&ctx.db)
    .await?;

    Ok(SseFragment::new(ResultsFragment { results })?)
}
```

## How SSE events are formatted

Each response has `Content-Type: text/event-stream` and
`Cache-Control: no-cache`. Events use the SSE wire format:

```
event: datastar-patch-elements
data: elements <div id="list"> <p>Item 1</p> </div>

event: datastar-patch-signals
data: signals {"count":42}
```

Multi-line HTML is collapsed to a single line in the `data:` field to prevent
SSE framing issues.
