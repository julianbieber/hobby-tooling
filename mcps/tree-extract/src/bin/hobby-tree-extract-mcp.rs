use std::process::ExitCode;

use rmcp::{ServiceExt, transport::stdio};

use hobby_tree_extract::mcp::TreeExtractServer;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let service = match TreeExtractServer::new().serve(stdio()).await {
        Ok(service) => service,
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "MCP initialisation failed");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = service.waiting().await {
        let error: &dyn std::error::Error = &e;
        tracing::error!(error, "MCP server stopped");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
