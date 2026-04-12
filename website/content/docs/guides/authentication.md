+++
title = "Authentication"
weight = 6
description = "Auth scaffolding, cookie sessions, password reset, JWT tokens, and deny-by-default route protection."
+++

Blixt provides a complete authentication system. The fastest way to get started
is the auth scaffold generator, which creates everything you need: database
tables, models, controllers, and templates.

## Quick start with `blixt generate auth`

```bash
blixt generate auth
```

This creates:

- **Migrations**: `users` table (email, password_hash, role, reset tokens) and `sessions` table (token_hash, expires_at)
- **Models**: `User` (registration, login, password reset) and `Session` (create, revoke)
- **Controller**: 9 handlers covering register, login, logout, forgot password, and reset password
- **Templates**: form pages for each flow with CSRF protection

For new projects, use `blixt new my_app --auth` to include auth from the start.

After generating, add the auth routes printed by the CLI to your `src/main.rs`.

## How sessions work

Blixt uses JWT tokens stored in HttpOnly cookies, backed by a `sessions` database table for revocation:

1. On login/register, the server creates a JWT, SHA-256 hashes it, stores the hash in the `sessions` table, and sets a `blixt_auth` HttpOnly cookie
2. On each request, the `AuthUser` extractor validates the JWT signature, then checks the sessions table for an active matching session
3. On logout, the session row is deleted and the cookie is cleared. The JWT becomes immediately invalid

Cookie flags: `HttpOnly`, `SameSite=Strict`, `Path=/`, and `Secure` in production.

This gives you stateless JWT performance with database-backed revocation -- a logout takes effect immediately, not when the JWT expires.

## Password reset flow

The generated scaffold includes a full password reset flow:

1. User submits their email on `/forgot-password`
2. Server generates a UUID v4 token, SHA-256 hashes it, stores the hash with a 30-minute expiry on the user record
3. If a `Mailer` is configured, sends the reset link by email. Otherwise logs it to stdout (dev mode)
4. User clicks the link, lands on `/reset-password?token=...`, submits a new password
5. Server validates the token against the DB, updates the password, clears the token, and revokes all sessions

Set SMTP environment variables to enable email sending. Without them, reset links are printed to the console for local development.

## Route protection with extractors

### `AuthUser` -- deny by default

Adding `AuthUser` to a handler's parameters requires a valid session. The extractor checks the `blixt_auth` cookie first, then the `Authorization: Bearer` header.

```rust
use blixt::prelude::*;

async fn dashboard(user: AuthUser) -> Result<impl IntoResponse> {
    // user.user_id  -> from the JWT sub claim
    // user.role     -> from the JWT role claim
    render!(DashboardPage { user_id: user.user_id })
}
```

The extractor:
1. Reads the token from the `blixt_auth` cookie or `Authorization: Bearer` header
2. Validates the JWT signature and expiry
3. Checks the `sessions` table for an active session (when a DB pool is available)
4. Returns `AuthUser { user_id, role }` on success, or `401 Unauthorized`

### `OptionalAuth` -- graceful degradation

`OptionalAuth` allows unauthenticated access but still decodes the token when present:

```rust
async fn home(auth: OptionalAuth) -> Result<impl IntoResponse> {
    match auth.0 {
        Some(user) => render!(HomePage { name: Some(user.user_id) }),
        None => render!(HomePage { name: None }),
    }
}
```

Returns `OptionalAuth(None)` when the token is missing, invalid, expired, or the session has been revoked.

## Building blocks

If you prefer to build auth manually rather than using the scaffold, Blixt provides these primitives:

### Password hashing

```rust
use blixt::auth::password::{hash_password, verify_password};

let hash = hash_password("correct-horse-battery-staple")?;
let matches = verify_password("correct-horse-battery-staple", &hash)?;
```

Uses Argon2id with OWASP-recommended parameters. Returns PHC-format strings.

### JWT tokens

```rust
use blixt::auth::jwt::{create_token, validate_token};

let token = create_token("user-42", Some("admin"), &jwt_secret, 3600)?;
let claims = validate_token(&token, &jwt_secret)?;
```

Algorithm pinned to HS256. Secret must be at least 32 bytes.

### Session cookies

```rust
use blixt::prelude::*;

// After login
let mut response = Redirect::to("/").into_response();
auth_cookie::set(&mut response, &token, 86400, config.is_production());

// On logout
auth_cookie::clear(&mut response);
```

### Token hashing

```rust
use blixt::auth::sha256_hex;

let hash = sha256_hex(&token);
// Store hash in DB, not the raw token
```

## Security notes

- Algorithm pinned to HS256 at encode and decode. Blocks algorithm substitution attacks.
- Argon2id with OWASP defaults. Salts from OS CSPRNG.
- JWT secret minimum 32 bytes, enforced at creation time.
- Timing-safe credential verification: always runs argon2 even when user not found.
- Session tokens SHA-256 hashed before database storage.
- Password reset tokens are single-use, 30-minute expiry, hashed in DB.
- Forgot-password returns the same response regardless of email existence (prevents enumeration).
- HttpOnly + SameSite=Strict cookies prevent XSS and CSRF token theft.
- `AuthUser` is deny-by-default: no separate middleware needed.
