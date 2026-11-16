//! The `hobby-lsp-mcp` binary: serves MCP on stdio, with logs on stderr
//! because stdout carries the protocol.
//!
//! The optional first argument is the fallback workspace root; it defaults to
//! the current directory.

use std::path::PathBuf;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let workspace_root = match std::env::args_os().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => match std::env::current_dir() {
            Ok(dir) => dir,
            Err(e) => return fail(&hobby_lsp_mcp::LspMcpError::WorkspaceRoot(e)),
        },
    };

    match hobby_lsp_mcp::serve_stdio(workspace_root).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn fail(e: &hobby_lsp_mcp::LspMcpError) -> ExitCode {
    let error: &dyn std::error::Error = e;
    tracing::error!(
        error,
        error_class = std::any::type_name_of_val(e),
        "LSP-MCP server stopped"
    );
    ExitCode::FAILURE
}
