use axum::http::{HeaderName, HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use tracing::info_span;
use uuid::Uuid;

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Axum middleware that assigns a UUID v7 request ID to each request.
///
/// The ID is:
/// - Attached as a `tracing` span field for correlated log output
/// - Added as an `x-request-id` response header
///
/// Use with `axum::middleware::from_fn(request_id)`.
pub async fn request_id(request: Request<axum::body::Body>, next: Next) -> Response {
    let id: Uuid = Uuid::now_v7();
    let span = info_span!("request", id = %id);

    let mut response: Response = next.run(request).instrument(span).await;

    if let Ok(value) = HeaderValue::from_str(&id.to_string()) {
        response.headers_mut().insert(X_REQUEST_ID.clone(), value);
    }

    response
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
            .layer(axum::middleware::from_fn(request_id))
    }

    #[tokio::test]
    async fn adds_request_id_header() {
        let app: Router = test_router();

        let request: Request<Body> = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("failed to build request");

        let response: Response = app.oneshot(request).await.expect("failed to send request");

        let header_value: &HeaderValue = response
            .headers()
            .get("x-request-id")
            .expect("x-request-id header missing");

        let id_str: &str = header_value
            .to_str()
            .expect("header value is not valid UTF-8");

        let parsed: Uuid = Uuid::parse_str(id_str).expect("header value is not a valid UUID");

        assert_eq!(parsed.get_version(), Some(uuid::Version::SortRand));
    }

    #[tokio::test]
    async fn each_request_gets_unique_id() {
        let app: Router = test_router();

        let req1: Request<Body> = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("failed to build request");

        let resp1: Response = app
            .clone()
            .oneshot(req1)
            .await
            .expect("failed to send first request");

        let id1: String = resp1
            .headers()
            .get("x-request-id")
            .expect("missing header on first response")
            .to_str()
            .expect("invalid UTF-8")
            .to_string();

        let req2: Request<Body> = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("failed to build request");

        let resp2: Response = app
            .oneshot(req2)
            .await
            .expect("failed to send second request");

        let id2: String = resp2
            .headers()
            .get("x-request-id")
            .expect("missing header on second response")
            .to_str()
            .expect("invalid UTF-8")
            .to_string();

        assert_ne!(id1, id2, "each request must receive a unique ID");
    }
}
