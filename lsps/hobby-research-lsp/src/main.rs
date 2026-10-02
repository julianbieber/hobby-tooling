//! `hobby-research-lsp`: a language server with two capabilities —
//! goto-definition on a link or a `[k]` ref in a `.hresearch` file, which
//! opens the link, or source `[k]`'s, in the browser; and workspace symbols
//! over every project's research cards, in any buffer it is attached to.

mod link;
mod symbols;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
    root: Mutex<Option<PathBuf>>,
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
    #[allow(deprecated)]
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = params
            .workspace_folders
            .and_then(|folders| folders.into_iter().next().map(|f| f.uri))
            .or(params.root_uri)
            .and_then(|uri| uri.to_file_path().ok());
        *self.root.lock().unwrap() = root;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
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

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let root = self.root.lock().unwrap().clone();
        let found =
            tokio::task::spawn_blocking(move || symbols::query(root.as_deref(), &params.query))
                .await;
        Ok(Some(found.unwrap_or_default()))
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
        root: Mutex::new(None),
    });
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
