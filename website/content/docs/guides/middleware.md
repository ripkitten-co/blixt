+++
title = "Middleware"
weight = 10
description = "Built-in middleware stack, CSRF protection, rate limiting, security headers, and custom middleware."
+++

# Middleware

Blixt ships a layered middleware stack that is applied automatically when you call `App::new(config).router(routes).serve().await`. Understanding the order and capabilities of each layer helps you extend the stack for your own needs.

## Built-in stack

The `App::build_router()` method assembles layers in this order (outermost first):

```rust
router
    .layer(CompressionLayer::new())
    .layer(middleware::from_fn(security_headers))
    .layer(middleware::from_fn(request_id))
    .layer(TraceLayer::new_for_http())
```

A request passes through these layers top-to-bottom:

1. **Tracing** (`tower_http::trace::TraceLayer`) -- emits structured spans for every HTTP request, including method, path, and status code.
2. **Request ID** (`middleware::request_id`) -- generates a UUID v7, attaches it as a `tracing` span field, and sets the `x-request-id` response header. Every log line within the request carries this ID.
3. **Security headers** (`middleware::security_headers`) -- sets six security headers on every response (see below).
4. **Compression** (`tower_http::compression::CompressionLayer`) -- gzip/brotli/deflate compression of response bodies.

User-defined routes sit inside this stack, so your handlers benefit from all layers without any extra configuration.

## Security headers

The `security_headers` middleware sets the following headers on every response:

| Header | Value |
|--------|-------|
| `Content-Security-Policy` | `default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'` |
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains; preload` |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=()` |

The CSP includes `'unsafe-eval'` because Datastar's expression engine relies on the `Function()` constructor. This is a framework-level requirement.

You can use `security_headers` standalone on any router:

```rust
use axum::middleware;
use blixt::middleware::security_headers::security_headers;

let app = Router::new()
    .route("/api", get(handler))
    .layer(middleware::from_fn(security_headers));
```

## CSRF protection

Blixt uses the double-submit cookie pattern with Origin header validation as defense-in-depth.

**How it works:**

- On safe methods (`GET`, `HEAD`, `OPTIONS`): the middleware sets a `blixt_csrf` cookie and an `x-csrf-token` response header with a matching random token (UUID v7).
- On state-changing methods (`POST`, `PUT`, `DELETE`, `PATCH`): the middleware validates that the `x-csrf-token` request header matches the `blixt_csrf` cookie. If an `Origin` header is present, it must match the `Host` header.

If validation fails, the request receives a `403 Forbidden` response. Token comparison uses constant-time equality to mitigate timing side-channels.

### Adding CSRF to your routes

```rust
use axum::middleware;
use blixt::middleware::csrf::csrf_protection;

let secure = true; // set to true in production for the Secure cookie flag

let app = Router::new()
    .route("/form", get(show_form))
    .route("/submit", post(handle_submit))
    .layer(middleware::from_fn(move |req, next| {
        csrf_protection(req, next, secure)
    }));
```

The `secure` parameter controls whether the cookie includes the `Secure` flag (HTTPS-only). Pass `true` in production.

### Client-side usage

When rendering forms, read the `x-csrf-token` response header from any GET request and include it in subsequent POST requests:

```html
<input type="hidden" name="_csrf" value="{{ csrf_token }}" />
```

Or set the `x-csrf-token` header in JavaScript fetch calls:

```javascript
fetch("/submit", {
    method: "POST",
    headers: { "x-csrf-token": token },
    body: formData,
});
```

## Rate limiting

The `RateLimiter` implements token-bucket rate limiting keyed by client IP. Each IP receives a token budget that replenishes at a constant rate over a time window. Exceeding the budget returns `429 Too Many Requests` with a `Retry-After` header.

### Presets

```rust
use blixt::middleware::rate_limit::RateLimiter;

// 100 requests per 60 seconds (general API traffic)
let limiter = RateLimiter::default_limit();

// 10 requests per 60 seconds (login, password reset)
let limiter = RateLimiter::strict();

// Custom: 50 requests per 30 seconds
let limiter = RateLimiter::new(50, 30);
```

### Applying to routes

```rust
use axum::middleware;
use blixt::middleware::rate_limit::{RateLimiter, rate_limit_middleware};

let limiter = RateLimiter::default_limit();

let app = Router::new()
    .route("/api/data", get(handler))
    .layer(middleware::from_fn_with_state(limiter, rate_limit_middleware));
```

### Trusted proxies

By default, the rate limiter uses the direct connection IP from `ConnectInfo<SocketAddr>`. The `X-Forwarded-For` header is **ignored** unless the connection IP is in the trusted proxies list. This prevents spoofing.

```rust
use std::net::IpAddr;

let limiter = RateLimiter::default_limit()
    .with_trusted_proxies(vec![
        "10.0.0.1".parse::<IpAddr>().unwrap(),
        "10.0.0.2".parse::<IpAddr>().unwrap(),
    ]);
```

When the direct connection IP matches a trusted proxy, the first address in `X-Forwarded-For` is used as the client IP.

### Eviction

The rate limiter tracks up to 10,000 IPs by default. When this threshold is exceeded, entries that haven't been seen in over 2x the window duration are evicted. You can adjust the limit:

```rust
let limiter = RateLimiter::default_limit()
    .with_max_entries(50_000);
```

## Request ID

The `request_id` middleware generates a UUID v7 for each incoming request and:

- Creates a `tracing` span with `id = <uuid>`, so all log output within the request includes the ID.
- Sets the `x-request-id` response header, allowing clients and load balancers to correlate requests.

```rust
use axum::middleware;
use blixt::middleware::request_id::request_id;

let app = Router::new()
    .route("/health", get(handler))
    .layer(middleware::from_fn(request_id));
```

This is already included in the default `App` stack.

## Static file serving

`App::static_dir()` serves files from a directory at `/static/` with a one-year immutable cache header. A built-in dotfile guard blocks requests to paths containing `/..` or segments starting with `.`, returning 404 for path traversal attempts.

```rust
App::new(config)
    .router(routes)
    .static_dir("static")
    .serve()
    .await?;
```

## Custom middleware

Write your own middleware using Axum's `from_fn` pattern:

```rust
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;

async fn my_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // before the handler
    let response = next.run(request).await;
    // after the handler
    response
}
```

Apply it to a router:

```rust
use axum::middleware;

let app = Router::new()
    .route("/", get(handler))
    .layer(middleware::from_fn(my_middleware));
```

For middleware that needs shared state (like the rate limiter), use `from_fn_with_state`:

```rust
let app = Router::new()
    .route("/", get(handler))
    .layer(middleware::from_fn_with_state(my_state, stateful_middleware));
```

Layer ordering matters. Axum applies layers bottom-to-top: the last `.layer()` call runs first on the request path. Place authentication middleware closer to the routes (earlier in the chain) and infrastructure middleware (compression, tracing) further out.
