/// Axum extractors for deny-by-default authentication.
///
/// [`AuthUser`] rejects unauthenticated requests with 401. [`OptionalAuth`]
/// allows unauthenticated access but still decodes the token when present.
///
/// Tokens are read from the `blixt_auth` HttpOnly cookie first, falling back
/// to the `Authorization: Bearer` header. This supports both browser sessions
/// (cookie) and API clients (header).
///
/// The JWT secret is read from [`JwtSecret`] in request extensions. The
/// framework's middleware layer inserts this automatically from [`AppContext`].
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use super::cookie::AUTH_COOKIE_NAME;

/// Wrapper for the JWT signing secret, stored in request extensions.
///
/// Insert this into the request extensions (typically via middleware) so that
/// [`AuthUser`] and [`OptionalAuth`] extractors can validate tokens.
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// An authenticated user, extracted from a valid JWT.
///
/// Checks the `blixt_auth` cookie first, then the `Authorization: Bearer`
/// header. Using this extractor on a handler makes the route require
/// authentication; requests without a valid token receive a 401 response.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// The user ID from the JWT `sub` claim.
    pub user_id: String,
    /// The optional role from the JWT `role` claim.
    pub role: Option<String>,
}

/// An optionally-authenticated user.
///
/// Returns `OptionalAuth(None)` for unauthenticated requests instead of
/// rejecting them. Useful for pages that change behavior for logged-in users
/// but are also accessible anonymously.
#[derive(Debug, Clone)]
pub struct OptionalAuth(pub Option<AuthUser>);

/// Extracts a JWT from the auth cookie or Bearer header (cookie takes priority).
fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(token) = extract_cookie_token(parts) {
        return Some(token);
    }
    extract_bearer_token(parts).map(String::from)
}

/// Reads the JWT from the `blixt_auth` HttpOnly cookie.
fn extract_cookie_token(parts: &Parts) -> Option<String> {
    let cookie_header = parts
        .headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?;
    cookie_header.split(';').map(str::trim).find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name.trim() == AUTH_COOKIE_NAME {
            Some(value.trim().to_owned())
        } else {
            None
        }
    })
}

/// Extracts the Bearer token from the `Authorization` header.
fn extract_bearer_token(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Reads the JWT secret from request extensions.
fn jwt_secret_from_extensions(parts: &Parts) -> crate::error::Result<String> {
    parts
        .extensions
        .get::<JwtSecret>()
        .map(|s| s.0.clone())
        .ok_or(crate::error::Error::Internal(
            "JwtSecret missing from request extensions".into(),
        ))
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts).ok_or(crate::error::Error::Unauthorized)?;
        let secret = jwt_secret_from_extensions(parts)?;
        let claims = super::jwt::validate_token(&token, &secret)?;

        Ok(Self {
            user_id: claims.sub,
            role: claims.role,
        })
    }
}

impl<S> FromRequestParts<S> for OptionalAuth
where
    S: Send + Sync,
{
    type Rejection = crate::error::Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(token) = extract_token(parts) else {
            return Ok(Self(None));
        };

        let secret = match jwt_secret_from_extensions(parts) {
            Ok(secret) => secret,
            Err(_) => return Ok(Self(None)),
        };

        match super::jwt::validate_token(&token, &secret) {
            Ok(claims) => Ok(Self(Some(AuthUser {
                user_id: claims.sub,
                role: claims.role,
            }))),
            Err(_) => Ok(Self(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, header};

    const SECRET: &str = "test-secret-that-is-at-least-32-bytes-long!";

    fn build_request_with_bearer(token: Option<&str>) -> Request<()> {
        let mut builder = Request::builder().uri("/test");
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let mut request = builder.body(()).expect("build request");
        request
            .extensions_mut()
            .insert(JwtSecret(SECRET.to_string()));
        request
    }

    fn build_request_with_cookie(token: Option<&str>) -> Request<()> {
        let mut builder = Request::builder().uri("/test");
        if let Some(token) = token {
            builder = builder.header(header::COOKIE, format!("{AUTH_COOKIE_NAME}={token}"));
        }
        let mut request = builder.body(()).expect("build request");
        request
            .extensions_mut()
            .insert(JwtSecret(SECRET.to_string()));
        request
    }

    #[tokio::test]
    async fn missing_token_returns_401() {
        let request = build_request_with_bearer(None);
        let (mut parts, _body) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn valid_bearer_token_extracts_user() {
        let token = super::super::jwt::create_token("user-99", Some("editor"), SECRET, 3600)
            .expect("create token");
        let request = build_request_with_bearer(Some(&token));
        let (mut parts, _body) = request.into_parts();

        let user = AuthUser::from_request_parts(&mut parts, &())
            .await
            .expect("extract user");
        assert_eq!(user.user_id, "user-99");
        assert_eq!(user.role.as_deref(), Some("editor"));
    }

    #[tokio::test]
    async fn valid_cookie_token_extracts_user() {
        let token = super::super::jwt::create_token("user-55", Some("admin"), SECRET, 3600)
            .expect("create token");
        let request = build_request_with_cookie(Some(&token));
        let (mut parts, _body) = request.into_parts();

        let user = AuthUser::from_request_parts(&mut parts, &())
            .await
            .expect("extract user");
        assert_eq!(user.user_id, "user-55");
        assert_eq!(user.role.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn cookie_takes_priority_over_bearer() {
        let cookie_token = super::super::jwt::create_token("cookie-user", None, SECRET, 3600)
            .expect("create token");
        let bearer_token = super::super::jwt::create_token("bearer-user", None, SECRET, 3600)
            .expect("create token");
        let mut request = Request::builder()
            .uri("/test")
            .header(header::COOKIE, format!("{AUTH_COOKIE_NAME}={cookie_token}"))
            .header(header::AUTHORIZATION, format!("Bearer {bearer_token}"))
            .body(())
            .expect("build request");
        request
            .extensions_mut()
            .insert(JwtSecret(SECRET.to_string()));
        let (mut parts, _body) = request.into_parts();

        let user = AuthUser::from_request_parts(&mut parts, &())
            .await
            .expect("extract user");
        assert_eq!(user.user_id, "cookie-user");
    }

    #[tokio::test]
    async fn expired_token_returns_401() {
        let token =
            super::super::jwt::create_token("user-99", None, SECRET, 0).expect("create token");
        std::thread::sleep(std::time::Duration::from_secs(1));

        let request = build_request_with_bearer(Some(&token));
        let (mut parts, _body) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn optional_auth_without_token_returns_none() {
        let request = build_request_with_bearer(None);
        let (mut parts, _body) = request.into_parts();

        let opt = OptionalAuth::from_request_parts(&mut parts, &())
            .await
            .expect("optional auth");
        assert!(opt.0.is_none());
    }

    #[tokio::test]
    async fn optional_auth_reads_cookie() {
        let token =
            super::super::jwt::create_token("user-77", None, SECRET, 3600).expect("create token");
        let request = build_request_with_cookie(Some(&token));
        let (mut parts, _body) = request.into_parts();

        let opt = OptionalAuth::from_request_parts(&mut parts, &())
            .await
            .expect("optional auth");
        assert!(opt.0.is_some());
        assert_eq!(opt.0.unwrap().user_id, "user-77");
    }
}
