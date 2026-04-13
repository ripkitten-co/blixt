+++
title = "Testing"
weight = 17
description = "HTTP integration tests, database test isolation, and fake data generation."
+++

# Testing

Blixt provides a `testing` module with an HTTP test client, database pool
helpers, and a re-export of the `fake` crate for generating test data.

## Setup

Add the `test-helpers` feature to your dev-dependencies:

```toml
# Cargo.toml
[dev-dependencies]
blixt = { version = "0.5", features = ["test-helpers"] }
tokio = { version = "1", features = ["full"] }
```

Import from `blixt::testing`:

```rust
use blixt::testing::{TestClient, TestResponse, test_config};
```

## HTTP integration tests

`TestClient` wraps an Axum `Router` and provides a fluent API for sending
requests and asserting on responses:

```rust
use axum::routing::get;
use axum::http::StatusCode;
use blixt::prelude::*;
use blixt::testing::TestClient;

fn app() -> Router {
    Router::new().route("/health", get(|| async { "ok" }))
}

#[tokio::test]
async fn health_returns_200() {
    let client = TestClient::new(app());
    let resp = client.get("/health").await;
    resp.assert_status(StatusCode::OK);
    assert_eq!(resp.text().await, "ok");
}
```

### Request methods

```rust
client.get("/path").await                          // GET, returns TestResponse
client.post("/path").json(&body).send().await      // POST with JSON body
client.put("/path").json(&body).send().await       // PUT with JSON body
client.patch("/path").json(&body).send().await     // PATCH with JSON body
client.delete("/path").send().await                // DELETE
client.post("/path").header("x-key", "val").send().await  // custom header
client.post("/path").signals(&json!({})).send().await      // Datastar signals
```

### Response assertions

```rust
let resp = client.get("/api/users").await;

resp.assert_status(StatusCode::OK);           // panic if status doesn't match
resp.assert_header("content-type", "application/json");

let body = resp.text().await;                 // read body as string
let user: User = resp.json().await;           // deserialize JSON
let status = resp.status();                   // get StatusCode
let ct = resp.header("content-type");         // get header value
```

`assert_status` and `assert_header` return `self`, so you can chain:

```rust
client.get("/api/me").await
    .assert_status(StatusCode::OK)
    .assert_header("content-type", "application/json");
```

## Database tests

`TestPool` connects to a test database and runs migrations once per process.
Subsequent calls reuse the same pool.

Set `TEST_DATABASE_URL` in your environment or `.env`:

```bash
TEST_DATABASE_URL=postgres://user:pass@localhost/myapp_test
```

```rust
use blixt::testing::TestPool;
use blixt::prelude::*;

#[tokio::test]
async fn creates_user() {
    blixt::require_test_db!();
    let pool = TestPool::new().await;

    Insert::into("users")
        .set("email", "test@example.com")
        .set("name", "Test User")
        .returning::<User>(&["id", "email", "name"])
        .execute(&pool)
        .await
        .unwrap();
}
```

`require_test_db!()` skips the test if `TEST_DATABASE_URL` is not set, so
your test suite stays green in environments without a database.

`TestPool` implements `Deref<Target = DbPool>`, so you pass `&pool` directly
to query builder methods.

## Fake data

The `fake` crate is re-exported at `blixt::testing::fake`. Use it to generate
realistic test data:

```rust
use blixt::testing::fake::{Fake, faker::internet::en::*, faker::name::en::*};

fn random_email() -> String {
    SafeEmail().fake()
}

fn random_name() -> String {
    Name().fake()
}
```

### Factory pattern

Build test factories using the query builder's `.set()` method with overrides:

```rust
use blixt::prelude::*;
use blixt::testing::fake::{Fake, faker::internet::en::*, faker::name::en::*};

fn user_insert() -> Insert {
    Insert::into("users")
        .set("email", SafeEmail().fake::<String>())
        .set("name", Name().fake::<String>())
        .set("role", "user")
}

#[tokio::test]
async fn admin_can_delete() {
    blixt::require_test_db!();
    let pool = TestPool::new().await;

    let admin = user_insert()
        .set("role", "admin")
        .returning::<User>(&["id", "email", "name", "role"])
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(admin.role, "admin");
}
```

## Environment variable helpers

`with_env_vars` sets variables for the duration of a closure, then restores
the original values. It serializes via a mutex to avoid races between tests:

```rust
use blixt::testing::with_env_vars;

#[test]
fn config_reads_custom_port() {
    with_env_vars(&[("PORT", Some("8080"))], || {
        let config = Config::from_env().unwrap();
        assert_eq!(config.port, 8080);
    });
}
```

## Test configuration

`test_config()` returns a `Config` set to test mode on port 0 (OS-assigned),
useful for building routers in tests without needing environment variables:

```rust
use blixt::testing::test_config;

let config = test_config();
let app = App::new(config).router(routes).build_router();
```
