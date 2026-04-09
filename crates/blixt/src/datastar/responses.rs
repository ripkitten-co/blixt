use axum::http::header;
use axum::response::sse::Sse;
use axum::response::{IntoResponse, Response};
use futures_core::Stream;
use std::pin::Pin;

/// Format a single SSE event for Datastar.
///
/// Datastar parses each `data:` line as `key value` pairs split by the
/// first space. For `datastar-patch-elements`, use `data_key` = `"elements"`.
/// For `datastar-patch-signals`, use `data_key` = `"signals"`.
///
/// Multi-line content is collapsed to a single line to prevent framing issues.
fn format_sse_event(event_type: &str, data_key: &str, data: &str) -> String {
    let oneline: String = data.trim().lines().map(str::trim).collect();
    format!("event: {event_type}\ndata: {data_key} {oneline}\n\n")
}

/// Single-shot SSE response that patches DOM elements via Datastar.
///
/// Renders an Askama template (or raw HTML) and sends it as a
/// `datastar-patch-elements` Server-Sent Event.
pub struct SseFragment {
    html: String,
}

impl SseFragment {
    /// Create a fragment from a compiled Askama template.
    pub fn new<T: askama::Template>(template: T) -> crate::error::Result<Self> {
        let html = template
            .render()
            .map_err(|err| crate::error::Error::Internal(err.to_string()))?;
        Ok(Self { html })
    }

    /// Create a fragment from a raw HTML string.
    pub fn from_html(html: String) -> Self {
        Self { html }
    }
}

impl IntoResponse for SseFragment {
    fn into_response(self) -> Response {
        let body = format_sse_event("datastar-patch-elements", "elements", &self.html);
        (
            [
                (header::CONTENT_TYPE, "text/event-stream"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            body,
        )
            .into_response()
    }
}

/// Single-shot SSE response that patches client-side signals via Datastar.
///
/// Serializes a value as JSON and sends it as a `datastar-patch-signals`
/// Server-Sent Event.
pub struct SseSignals {
    json: String,
}

impl SseSignals {
    /// Create a signals response from any serializable value.
    pub fn new<T: serde::Serialize>(data: &T) -> crate::error::Result<Self> {
        let json = serde_json::to_string(data)
            .map_err(|err| crate::error::Error::Internal(err.to_string()))?;
        Ok(Self { json })
    }
}

impl IntoResponse for SseSignals {
    fn into_response(self) -> Response {
        let body = format_sse_event("datastar-patch-signals", "signals", &self.json);
        (
            [
                (header::CONTENT_TYPE, "text/event-stream"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            body,
        )
            .into_response()
    }
}

type BoxedEventStream = Pin<
    Box<dyn Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>> + Send>,
>;

/// Streaming SSE response for sending multiple Datastar events over time.
///
/// Wraps Axum's [`Sse`] type with a boxed stream of SSE events.
pub struct SseStream {
    inner: Sse<BoxedEventStream>,
}

impl SseStream {
    /// Create a streaming SSE response from any compatible stream.
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>
            + Send
            + 'static,
    {
        Self {
            inner: Sse::new(Box::pin(stream)),
        }
    }
}

impl IntoResponse for SseStream {
    fn into_response(self) -> Response {
        self.inner.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::sse::Event;
    use serde_json::json;

    async fn response_body(resp: Response) -> String {
        let bytes = to_bytes(resp.into_body(), 1024 * 64)
            .await
            .expect("read body");
        String::from_utf8(bytes.to_vec()).expect("valid utf-8")
    }

    #[test]
    fn format_sse_event_single_line() {
        let result = format_sse_event("datastar-patch-elements", "elements", "<div>hello</div>");
        assert_eq!(
            result,
            "event: datastar-patch-elements\ndata: elements <div>hello</div>\n\n"
        );
    }

    #[test]
    fn format_sse_event_multiline_collapses_to_single_line() {
        let html = "<div>\n  <p>hi</p>\n</div>";
        let result = format_sse_event("datastar-patch-elements", "elements", html);
        assert_eq!(
            result,
            "event: datastar-patch-elements\ndata: elements <div><p>hi</p></div>\n\n"
        );
    }

    #[test]
    fn format_sse_event_signals() {
        let result = format_sse_event("datastar-patch-signals", "signals", r#"{"count":42}"#);
        assert_eq!(
            result,
            "event: datastar-patch-signals\ndata: signals {\"count\":42}\n\n"
        );
    }

    #[tokio::test]
    async fn sse_fragment_from_html_has_correct_content_type() {
        let fragment = SseFragment::from_html("<p>test</p>".to_owned());
        let resp = fragment.into_response();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("Content-Type header")
            .to_str()
            .expect("valid str");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream, got: {content_type}"
        );
    }

    #[tokio::test]
    async fn sse_fragment_from_html_has_cache_control() {
        let fragment = SseFragment::from_html("<p>test</p>".to_owned());
        let resp = fragment.into_response();
        let cache = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("Cache-Control header")
            .to_str()
            .expect("valid str");
        assert_eq!(cache, "no-cache");
    }

    #[tokio::test]
    async fn sse_fragment_from_html_produces_correct_body() {
        let fragment = SseFragment::from_html("<p>test</p>".to_owned());
        let resp = fragment.into_response();
        let body = response_body(resp).await;
        assert_eq!(
            body,
            "event: datastar-patch-elements\ndata: elements <p>test</p>\n\n"
        );
    }

    #[tokio::test]
    async fn sse_fragment_multiline_html() {
        let html = "<div>\n  <span>inner</span>\n</div>".to_owned();
        let fragment = SseFragment::from_html(html);
        let resp = fragment.into_response();
        let body = response_body(resp).await;
        assert_eq!(
            body,
            "event: datastar-patch-elements\ndata: elements <div><span>inner</span></div>\n\n"
        );
    }

    #[tokio::test]
    async fn sse_signals_produces_valid_sse() {
        let data = json!({"count": 42});
        let signals = SseSignals::new(&data).expect("serialize");
        let resp = signals.into_response();

        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("Content-Type header")
            .to_str()
            .expect("valid str");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream, got: {content_type}"
        );

        let body = response_body(resp).await;
        assert!(body.starts_with("event: datastar-patch-signals\n"));
        assert!(body.contains("data: signals "));
        assert!(body.contains("\"count\":42"));
    }

    #[tokio::test]
    async fn sse_signals_cache_control() {
        let data = json!({"ok": true});
        let signals = SseSignals::new(&data).expect("serialize");
        let resp = signals.into_response();
        let cache = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("Cache-Control header")
            .to_str()
            .expect("valid str");
        assert_eq!(cache, "no-cache");
    }

    #[tokio::test]
    async fn sse_stream_has_event_stream_content_type() {
        let stream =
            SingleEventStream::new(Event::default().event("datastar-patch-elements").data("hi"));
        let sse_stream = SseStream::new(stream);
        let resp = sse_stream.into_response();
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("Content-Type header")
            .to_str()
            .expect("valid str");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream, got: {content_type}"
        );
    }

    /// Minimal single-item stream for testing without external stream helpers.
    struct SingleEventStream {
        event: Option<Event>,
    }

    impl SingleEventStream {
        fn new(event: Event) -> Self {
            Self { event: Some(event) }
        }
    }

    impl Stream for SingleEventStream {
        type Item = Result<Event, std::convert::Infallible>;

        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(self.event.take().map(Ok))
        }
    }
}
