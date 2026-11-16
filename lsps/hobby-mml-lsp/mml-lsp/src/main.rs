//! `hobby-mml-lsp` binary: the HMML v3 language server on stdio.

#[tokio::main]
async fn main() {
    hobby_mml_lsp::serve_stdio().await;
}
