use axum::http::Request;
use axum::http::header::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

static CSP: HeaderName = HeaderName::from_static("content-security-policy");
static HSTS: HeaderName = HeaderName::from_static("strict-transport-security");
static XCTO: HeaderName = HeaderName::from_static("x-content-type-options");
static XFO: HeaderName = HeaderName::from_static("x-frame-options");
static RP: HeaderName = HeaderName::from_static("referrer-policy");
static PP: HeaderName = HeaderName::from_static("permissions-policy");

// Datastar requires 'unsafe-eval' for its expression engine (Function() constructor).
// This is a framework-level requirement, not optional.
static CSP_VALUE: HeaderValue = HeaderValue::from_static(
    "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self'; \
     img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'",
);
static HSTS_VALUE: HeaderValue =
    HeaderValue::from_static("max-age=63072000; includeSubDomains; preload");
static XCTO_VALUE: HeaderValue = HeaderValue::from_static("nosniff");
static XFO_VALUE: HeaderValue = HeaderValue::from_static("DENY");
static RP_VALUE: HeaderValue = HeaderValue::from_static("strict-origin-when-cross-origin");
static PP_VALUE: HeaderValue = HeaderValue::from_static("camera=(), microphone=(), geolocation=()");

/// Axum middleware that sets security headers on every response.
///
/// Sets Content-Security-Policy, Strict-Transport-Security,
/// X-Content-Type-Options, X-Frame-Options, Referrer-Policy,
/// and Permissions-Policy headers.
///
/// Use with `axum::middleware::from_fn(security_headers)`.
pub async fn security_headers(request: Request<axum::body::Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    insert_headers(response.headers_mut());
    response
}

/// Inserts all security headers into the provided header map.
fn insert_headers(headers: &mut axum::http::HeaderMap) {
    headers.insert(CSP.clone(), CSP_VALUE.clone());
    headers.insert(HSTS.clone(), HSTS_VALUE.clone());
    headers.insert(XCTO.clone(), XCTO_VALUE.clone());
    headers.insert(XFO.clone(), XFO_VALUE.clone());
    headers.insert(RP.clone(), RP_VALUE.clone());
    headers.insert(PP.clone(), PP_VALUE.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt;

    fn test_router() -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(security_headers))
    }

    #[tokio::test]
    async fn sets_all_security_headers() {
        let app = test_router();

        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("failed to build request");

        let response = app.oneshot(request).await.expect("failed to send request");

        let headers = response.headers();

        assert_eq!(
            headers.get("content-security-policy").expect("CSP missing"),
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self'; \
             img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'",
        );
        assert_eq!(
            headers
                .get("strict-transport-security")
                .expect("HSTS missing"),
            "max-age=63072000; includeSubDomains; preload",
        );
        assert_eq!(
            headers.get("x-content-type-options").expect("XCTO missing"),
            "nosniff",
        );
        assert_eq!(headers.get("x-frame-options").expect("XFO missing"), "DENY",);
        assert_eq!(
            headers.get("referrer-policy").expect("RP missing"),
            "strict-origin-when-cross-origin",
        );
        assert_eq!(
            headers.get("permissions-policy").expect("PP missing"),
            "camera=(), microphone=(), geolocation=()",
        );
    }
}
