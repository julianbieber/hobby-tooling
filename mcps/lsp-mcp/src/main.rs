use anyhow::Result;
use hobby_lsp_mcp::{config, lsp, LspMcpServer};
use rmcp::{transport::stdio, ServiceExt};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Load Helix language server configuration
    let config = config::load_helix_config()?;
    let helix_config_path = config::helix_config_path()?;

    // Determine workspace root (use current directory or passed argument)
    let workspace_root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("Failed to get current directory"));

    // Startup diagnostics must go to stderr: this server speaks MCP JSON-RPC
    // over stdout, and any non-JSON line there corrupts the client's stream.
    eprintln!(
        "LSP-MCP Server starting with workspace: {}",
        workspace_root.display()
    );
    eprintln!("Helix config: {}", helix_config_path.display());
    eprintln!(
        "Loaded {} language servers from Helix config",
        config.language_servers.len()
    );

    // Create LSP manager
    let lsp_manager = Arc::new(lsp::LspManager::new(config, workspace_root));

    // Create and start the MCP server with stdio transport
    let service = LspMcpServer::new(lsp_manager).serve(stdio()).await?;

    // Wait for the server to complete
    service.waiting().await?;

    Ok(())
}
