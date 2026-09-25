//! `hobby-research-lsp`: a language server for `.hresearch` files whose one
//! capability is goto-definition on a link or a `[k]` ref, which opens the
//! link, or source `[k]`'s, in the browser.

mod link;

use std::collections::HashMap;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
}

impl Backend {
    async fn open_external(&self, url: Url) {
        let params = ShowDocumentParams {
            uri: url.clone(),
            external: Some(true),
            take_focus: Some(true),
            selection: None,
        };
        if matches!(self.client.show_document(params).await, Ok(true)) {
            return;
        }
        let opener = std::env::var("BROWSER").unwrap_or_else(|_| "xdg-open".to_string());
        let _ = std::process::Command::new(opener)
            .arg(url.as_str())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    fn text_of(&self, uri: &Url) -> Option<String> {
        if let Some(text) = self.documents.lock().unwrap().get(uri) {
            return Some(text.clone());
        }
        std::fs::read_to_string(uri.to_file_path().ok()?).ok()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "hobby-research-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let td = params.text_document;
        self.documents.lock().unwrap().insert(td.uri, td.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.documents
                .lock()
                .unwrap()
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let p = params.text_document_position_params;
        let url = self
            .text_of(&p.text_document.uri)
            .and_then(|text| link::target_at(&text, p.position.line, p.position.character))
            .and_then(|s| Url::parse(&s).ok());
        if let Some(url) = url {
            self.open_external(url).await;
        }
        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Mutex::new(HashMap::new()),
    });
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
