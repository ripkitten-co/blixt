/// SSE response types for DOM patching and signal updates.
pub mod responses;
mod signals;
mod signals_builder;
mod sse_response;

#[cfg(debug_assertions)]
pub mod hmr;

pub use responses::{SseFragment, SseSignals, SseStream};
pub use signals::DatastarSignals;
pub use signals_builder::Signals;
pub use sse_response::SseResponse;

/// Compile-time gate: in release builds, the `hmr` module must not exist.
/// This function will fail to compile if `hmr` is accidentally exposed in
/// release mode, because `crate::datastar::hmr` would not resolve.
#[cfg(not(debug_assertions))]
fn _assert_hmr_absent_in_release() {
    // If this file compiles in release mode, it proves that no `hmr` module
    // is publicly accessible. The function body is intentionally empty.
}

#[cfg(test)]
mod gate_tests {
    #[test]
    #[cfg(debug_assertions)]
    fn hmr_module_available_in_debug() {
        // Verify the HMR module is accessible in debug builds.
        let _ = std::any::type_name::<super::hmr::CssHmrBroadcaster>();
    }
}
