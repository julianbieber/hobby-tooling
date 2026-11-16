//! An MCP server that exposes language server capabilities, configured from
//! Helix's `languages.toml`.

pub mod config;
pub mod error;
pub mod fuzzy;
pub mod lsp;
pub mod server;
pub mod tools;

pub use error::{LspMcpError, Result, error_chain};
pub use server::LspMcpServer;

use rmcp::ServiceExt;
use std::path::PathBuf;
use std::sync::Arc;

/// Load the Helix configuration and serve MCP on stdin/stdout until the
/// client disconnects. Language servers are rooted at the project of each
/// queried file, falling back to `workspace_root`.
pub async fn serve_stdio(workspace_root: PathBuf) -> Result<()> {
    let helix_config_path = config::helix_config_path()?;
    let config = config::load_config_from_file(&helix_config_path)?;

    tracing::info!(
        workspace = %workspace_root.display(),
        helix_config = %helix_config_path.display(),
        language_servers = config.language_servers.len(),
        "LSP-MCP server starting"
    );

    let lsp_manager = Arc::new(lsp::LspManager::new(config, workspace_root));
    let service = LspMcpServer::new(lsp_manager)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| LspMcpError::Initialize(Box::new(e)))?;
    service.waiting().await?;
    Ok(())
}
