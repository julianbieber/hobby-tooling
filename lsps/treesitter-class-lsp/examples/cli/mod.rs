//! How the example binaries set up their logging and report a failure: both go
//! to stderr through `tracing`, leaving stdout to the answer.

use std::error::Error;
use std::process::ExitCode;

/// Logs to stderr at `default_filter`, unless `RUST_LOG` names another filter.
pub fn init_logging(default_filter: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter)),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
}

/// Logs `error` with its sources and returns the failure code.
pub fn report(error: &(dyn Error + 'static)) -> ExitCode {
    tracing::error!(error, "failed");
    ExitCode::FAILURE
}
