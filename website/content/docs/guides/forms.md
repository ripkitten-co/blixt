+++
title = "Forms & CSRF Protection"
weight = 5
description = "Type-safe form extraction with automatic CSRF validation using the double-submit cookie pattern."
+++

Blixt provides a `Form<T>` extractor that deserializes URL-encoded form data
and validates CSRF tokens on state-changing requests. The CSRF middleware uses
the double-submit cookie pattern with constant-time comparison and Origin header
validation as defense-in-depth.

## How CSRF protection works

On **safe methods** (GET, HEAD, OPTIONS), the CSRF middleware generates a random
token (UUID v7) and sets it as both a `blixt_csrf` cookie (`SameSite=Strict`)
and an `x-csrf-token` response header.

On **state-changing methods** (POST, PUT, PATCH, DELETE), the `Form<T>`
extractor validates that the submitted token matches the `blixt_csrf` cookie.
The token can be submitted via either:

1. An `x-csrf-token` request header (preferred for Datastar/AJAX)
2. A hidden `_csrf` form field (traditional HTML forms)

The header is checked first. If neither is present or the token does not match
the cookie, the request is rejected with `403 Forbidden`.

## The `Form<T>` extractor

`Form<T>` works like Axum's built-in form extractor but adds automatic CSRF
validation. Define a struct with `Deserialize` and use it as a handler parameter:

```rust
use blixt::prelude::*;

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

async fn login(form: Form<LoginForm>) -> Result<impl IntoResponse> {
    let data = form.into_inner();
    // data.email, data.password are available
    // CSRF was already validated before this code runs
    Ok(Redirect::to("/dashboard"))
}
```

The body size is capped at 64 KB. If deserialization fails, the extractor
returns `400 Bad Request`.

## The `CsrfToken` extractor

Use `CsrfToken` in handlers that render forms. It reads the token from the
`blixt_csrf` cookie set by the CSRF middleware:

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/login.html")]
struct LoginPage {
    csrf_token: String,
}

async fn login_page(csrf: CsrfToken) -> Result<impl IntoResponse> {
    render!(LoginPage {
        csrf_token: csrf.value().to_owned(),
    })
}
```

## Template: hidden `_csrf` field

In your Askama template, add a hidden input with the token value:

```html
<form method="post" action="/login">
    <input type="hidden" name="_csrf" value="{{ csrf_token }}">

    <label for="email">Email</label>
    <input type="email" name="email" id="email" required>

    <label for="password">Password</label>
    <input type="password" name="password" id="password" required>

    <button type="submit">Log in</button>
</form>
```

## Using the `x-csrf-token` header

For Datastar or AJAX requests, read the token from the `x-csrf-token` response
header on any GET request and send it back as a request header on mutations.
Datastar does this automatically when you use `@post`, `@put`, `@patch`, or
`@delete` actions.

For manual fetch calls:

```html
<script>
const token = document.cookie
    .split('; ')
    .find(c => c.startsWith('blixt_csrf='))
    ?.split('=')[1];

fetch('/api/submit', {
    method: 'POST',
    headers: { 'x-csrf-token': token },
    body: formData,
});
</script>
```

## Full example: login form

**Router:**

```rust
use blixt::prelude::*;

pub fn routes() -> Router<AppContext> {
    Router::new()
        .route("/login", get(login_page))
        .route("/login", post(login))
}
```

**Handler:**

```rust
use blixt::prelude::*;
use blixt::auth::password::verify_password;

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

async fn login_page(csrf: CsrfToken) -> Result<impl IntoResponse> {
    render!(LoginPage {
        csrf_token: csrf.value().to_owned(),
    })
}

async fn login(
    State(ctx): State<AppContext>,
    form: Form<LoginForm>,
) -> Result<impl IntoResponse> {
    let data = form.into_inner();

    let user = query_as!(User, "SELECT * FROM users WHERE email = $1", &data.email)
        .fetch_optional(&ctx.db)
        .await?
        .ok_or(Error::Unauthorized)?;

    if !verify_password(&data.password, &user.password_hash)? {
        return Err(Error::Unauthorized);
    }

    Ok(Redirect::to("/dashboard")
        .with_flash(Flash::success("Welcome back!")))
}
```

## Security notes

- The CSRF cookie uses `SameSite=Strict` and adds the `Secure` flag in
  production.
- Token comparison uses constant-time equality to prevent timing side-channels.
- The middleware also validates the `Origin` header against the `Host` header
  when present, rejecting cross-origin submissions.
- GET requests skip CSRF validation entirely since they should not modify state.
