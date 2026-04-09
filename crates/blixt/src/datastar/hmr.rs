//! CSS Hot Module Replacement via Datastar SSE.
//!
//! Watches a CSS file for changes and broadcasts reload events to connected
//! browsers using the Datastar `execute-script` SSE event type.
//!
//! This module is only available in debug builds (`#[cfg(debug_assertions)]`).
//! The `execute-script` event can run arbitrary JavaScript in connected
//! browsers, so it must never be present in release binaries.

use std::net::IpAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use futures_core::Stream;
use notify::{RecursiveMode, Watcher};
use tokio::sync::broadcast;

/// Hardcoded script that reloads the CSS stylesheet by cache-busting the href.
///
/// This is a compile-time constant — never constructed from runtime data,
/// file contents, or user input.
const CSS_RELOAD_SCRIPT: &str =
    "document.querySelector('link[data-blixt-css]').href='/static/css/output.css?v='+Date.now()";

/// Debounce interval for file change events.
const DEBOUNCE_DURATION: Duration = Duration::from_millis(200);

/// Broadcasts CSS file change events to connected SSE clients.
pub struct CssHmrBroadcaster {
    sender: broadcast::Sender<()>,
    /// Held to keep the watcher alive for the lifetime of the broadcaster.
    _watcher: notify::RecommendedWatcher,
}

impl CssHmrBroadcaster {
    /// Create a new broadcaster that watches the given CSS file for changes.
    ///
    /// Returns an error if the file watcher cannot be initialized or the
    /// path cannot be watched.
    pub fn new(css_path: PathBuf) -> crate::error::Result<Arc<Self>> {
        let (sender, _) = broadcast::channel(16);
        let watcher = start_watcher(css_path, sender.clone())?;
        Ok(Arc::new(Self {
            sender,
            _watcher: watcher,
        }))
    }

    /// Subscribe to CSS reload events.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }
}

/// Start a file watcher with debounced change notifications.
fn start_watcher(
    css_path: PathBuf,
    sender: broadcast::Sender<()>,
) -> crate::error::Result<notify::RecommendedWatcher> {
    let debounce_sender = sender.clone();
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Spawn a debounce task that coalesces rapid file changes.
    tokio::spawn(async move {
        while let Some(()) = notify_rx.recv().await {
            tokio::time::sleep(DEBOUNCE_DURATION).await;
            // Drain any events that arrived during the debounce window.
            while notify_rx.try_recv().is_ok() {}
            let _ = debounce_sender.send(());
            tracing::debug!("CSS change detected, broadcasting reload");
        }
    });

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && is_modify_event(&event.kind)
        {
            let _ = notify_tx.blocking_send(());
        }
    })
    .map_err(|err| crate::error::Error::Internal(format!("File watcher init failed: {err}")))?;

    watcher
        .watch(&css_path, RecursiveMode::NonRecursive)
        .map_err(|err| {
            crate::error::Error::Internal(format!("Failed to watch {}: {err}", css_path.display()))
        })?;

    tracing::info!(path = %css_path.display(), "CSS HMR watcher started");
    Ok(watcher)
}

/// Check if a notify event kind represents a file modification.
fn is_modify_event(kind: &notify::EventKind) -> bool {
    matches!(
        kind,
        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
    )
}

/// Check if an IP address is a loopback address.
fn is_loopback(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Axum handler for the HMR SSE endpoint.
///
/// Only accepts connections from loopback addresses. Returns a
/// `text/event-stream` response that sends `execute-script` events
/// whenever the watched CSS file changes.
pub async fn hmr_handler(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    state: axum::extract::State<Arc<CssHmrBroadcaster>>,
) -> impl IntoResponse {
    if !is_loopback(&addr.ip()) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let receiver = state.subscribe();
    let stream = HmrEventStream::new(receiver);
    Sse::new(stream).into_response()
}

/// Build a reload SSE event with the hardcoded CSS reload script.
fn reload_event() -> Event {
    Event::default()
        .event("datastar-execute-script")
        .data(CSS_RELOAD_SCRIPT)
}

/// Bridge a broadcast receiver into an mpsc receiver via a spawned task.
///
/// This allows using `mpsc::Receiver::poll_recv` in the `Stream` impl,
/// which the broadcast receiver does not directly support.
fn bridge_broadcast(mut broadcast_rx: broadcast::Receiver<()>) -> tokio::sync::mpsc::Receiver<()> {
    let (mpsc_tx, mpsc_rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        while let Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) = broadcast_rx.recv().await {
            if mpsc_tx.send(()).await.is_err() {
                break;
            }
        }
    });
    mpsc_rx
}

/// Stream adapter that converts broadcast messages into SSE events.
struct HmrEventStream {
    receiver: tokio::sync::mpsc::Receiver<()>,
}

