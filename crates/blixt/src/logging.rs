use tracing_subscriber::{EnvFilter, fmt};

use crate::error::Result;

/// Initializes the tracing subscriber with format determined by `BLIXT_ENV`.
///
/// - `BLIXT_ENV=production` uses JSON output for structured log ingestion.
/// - All other values (default: `development`) use pretty human-readable output.
/// - Log level is controlled by `RUST_LOG` (default: `info`).
pub fn init_tracing() -> Result<()> {
    let env_filter: EnvFilter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let blixt_env: String =
        std::env::var("BLIXT_ENV").unwrap_or_else(|_| "development".to_string());

    let is_production: bool = blixt_env == "production";

    if is_production {
        fmt::Subscriber::builder()
            .with_env_filter(env_filter)
            .json()
            .try_init()
            .map_err(|err| crate::error::Error::Internal(err.to_string()))?;
    } else {
        fmt::Subscriber::builder()
            .with_env_filter(env_filter)
            .pretty()
            .try_init()
            .map_err(|err| crate::error::Error::Internal(err.to_string()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_tracing_returns_ok_on_first_call() {
        // Note: tracing subscriber can only be set once per process.
        // This test verifies the function succeeds, but subsequent calls
        // in the same test binary will fail (expected behavior).
        let result: Result<()> = init_tracing();
        assert!(result.is_ok());
    }
}
