use std::path::Path;

use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

use claude_skills_lsp::index::resolve_config_dir;
use claude_skills_lsp::server::Backend;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let config_dir = std::env::var_os("HOME").map(|home| {
        resolve_config_dir(
            Path::new(&home),
            std::env::var_os("CLAUDE_CONFIG_DIR").as_deref(),
        )
    });
    tracing::debug!(dir = ?config_dir, "claude config directory");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|_client| Backend::new(config_dir));
    Server::new(stdin, stdout, socket).serve(service).await;
}
