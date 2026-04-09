/// Axum extractors for deny-by-default authentication.
///
/// [`AuthUser`] rejects unauthenticated requests with 401. [`OptionalAuth`]
/// allows unauthenticated access but still decodes the token when present.
///
/// The JWT secret is read from [`JwtSecret`] in request extensions. The
/// framework's middleware layer inserts this automatically from [`AppContext`].
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

/// Wrapper for the JWT signing secret, stored in request extensions.
///
/// Insert this into the request extensions (typically via middleware) so that
/// [`AuthUser`] and [`OptionalAuth`] extractors can validate tokens.
#[derive(Clone)]
pub struct JwtSecret(pub String);

/// An authenticated user, extracted from a valid `Authorization: Bearer` header.
///
/// Using this extractor on a handler makes the route require authentication;
/// requests without a valid token receive a 401 response.
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
        let token = extract_bearer_token(parts).ok_or(crate::error::Error::Unauthorized)?;
        let secret = jwt_secret_from_extensions(parts)?;
        let claims = super::jwt::validate_token(token, &secret)?;

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
        let Some(token) = extract_bearer_token(parts) else {
            return Ok(Self(None));
        };

        let secret = match jwt_secret_from_extensions(parts) {
            Ok(secret) => secret,
            Err(_) => return Ok(Self(None)),
        };

        match super::jwt::validate_token(token, &secret) {
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

    fn build_request(token: Option<&str>) -> Request<()> {
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

    #[tokio::test]
    async fn missing_header_returns_401() {
        let request = build_request(None);
        let (mut parts, _body) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn valid_token_extracts_user() {
        let token = super::super::jwt::create_token("user-99", Some("editor"), SECRET, 3600)
            .expect("create token");
        let request = build_request(Some(&token));
        let (mut parts, _body) = request.into_parts();

        let user = AuthUser::from_request_parts(&mut parts, &())
            .await
            .expect("extract user");
        assert_eq!(user.user_id, "user-99");
        assert_eq!(user.role.as_deref(), Some("editor"));
    }

    #[tokio::test]
    async fn expired_token_returns_401() {
        let token =
            super::super::jwt::create_token("user-99", None, SECRET, 0).expect("create token");
        std::thread::sleep(std::time::Duration::from_secs(1));

        let request = build_request(Some(&token));
        let (mut parts, _body) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn optional_auth_without_header_returns_none() {
        let request = build_request(None);
        let (mut parts, _body) = request.into_parts();

        let opt = OptionalAuth::from_request_parts(&mut parts, &())
            .await
            .expect("optional auth");
        assert!(opt.0.is_none());
    }
}
