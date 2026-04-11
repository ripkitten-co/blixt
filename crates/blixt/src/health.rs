use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::db::DbPool;

/// Liveness probe. Returns `200 pong` with no external checks.
pub async fn ping() -> &'static str {
    "pong"
}

/// Readiness probe. Verifies database connectivity via `SELECT 1`.
pub async fn check(State(pool): State<Option<DbPool>>) -> Response {
    let Some(pool) = pool else {
        return (
            StatusCode::OK,
            [("content-type", "application/json")],
            r#"{"status":"ok","database":"not_configured"}"#,
        )
            .into_response();
    };

    match sqlx::query("SELECT 1").execute(&pool).await {
        Ok(_) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            r#"{"status":"ok","database":"connected"}"#,
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [("content-type", "application/json")],
            r#"{"status":"error","database":"unreachable"}"#,
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[tokio::test]
    async fn ping_returns_pong() {
        let app = Router::new().route("/_ping", get(ping));
        let request = Request::builder()
            .uri("/_ping")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"pong");
    }

    #[tokio::test]
    async fn health_without_db_returns_not_configured() {
        let app = Router::new()
            .route("/_health", get(check))
            .with_state(None::<DbPool>);
        let request = Request::builder()
            .uri("/_health")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["database"], "not_configured");
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn health_with_db_returns_connected() {
        use crate::config::{Config, Environment};
        use crate::db::create_pool;

        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 3000,
            blixt_env: Environment::Test,
            database_url: Some(secrecy::SecretString::from("sqlite::memory:".to_string())),
            jwt_secret: None,
        };
        let pool = create_pool(&config).await.expect("pool");

        let app = Router::new()
            .route("/_health", get(check))
            .with_state(Some(pool));
        let request = Request::builder()
            .uri("/_health")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["database"], "connected");
    }
}
