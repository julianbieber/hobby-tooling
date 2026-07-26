use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

use claude_skills_lsp::server::Backend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs go to stderr; stdout is reserved for the LSP JSON-RPC stream.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