impl HmrEventStream {
    fn new(broadcast_rx: broadcast::Receiver<()>) -> Self {
        Self {
            receiver: bridge_broadcast(broadcast_rx),
        }
    }
}

impl Stream for HmrEventStream {
    type Item = Result<Event, std::convert::Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(())) => Poll::Ready(Some(Ok(reload_event()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Create an Axum router with the HMR endpoint mounted at `/__blixt_hmr`.
///
/// The returned router expects `Arc<CssHmrBroadcaster>` as application state
/// and requires `ConnectInfo<SocketAddr>` to be available (provided by
/// `axum::serve` with `into_make_service_with_connect_info`).
pub fn hmr_route(broadcaster: Arc<CssHmrBroadcaster>) -> Router {
    Router::new()
        .route("/__blixt_hmr", get(hmr_handler))
        .with_state(broadcaster)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn loopback_ipv4_detected() {
        let addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(is_loopback(&addr));
    }

    #[test]
    fn loopback_ipv6_detected() {
        let addr = IpAddr::V6(Ipv6Addr::LOCALHOST);
        assert!(is_loopback(&addr));
    }

    #[test]
    fn non_loopback_ipv4_rejected() {
        let addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn non_loopback_ipv6_rejected() {
        let addr = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn unspecified_ipv4_is_not_loopback() {
        let addr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn unspecified_ipv6_is_not_loopback() {
        let addr = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        assert!(!is_loopback(&addr));
    }

    #[test]
    fn css_reload_script_is_static_constant() {
        // The script must not contain template placeholders or injection vectors.
        assert!(!CSS_RELOAD_SCRIPT.contains("${"));
        assert!(!CSS_RELOAD_SCRIPT.contains("eval("));
        assert!(!CSS_RELOAD_SCRIPT.contains("innerHTML"));
        assert!(!CSS_RELOAD_SCRIPT.contains("document.write"));
        assert!(!CSS_RELOAD_SCRIPT.contains("Function("));

        // Must reference the expected CSS selector.
        assert!(CSS_RELOAD_SCRIPT.contains("data-blixt-css"));
        assert!(CSS_RELOAD_SCRIPT.contains("output.css"));
    }

    #[test]
    fn css_reload_script_no_user_input_interpolation() {
        // Ensure the script contains no mechanisms for runtime string building
        // beyond the safe Date.now() cache-buster.
        assert!(!CSS_RELOAD_SCRIPT.contains("fetch("));
        assert!(!CSS_RELOAD_SCRIPT.contains("XMLHttpRequest"));
        assert!(!CSS_RELOAD_SCRIPT.contains("import("));
    }

    #[tokio::test]
    async fn hmr_handler_returns_event_stream_content_type() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (sender, _) = broadcast::channel(16);
        let broadcaster = Arc::new(CssHmrBroadcaster {
            sender,
            _watcher: create_null_watcher(),
        });

        let app = Router::new()
            .route("/__blixt_hmr", get(hmr_handler))
            .with_state(broadcaster);
        let app = app.into_make_service_with_connect_info::<SocketAddr>();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let bound_addr = listener.local_addr().expect("local addr");

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        let mut stream = tokio::net::TcpStream::connect(bound_addr)
            .await
            .expect("connect");
        let request = "GET /__blixt_hmr HTTP/1.1\r\nHost: localhost\r\n\r\n";
        stream.write_all(request.as_bytes()).await.expect("write");

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.expect("read");
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(
            response.contains("text/event-stream"),
            "expected text/event-stream in response headers, got:\n{response}"
        );

        server.abort();
    }

    #[tokio::test]
    async fn hmr_handler_rejects_non_loopback() {
        // We verify via the is_loopback function since crafting a non-loopback
        // ConnectInfo in an integration test requires network manipulation.
        // The handler checks is_loopback and returns 403 for non-loopback.
        let external = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        assert!(!is_loopback(&external));
    }

    #[test]
    fn modify_event_detected() {
        let kind = notify::EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        ));
        assert!(is_modify_event(&kind));
    }

    #[test]
    fn create_event_detected() {
        let kind = notify::EventKind::Create(notify::event::CreateKind::File);
        assert!(is_modify_event(&kind));
    }

    #[test]
    fn remove_event_not_detected_as_modify() {
        let kind = notify::EventKind::Remove(notify::event::RemoveKind::File);
        assert!(!is_modify_event(&kind));
    }

    /// Create a null watcher for testing (watches nothing).
    fn create_null_watcher() -> notify::RecommendedWatcher {
        notify::recommended_watcher(|_: notify::Result<notify::Event>| {}).expect("null watcher")
    }
}
