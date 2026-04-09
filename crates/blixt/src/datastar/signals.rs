use std::collections::HashMap;

use axum::extract::{FromRequest, Request};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

/// Maximum request body size for signal payloads (64kB).
const MAX_BODY_SIZE: usize = 64 * 1024;

/// Maximum number of signals allowed in a single request.
const MAX_SIGNAL_COUNT: usize = 100;

/// Maximum length of a signal key in characters.
const MAX_KEY_LENGTH: usize = 128;

/// Maximum size of an individual signal value when serialized (8kB).
const MAX_VALUE_SIZE: usize = 8 * 1024;

/// The key used by Datastar to nest signals in the request body or query string.
const DATASTAR_KEY: &str = "datastar";

/// Axum extractor that reads Datastar client-side signal state from HTTP requests.
///
/// Signals arrive as JSON — either in the request body (POST/PUT/DELETE/PATCH)
/// or as a `datastar` query parameter (GET). The extractor validates size limits,
/// signal count, key lengths, and value sizes before returning the parsed signals.
#[derive(Debug)]
pub struct DatastarSignals {
    inner: HashMap<String, Value>,
}

/// Rejection type for the `DatastarSignals` extractor.
#[derive(Debug)]
pub enum SignalRejection {
    BadRequest(String),
    PayloadTooLarge,
}

impl IntoResponse for SignalRejection {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            Self::PayloadTooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, "Payload too large").into_response()
            }
        }
    }
}

impl DatastarSignals {
    /// Get a typed value from the signals.
    ///
    /// Returns a `BadRequest` error if the key is missing or cannot be
    /// deserialized into the target type.
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> crate::error::Result<T> {
        let value = self
            .inner
            .get(key)
            .ok_or_else(|| crate::error::Error::BadRequest(format!("Missing signal: {key}")))?;
        serde_json::from_value(value.clone())
            .map_err(|_| crate::error::Error::BadRequest(format!("Invalid signal type: {key}")))
    }

    /// Get an optional typed value from the signals.
    ///
    /// Returns `Ok(None)` if the key does not exist. Returns an error only if
    /// the key exists but cannot be deserialized into the target type.
    pub fn get_opt<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> crate::error::Result<Option<T>> {
        match self.inner.get(key) {
            None => Ok(None),
            Some(value) => {
                let typed = serde_json::from_value(value.clone()).map_err(|_| {
                    crate::error::Error::BadRequest(format!("Invalid signal type: {key}"))
                })?;
                Ok(Some(typed))
            }
        }
    }

    /// Check if a signal exists.
    pub fn has(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Get all signal keys.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.inner.keys().map(String::as_str)
    }
}

impl<S: Send + Sync> FromRequest<S> for DatastarSignals {
    type Rejection = SignalRejection;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let method = req.method().clone();
        let raw = extract_raw_signals(&method, req).await?;
        let signals = parse_signals(&raw)?;
        validate_signals(&signals)?;
        Ok(DatastarSignals { inner: signals })
    }
}

/// Read the raw JSON string from the appropriate request source.
async fn extract_raw_signals(method: &Method, req: Request) -> Result<String, SignalRejection> {
    if *method == Method::GET {
        extract_from_query(req)
    } else {
        extract_from_body(req).await
    }
}

/// Extract the `datastar` query parameter value from the request URI.
fn extract_from_query(req: Request) -> Result<String, SignalRejection> {
    let query = req.uri().query().unwrap_or_default();
    parse_query_param(query)
}

/// Parse the `datastar` key from a raw query string.
fn parse_query_param(query: &str) -> Result<String, SignalRejection> {
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("datastar=") {
            let decoded = urlencoding_decode(value)
                .map_err(|_| bad("Invalid URL encoding in datastar parameter"))?;
            return Ok(decoded);
        }
    }
    Err(bad("Missing datastar query parameter"))
}

/// Minimal percent-decoding for the query parameter value.
fn urlencoding_decode(input: &str) -> Result<String, ()> {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(());
                }
                let hi = hex_digit(bytes[index + 1]).ok_or(())?;
                let lo = hex_digit(bytes[index + 2]).ok_or(())?;
                output.push(hi << 4 | lo);
                index += 3;
            }
            other => {
                output.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8(output).map_err(|_| ())
}

