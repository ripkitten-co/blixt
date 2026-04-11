+++
title = "Authentication"
weight = 6
description = "JWT token management, Argon2id password hashing, and deny-by-default route protection."
+++

Blixt provides three authentication building blocks: password hashing with
Argon2id, JWT creation and validation with algorithm pinning, and Axum
extractors that enforce deny-by-default access control.

## Password hashing

The `blixt::auth::password` module uses Argon2id with OWASP-recommended default
parameters. Salts are generated from the OS CSPRNG.

```rust
use blixt::auth::password::{hash_password, verify_password};

// During registration
let hash = hash_password("correct-horse-battery-staple")?;
// hash is a PHC-format string: $argon2id$v=19$m=19456,t=2,p=1$...

// During login
let matches = verify_password("correct-horse-battery-staple", &hash)?;
assert!(matches);
```

`hash_password` returns a PHC-format string that includes the algorithm,
parameters, salt, and hash. Store this string directly in your database.

`verify_password` returns `Ok(true)` on match, `Ok(false)` on mismatch, and
`Err` only if the stored hash is malformed.

## JWT tokens

The `blixt::auth::jwt` module creates and validates JWTs with the algorithm
hardcoded to HS256. The validation explicitly restricts the accepted algorithm
set to `[HS256]`, preventing `none`-algorithm attacks and key-confusion attacks.

### Creating tokens

```rust
use blixt::auth::jwt::create_token;

let token = create_token(
    "user-42",          // sub claim (user ID)
    Some("admin"),      // optional role claim
    &jwt_secret,        // HMAC secret (min 32 bytes)
    3600,               // TTL in seconds
)?;
```

The secret must be at least 32 bytes. Shorter secrets are rejected with an
error.

### Validating tokens

```rust
use blixt::auth::jwt::validate_token;

let claims = validate_token(&token, &jwt_secret)?;
// claims.sub   -> "user-42"
// claims.exp   -> Unix timestamp
// claims.iat   -> Unix timestamp
// claims.role  -> Some("admin")
```

Validation rejects tokens that:
- Use any algorithm other than HS256
- Are expired (with zero leeway)
- Have an invalid signature

Invalid tokens return `Error::Unauthorized`.

### Claims struct

```rust
pub struct Claims {
    pub sub: String,          // user ID
    pub exp: usize,           // expiry (Unix seconds)
    pub iat: usize,           // issued-at (Unix seconds)
    pub role: Option<String>, // optional role
}
```

## JWT secret configuration

The JWT secret is loaded from the `JWT_SECRET` environment variable and stored
as a `SecretString` in the `Config` struct. It is automatically redacted in
debug output.

```bash
# .env
JWT_SECRET=your-secret-at-least-32-bytes-long
```

Access it in handlers via the config:

```rust
use secrecy::ExposeSecret;

let secret = ctx.config.jwt_secret()
    .ok_or(Error::Internal("JWT_SECRET not configured".into()))?;
```

The framework middleware automatically inserts a `JwtSecret` into request
extensions from `AppContext`, so the auth extractors can find it without manual
wiring.

## Route protection with extractors

### `AuthUser` -- deny by default

Adding `AuthUser` to a handler's parameters makes the route require a valid
`Authorization: Bearer <token>` header. Requests without a valid token receive
`401 Unauthorized`.

```rust
use blixt::prelude::*;

async fn dashboard(user: AuthUser) -> Result<impl IntoResponse> {
    // user.user_id  -> from the JWT sub claim
    // user.role     -> from the JWT role claim
    render!(DashboardPage {
        user_id: user.user_id,
    })
}
```

The extractor:
1. Reads the `Authorization: Bearer <token>` header
2. Retrieves the `JwtSecret` from request extensions
3. Validates the token via `validate_token`
4. Returns `AuthUser { user_id, role }` on success

### `OptionalAuth` -- graceful degradation

`OptionalAuth` allows unauthenticated access but still decodes the token when
present. It wraps `Option<AuthUser>`:

```rust
use blixt::prelude::*;

async fn home(auth: OptionalAuth) -> Result<impl IntoResponse> {
    match auth.0 {
        Some(user) => render!(HomePage { name: Some(user.user_id) }),
        None => render!(HomePage { name: None }),
    }
}
```

`OptionalAuth` never rejects a request. If the token is missing, invalid, or
expired, it returns `OptionalAuth(None)`.

## Full example: login and protected route

```rust
use blixt::prelude::*;
use blixt::auth::jwt::create_token;
use blixt::auth::password::{hash_password, verify_password};

pub fn routes() -> Router<AppContext> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct RegisterForm {
    email: String,
    password: String,
}

async fn register(
    State(ctx): State<AppContext>,
    form: Form<RegisterForm>,
) -> Result<impl IntoResponse> {
    let data = form.into_inner();
    let hash = hash_password(&data.password)?;

    query!(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2)",
        &data.email, &hash
    )
    .execute(&ctx.db)
    .await?;

    Ok(Redirect::to("/login")
        .with_flash(Flash::success("Account created")))
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
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

    let secret = ctx.config.jwt_secret()
        .ok_or(Error::Internal("JWT_SECRET not configured".into()))?;

    let token = create_token(&user.id.to_string(), None, secret, 86400)?;

    Ok(axum::Json(TokenResponse { token }))
}

async fn me(user: AuthUser) -> Result<impl IntoResponse> {
    Ok(format!("Hello, user {}", user.user_id))
}
```

## Security notes

- The algorithm is pinned to HS256 at both encoding and decoding time. The
  validation explicitly sets `algorithms = vec![HS256]`, blocking algorithm
  substitution attacks.
- Password hashing uses Argon2id with the `argon2` crate's defaults, which
  follow OWASP recommendations. Salts come from `OsRng`.
- The JWT secret minimum length of 32 bytes is enforced at token creation time.
- `AuthUser` is deny-by-default: adding it to a handler automatically rejects
  unauthenticated requests. You do not need to add separate middleware.
