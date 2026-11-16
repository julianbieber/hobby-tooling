//! Runs the claude-comment language server over stdio.

use claude_lsp::server::ClaudeLspServer;
use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(ClaudeLspServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
