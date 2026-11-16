//! The `hobby-research-lsp` binary: serve the language server on stdin and stdout.

#[tokio::main]
async fn main() {
    hobby_research_lsp::serve_stdio().await;
}
