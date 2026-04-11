+++
title = "Error Handling"
weight = 11
description = "Typed error variants, HTTP status mapping, information leakage prevention, and the Redact wrapper."
+++

# Error Handling

Blixt uses a single `Error` enum that maps directly to HTTP status codes. Handlers return `Result<T>` (aliased to `std::result::Result<T, blixt::Error>`) and the framework converts errors into appropriate HTTP responses automatically.

## The Error enum

```rust
use blixt::prelude::*;

pub enum Error {
    Io(std::io::Error),
    Database(sqlx::Error),
    NotFound,
    Unauthorized,
    Forbidden,
    BadRequest(String),
    RateLimited { retry_after_secs: Option<u64> },
    Validation(ValidationErrors),
    Internal(String),
}
```

### HTTP status mapping

`Error` implements `IntoResponse`, so Axum converts it to an HTTP response automatically:

| Variant | Status Code | Response Body |
|---------|------------|---------------|
| `NotFound` | 404 | `"Not found"` |
| `Unauthorized` | 401 | `"Unauthorized"` |
| `Forbidden` | 403 | `"Forbidden"` |
| `BadRequest(msg)` | 400 | The provided message |
| `Validation(errors)` | 422 | JSON object with per-field errors |
| `RateLimited { .. }` | 429 | `"Too many requests"` + `Retry-After` header |
| `Io(..)` | 500 | `"Internal server error"` |
| `Database(..)` | 500 | `"Internal server error"` |
| `Internal(..)` | 500 | `"Internal server error"` |

### No information leakage

Internal errors (`Io`, `Database`, `Internal`) are logged with full details via `tracing::error!` but the HTTP response always returns the generic `"Internal server error"` string. SQL connection strings, file paths, and stack traces never reach the client.

```rust
// This logs "Database error: connection refused to host=db.internal"
// but the client only sees "Internal server error"
let user = sqlx::query_as("SELECT * FROM users WHERE id = $1")
    .bind(id)
    .fetch_one(&pool)
    .await?; // ? converts sqlx::Error into Error::Database
```

## Result alias

The `blixt::error::Result<T>` type alias is re-exported in the prelude:

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

Use it in handlers and across your application code:

```rust
use blixt::prelude::*;

pub async fn show_user(Path(id): Path<i64>, State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let user = find_user(&ctx.db, id).await?;
    render!(UserPage { user })
}
```

## Returning errors from handlers

Use the error variants directly:

```rust
use blixt::prelude::*;

pub async fn get_item(Path(id): Path<i64>) -> Result<impl IntoResponse> {
    if id < 0 {
        return Err(Error::BadRequest("ID must be positive".into()));
    }
    // ...
    Ok(Html("found"))
}
```

The `?` operator handles conversions automatically for `std::io::Error` and `sqlx::Error` via `From` implementations.

## Validation errors

`ValidationErrors` holds per-field error messages and serializes to JSON:

```rust
use blixt::error::{Error, ValidationErrors};

pub async fn create_post(Form(input): Form<NewPost>) -> Result<impl IntoResponse> {
    let mut errors = ValidationErrors::new();

    if input.title.is_empty() {
        errors.add("title", "must not be empty".into());
    }
    if input.title.len() > 255 {
        errors.add("title", "must be at most 255 characters".into());
    }
    if input.priority < 1 || input.priority > 5 {
        errors.add("priority", "must be between 1 and 5".into());
    }

    if !errors.is_empty() {
        return Err(Error::Validation(errors));
    }

    // ... create the post
    Ok(Html("created"))
}
```

The 422 response body looks like:

```json
{
  "errors": {
    "title": ["must not be empty", "must be at most 255 characters"],
    "priority": ["must be between 1 and 5"]
  }
}
```

## Rate limited errors

The `RateLimited` variant optionally includes a `Retry-After` header value:

```rust
Err(Error::RateLimited { retry_after_secs: Some(60) })
```

This produces a `429 Too Many Requests` response with `Retry-After: 60`. Pass `None` to omit the header.

## Redact\<T\>

The `Redact<T>` wrapper prevents sensitive values from leaking into logs, debug output, or serialized representations. `Debug`, `Display`, and `Serialize` all emit `[REDACTED]` instead of the real value.

```rust
use blixt::redact::Redact;

let api_key = Redact::new("sk-live-abc123".to_string());

// These all print "[REDACTED]"
println!("{}", api_key);
println!("{:?}", api_key);
let json = serde_json::to_string(&api_key).unwrap(); // "\"[REDACTED]\""
```

Access the inner value when you need it for business logic:

```rust
let key = api_key.expose();       // &String
let owned = api_key.into_inner(); // String
```

`Redact<T>` implements `Clone`, `PartialEq`, `From<T>`, `Serialize`, and `Deserialize`. Deserialization passes through to the inner type, so you can read a `Redact<String>` from JSON and the actual value is preserved internally.

```rust
let token: Redact<String> = serde_json::from_str("\"my-token\"").unwrap();
assert_eq!(token.expose(), "my-token");
assert_eq!(format!("{}", token), "[REDACTED]");
```

Use `Redact` for API keys, tokens, or any value that should never appear in logs or error messages. For database connection strings and JWT secrets, the framework's `Config` struct already uses `SecretString` from the `secrecy` crate with the same redaction behavior.
