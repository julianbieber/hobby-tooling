//! The `hobby-just-mcp` binary: serves the MCP protocol on stdio, and logs to
//! stderr why serving failed.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();

    match hobby_just_mcp::server::serve_stdio().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "serving MCP failed");
            ExitCode::FAILURE
        }
    }
}
