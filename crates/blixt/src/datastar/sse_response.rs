use axum::response::{IntoResponse, Response};

use super::responses::{format_sse_event, into_sse_response};

/// Multi-event SSE response builder for composing Datastar actions.
///
/// Handlers frequently need to send multiple events in a single response —
/// for example, patching a DOM fragment *and* clearing form signals. The
/// builder concatenates events in insertion order and serializes them as a
/// single `text/event-stream` response.
///
/// # Example
///
/// ```rust,ignore
/// async fn create(
///     State(ctx): State<AppContext>,
///     signals: DatastarSignals,
/// ) -> Result<impl IntoResponse> {
///     let items = fetch_items(&ctx.db).await?;
///     Ok(SseResponse::new()
///         .patch(ItemListFragment { items })?
///         .signals(&serde_json::json!({"name": ""}))?)
/// }
/// ```
#[derive(Default)]
pub struct SseResponse {
    body: String,
}

impl SseResponse {
    /// Create an empty response builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a DOM element patch from a compiled Askama template.
    ///
    /// Renders the template and sends it as a `datastar-patch-elements` event.
    pub fn patch<T: askama::Template>(mut self, template: T) -> crate::error::Result<Self> {
        let html = template
            .render()
            .map_err(|err| crate::error::Error::Internal(err.to_string()))?;
        self.body.push_str(&format_sse_event(
            "datastar-patch-elements",
            "elements",
            &html,
        ));
        Ok(self)
    }

    /// Append a DOM element patch from a raw HTML string.
    ///
    /// Use this only with pre-sanitized HTML. Prefer [`Self::patch`] with
    /// Askama templates for automatic compile-time escaping.
    pub fn patch_html(mut self, html: &str) -> Self {
        self.body.push_str(&format_sse_event(
            "datastar-patch-elements",
            "elements",
            html,
        ));
        self
    }

    /// Append a signal patch from any serializable value.
    ///
    /// Serializes the value as JSON and sends it as a `datastar-patch-signals`
    /// event.
    pub fn signals<T: serde::Serialize>(mut self, data: &T) -> crate::error::Result<Self> {
        let json = serde_json::to_string(data)
            .map_err(|err| crate::error::Error::Internal(err.to_string()))?;
        self.body.push_str(&format_sse_event(
            "datastar-patch-signals",
            "signals",
            &json,
        ));
        Ok(self)
    }

    /// Returns true if no events have been added.
    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> Response {
        into_sse_response(self.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use axum::body::to_bytes;
    use axum::http::header;
    use serde_json::json;

    async fn response_body(resp: Response) -> String {
        let bytes = to_bytes(resp.into_body(), 1024 * 64)
            .await
            .expect("read body");
        String::from_utf8(bytes.to_vec()).expect("valid utf-8")
    }

    #[derive(Template)]
    #[template(source = "<div id=\"list\">{{ content }}</div>", ext = "html")]
    struct TestFragment<'a> {
        content: &'a str,
    }

    #[tokio::test]
    async fn empty_response_has_sse_headers() {
        let resp = SseResponse::new().into_response();
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("Content-Type")
            .to_str()
            .expect("valid str");
        assert!(ct.contains("text/event-stream"));
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("Cache-Control")
            .to_str()
            .expect("valid str");
        assert_eq!(cc, "no-cache");
    }

    #[tokio::test]
    async fn empty_response_has_empty_body() {
        let resp = SseResponse::new().into_response();
        let body = response_body(resp).await;
        assert!(body.is_empty());
    }

    #[test]
    fn is_empty_on_new() {
        assert!(SseResponse::new().is_empty());
    }

    #[test]
    fn is_not_empty_after_patch_html() {
        assert!(!SseResponse::new().patch_html("<p>hi</p>").is_empty());
    }

    #[tokio::test]
    async fn patch_renders_template() {
        let resp = SseResponse::new()
            .patch(TestFragment { content: "hello" })
            .expect("render")
            .into_response();
        let body = response_body(resp).await;
        assert!(body.contains("event: datastar-patch-elements\n"));
        assert!(body.contains("<div id=\"list\">hello</div>"));
    }

    #[tokio::test]
    async fn patch_html_sends_raw_html() {
        let resp = SseResponse::new()
            .patch_html("<span>raw</span>")
            .into_response();
        let body = response_body(resp).await;
        assert!(body.contains("event: datastar-patch-elements\n"));
        assert!(body.contains("<span>raw</span>"));
    }

    #[tokio::test]
    async fn signals_sends_json() {
        let resp = SseResponse::new()
            .signals(&json!({"title": "", "count": 0}))
            .expect("serialize")
            .into_response();
        let body = response_body(resp).await;
        assert!(body.contains("event: datastar-patch-signals\n"));
        assert!(body.contains("\"title\":\"\""));
        assert!(body.contains("\"count\":0"));
    }

    #[tokio::test]
    async fn combined_patch_and_signals() {
        let resp = SseResponse::new()
            .patch(TestFragment { content: "items" })
            .expect("render")
            .signals(&json!({"form": ""}))
            .expect("serialize")
            .into_response();
        let body = response_body(resp).await;

        assert!(body.contains("event: datastar-patch-elements\n"));
        assert!(body.contains("event: datastar-patch-signals\n"));

        let patch_pos = body.find("datastar-patch-elements").expect("patch event");
        let signals_pos = body.find("datastar-patch-signals").expect("signals event");
        assert!(patch_pos < signals_pos, "patch must precede signals");
    }

    #[tokio::test]
    async fn multiple_patches_preserve_order() {
        let resp = SseResponse::new()
            .patch_html("<div id=\"a\">first</div>")
            .patch_html("<div id=\"b\">second</div>")
            .into_response();
        let body = response_body(resp).await;

        let first = body.find("first").expect("first fragment");
        let second = body.find("second").expect("second fragment");
        assert!(first < second, "fragments must appear in insertion order");
    }

    #[tokio::test]
    async fn multiline_html_collapsed_to_single_line() {
        let html = "<div>\n  <p>inner</p>\n</div>";
        let resp = SseResponse::new().patch_html(html).into_response();
        let body = response_body(resp).await;
        assert!(body.contains("<div><p>inner</p></div>"));
    }

    #[tokio::test]
    async fn each_event_terminated_by_double_newline() {
        let resp = SseResponse::new()
            .patch_html("<p>a</p>")
            .signals(&json!({"x": 1}))
            .expect("serialize")
            .into_response();
        let body = response_body(resp).await;

        let events: Vec<&str> = body.split("\n\n").filter(|s| !s.is_empty()).collect();
        assert_eq!(events.len(), 2, "expected 2 events, got: {events:?}");
    }

    #[test]
    fn default_and_new_are_equivalent() {
        let a = SseResponse::new();
        let b = SseResponse::default();
        assert_eq!(a.body, b.body);
    }
}
