+++
title = "Flash Messages"
weight = 8
description = "Cookie-based read-once messages for redirect-after-POST flows."
+++

Flash messages are one-time notifications displayed after a redirect. They are
the standard way to show "Post created" or "Login failed" messages in
traditional form workflows where the server redirects after processing a
submission.

## Creating flash messages

The `Flash` struct has three constructors matching common severity levels:

```rust
use blixt::prelude::*;

let success = Flash::success("Post created");
let error = Flash::error("Invalid credentials");
let info = Flash::info("Your session will expire in 5 minutes");
```

## Redirecting with a flash

Attach a flash to a `Redirect` response using `.with_flash()`:

```rust
use blixt::prelude::*;

async fn create_post(
    State(ctx): State<AppContext>,
    form: Form<PostForm>,
) -> Result<impl IntoResponse> {
    let data = form.into_inner();

    query!(
        "INSERT INTO posts (title, body) VALUES ($1, $2)",
        &data.title, &data.body
    )
    .execute(&ctx.db)
    .await?;

    Ok(Redirect::to("/posts")
        .with_flash(Flash::success("Post created")))
}
```

`Redirect::to` produces a `303 See Other` response with the `Location` header
set to the given path. When a flash is attached, it also sets a `blixt_flash`
cookie containing the message.

You can also redirect without a flash:

```rust
Ok(Redirect::to("/login"))
```

## Reading flash messages

Extract `Option<Flash>` in the handler that renders the destination page. The
extractor reads and consumes the flash cookie:

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/posts.html")]
struct PostsPage {
    posts: Vec<Post>,
    flash: Option<Flash>,
}

async fn list_posts(
    State(ctx): State<AppContext>,
    flash: Option<Flash>,
) -> Result<impl IntoResponse> {
    let posts = query_as!(Post, "SELECT * FROM posts ORDER BY created_at DESC")
        .fetch_all(&ctx.db)
        .await?;

    render!(PostsPage { posts, flash })
}
```

Use `Option<Flash>` rather than `Flash` directly. The `Flash` extractor returns
a rejection when no flash cookie is present, but wrapping it in `Option`
converts that rejection into `None`.

## Displaying in templates

Check flash presence and use the level methods to style the message:

```html
{% if let Some(flash) = flash %}
    <div class="{% if flash.is_success() %}bg-green-100 text-green-800{% endif %}{% if flash.is_error() %}bg-red-100 text-red-800{% endif %}{% if flash.is_info() %}bg-blue-100 text-blue-800{% endif %} p-4 rounded mb-4">
        {{ flash.message() }}
    </div>
{% endif %}
```

Available methods on `Flash`:
- `.message()` -- the message text
- `.is_success()` -- true for success level
- `.is_error()` -- true for error level
- `.is_info()` -- true for info level

## Cookie mechanics

The flash cookie uses the following settings:

| Attribute   | Value         |
|-------------|---------------|
| Name        | `blixt_flash` |
| Path        | `/`           |
| HttpOnly    | Yes           |
| SameSite    | Lax           |
| Max-Age     | 60 seconds    |

The cookie value is URL-encoded and formatted as `level:message` (for example,
`success%3APost%20created`). The `split_once(':')` parser correctly handles
colons within the message body.

The cookie is HttpOnly so JavaScript cannot read it. The 60-second Max-Age
provides a safety net -- even if the user does not immediately load the
redirect target, the cookie self-destructs.

The `Flash` extractor reads the cookie but does not delete it. The browser
removes it after Max-Age expires. In practice, the cookie is consumed on the
first page load after the redirect.

## Full example: CRUD with flash messages

```rust
use blixt::prelude::*;

pub fn routes() -> Router<AppContext> {
    Router::new()
        .route("/posts", get(list_posts))
        .route("/posts", post(create_post))
        .route("/posts/:id", delete(delete_post))
}

async fn list_posts(
    State(ctx): State<AppContext>,
    flash: Option<Flash>,
) -> Result<impl IntoResponse> {
    let posts = query_as!(Post, "SELECT * FROM posts ORDER BY created_at DESC")
        .fetch_all(&ctx.db)
        .await?;
    render!(PostsPage { posts, flash })
}

async fn create_post(
    State(ctx): State<AppContext>,
    form: Form<PostForm>,
) -> Result<impl IntoResponse> {
    let data = form.into_inner();
    query!(
        "INSERT INTO posts (title, body) VALUES ($1, $2)",
        &data.title, &data.body
    )
    .execute(&ctx.db)
    .await?;
    Ok(Redirect::to("/posts")
        .with_flash(Flash::success("Post created")))
}

async fn delete_post(
    State(ctx): State<AppContext>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse> {
    query!("DELETE FROM posts WHERE id = $1", id)
        .execute(&ctx.db)
        .await?;
    Ok(Redirect::to("/posts")
        .with_flash(Flash::info("Post deleted")))
}
```
