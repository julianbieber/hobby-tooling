//! The `hobby-struct-lsp` binary: serve the language server on stdin and stdout.

#[tokio::main]
async fn main() {
    struct_lsp::serve_stdio().await;
}