/// Convert an ASCII hex character to its nibble value.
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Read the request body up to `MAX_BODY_SIZE` and return it as a string.
async fn extract_from_body(req: Request) -> Result<String, SignalRejection> {
    let body = axum::body::to_bytes(req.into_body(), MAX_BODY_SIZE)
        .await
        .map_err(|_| SignalRejection::PayloadTooLarge)?;

    String::from_utf8(body.to_vec()).map_err(|_| bad("Request body is not valid UTF-8"))
}

/// Parse the raw JSON string into a signal map.
///
/// Accepts either a plain JSON object `{"key": value}` or a nested form
/// `{"datastar": {"key": value}}`.
fn parse_signals(raw: &str) -> Result<HashMap<String, Value>, SignalRejection> {
    let parsed: Value =
        serde_json::from_str(raw).map_err(|_| bad("Invalid JSON in signal payload"))?;

    let obj = parsed
        .as_object()
        .ok_or_else(|| bad("Signal payload must be a JSON object"))?;

    if let Some(nested) = obj.get(DATASTAR_KEY) {
        return nested
            .as_object()
            .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .ok_or_else(|| bad("datastar key must contain a JSON object"));
    }

    Ok(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Validate signal count, key lengths, and individual value sizes.
fn validate_signals(signals: &HashMap<String, Value>) -> Result<(), SignalRejection> {
    validate_signal_count(signals)?;
    validate_keys_and_values(signals)?;
    Ok(())
}

/// Ensure the number of signals does not exceed the limit.
fn validate_signal_count(signals: &HashMap<String, Value>) -> Result<(), SignalRejection> {
    if signals.len() > MAX_SIGNAL_COUNT {
        return Err(bad(&format!(
            "Too many signals: {} exceeds limit of {MAX_SIGNAL_COUNT}",
            signals.len()
        )));
    }
    Ok(())
}

/// Ensure every key is within length limits and every value is within size limits.
fn validate_keys_and_values(signals: &HashMap<String, Value>) -> Result<(), SignalRejection> {
    for (key, value) in signals {
        if key.len() > MAX_KEY_LENGTH {
            return Err(bad(&format!(
                "Signal key too long: {len} exceeds limit of {MAX_KEY_LENGTH}",
                len = key.len()
            )));
        }
        let serialized_len = serde_json::to_string(value).map(|s| s.len()).unwrap_or(0);
        if serialized_len > MAX_VALUE_SIZE {
            return Err(bad(&format!(
                "Signal value too large for key '{key}': {serialized_len} exceeds limit of {MAX_VALUE_SIZE}"
            )));
        }
    }
    Ok(())
}

/// Shorthand for constructing a `BadRequest` rejection.
fn bad(msg: &str) -> SignalRejection {
    SignalRejection::BadRequest(msg.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};

    /// Helper: build a POST request with a JSON body.
    fn post_request(body: &str) -> HttpRequest<Body> {
        HttpRequest::builder()
            .method(Method::POST)
            .uri("/test")
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .expect("failed to build request")
    }

    /// Helper: build a GET request with a query string.
    fn get_request(query: &str) -> HttpRequest<Body> {
        let uri = format!("/test?{query}");
        HttpRequest::builder()
            .method(Method::GET)
            .uri(&uri)
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Helper: extract `DatastarSignals` from a request.
    async fn extract_signals(req: HttpRequest<Body>) -> Result<DatastarSignals, Response> {
        DatastarSignals::from_request(Request::from(req), &())
            .await
            .map_err(|rejection| rejection.into_response())
    }

    #[tokio::test]
    async fn parse_signals_from_post_json_body() {
        let req = post_request(r#"{"search": "hello", "page": 1}"#);
        let signals = extract_signals(req).await.expect("should parse");

        let search: String = signals.get("search").expect("search signal");
        assert_eq!(search, "hello");

        let page: i64 = signals.get("page").expect("page signal");
        assert_eq!(page, 1);
    }

    #[tokio::test]
    async fn get_typed_string() {
        let req = post_request(r#"{"search": "hello"}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let val: String = signals.get("search").expect("get string");
        assert_eq!(val, "hello");
    }

    #[tokio::test]
    async fn get_typed_i64() {
        let req = post_request(r#"{"page": 42}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let val: i64 = signals.get("page").expect("get i64");
        assert_eq!(val, 42);
    }

    #[tokio::test]
    async fn get_missing_key_returns_bad_request() {
        let req = post_request(r#"{"search": "hello"}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let result: crate::error::Result<String> = signals.get("missing");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Missing signal: missing"), "got: {msg}");
    }

    #[tokio::test]
    async fn body_over_max_size_returns_error() {
        let oversized = "x".repeat(MAX_BODY_SIZE + 1);
        let body = format!(r#"{{"data": "{oversized}"}}"#);
        let req = post_request(&body);
        let result = extract_signals(req).await;
        assert!(result.is_err());

        let resp = result.unwrap_err();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn too_many_signals_returns_error() {
        let mut obj = serde_json::Map::new();
        for idx in 0..=MAX_SIGNAL_COUNT {
            obj.insert(format!("key_{idx}"), Value::from("value"));
        }
        let body = serde_json::to_string(&obj).expect("serialize");
        let req = post_request(&body);
        let result = extract_signals(req).await;
        assert!(result.is_err());

        let resp = result.unwrap_err();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn key_longer_than_limit_returns_error() {
        let long_key = "k".repeat(MAX_KEY_LENGTH + 1);
        let body = format!(r#"{{"{long_key}": "value"}}"#);
        let req = post_request(&body);
        let result = extract_signals(req).await;
        assert!(result.is_err());

        let resp = result.unwrap_err();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_opt_returns_none_for_missing_key() {
        let req = post_request(r#"{"search": "hello"}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let val: Option<String> = signals.get_opt("missing").expect("get_opt");
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn get_opt_returns_some_for_existing_key() {
        let req = post_request(r#"{"search": "hello"}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let val: Option<String> = signals.get_opt("search").expect("get_opt");
        assert_eq!(val, Some("hello".to_owned()));
    }

    #[tokio::test]
    async fn has_returns_true_for_existing_key() {
        let req = post_request(r#"{"search": "hello"}"#);
        let signals = extract_signals(req).await.expect("should parse");
        assert!(signals.has("search"));
    }

    #[tokio::test]
    async fn has_returns_false_for_missing_key() {
        let req = post_request(r#"{"search": "hello"}"#);
        let signals = extract_signals(req).await.expect("should parse");
        assert!(!signals.has("nope"));
    }

    #[tokio::test]
    async fn parse_signals_from_query_string() {
        let req = get_request("datastar=%7B%22search%22%3A%22hello%22%7D");
        let signals = extract_signals(req).await.expect("should parse");
        let val: String = signals.get("search").expect("get string");
        assert_eq!(val, "hello");
    }

    #[tokio::test]
    async fn nested_datastar_key_unwrapped() {
        let req = post_request(r#"{"datastar": {"name": "blixt"}}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let val: String = signals.get("name").expect("get name");
        assert_eq!(val, "blixt");
    }

    #[tokio::test]
    async fn invalid_json_returns_bad_request() {
        let req = post_request("not json");
        let result = extract_signals(req).await;
        assert!(result.is_err());
        let resp = result.unwrap_err();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn value_exceeding_max_size_returns_error() {
        let big_value = "x".repeat(MAX_VALUE_SIZE + 1);
        let body = format!(r#"{{"big": "{big_value}"}}"#);
        let req = post_request(&body);
        let result = extract_signals(req).await;
        // This will either fail at body size or value size validation
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn keys_iterator_returns_all_keys() {
        let req = post_request(r#"{"alpha": 1, "beta": 2}"#);
        let signals = extract_signals(req).await.expect("should parse");
        let mut keys: Vec<&str> = signals.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["alpha", "beta"]);
    }
}
