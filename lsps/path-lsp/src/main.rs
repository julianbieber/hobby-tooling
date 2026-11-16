//! The `path_lsp` binary: serve the language server on stdin and stdout.

#[tokio::main]
async fn main() {
    path_lsp::serve_stdio().await;
}
