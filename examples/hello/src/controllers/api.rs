use askama::Template;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub uptime_secs: u64,
    pub timestamp: String,
}

static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn sse_fragments(html: &str) -> Response {
    let oneline = html.trim().lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("");
    let sse = format!("event: datastar-patch-elements\ndata: elements {oneline}\n\n");
    (
        [(header::CONTENT_TYPE, "text/event-stream"),
         (header::CACHE_CONTROL, "no-cache")],
        sse,
    ).into_response()
}

/// GET /api/status — raw JSON API
pub async fn status() -> impl IntoResponse {
    let start = START.get_or_init(std::time::Instant::now);
    axum::Json(StatusResponse {
        status: "ok".into(),
        uptime_secs: start.elapsed().as_secs(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// GET /fragments/time — SSE fragment for Datastar
#[derive(Template)]
#[template(path = "fragments/time.html")]
pub struct TimeFragment {
    pub time: String,
}

pub async fn time_fragment() -> Response {
    let html = TimeFragment {
        time: chrono::Utc::now().format("%H:%M:%S%.3f UTC").to_string(),
    }.render().unwrap_or_default();
    sse_fragments(&html)
}

/// GET /fragments/status — SSE fragment showing API data
#[derive(Template)]
#[template(path = "fragments/status.html")]
pub struct StatusFragment {
    pub status: String,
    pub uptime: u64,
    pub timestamp: String,
}

pub async fn status_fragment() -> Response {
    let start = START.get_or_init(std::time::Instant::now);
    let html = StatusFragment {
        status: "ok".into(),
        uptime: start.elapsed().as_secs(),
        timestamp: chrono::Utc::now().format("%H:%M:%S UTC").to_string(),
    }.render().unwrap_or_default();
    sse_fragments(&html)
}
