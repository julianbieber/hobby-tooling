//! The language server: answers completion, symbol, definition and reference
//! requests from an index of the workspace kept current as files change.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::definition;
use crate::document_symbol;
use crate::fuzzy;
use crate::indexer::{self, Index, IndexEntry, Indexer};
use crate::languages::{LanguageInfo, LanguageRegistry};
use crate::references::{self, Reference};
use crate::worktree;

const COMPLETION_LIMIT: usize = 200;
const SYMBOL_LIMIT: usize = 500;

pub struct Backend {
    client: Client,
    state: Arc<State>,
}

struct State {
    indexer: Indexer,
    index: RwLock<Index>,
    task_root: RwLock<Option<PathBuf>>,
    /// Client-opened documents. Keyed by file URI → full text. We use `FULL`
    /// sync so each `did_change` replaces the whole buffer.
    docs: RwLock<HashMap<Url, String>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let registry = match LanguageRegistry::load() {
            Ok(r) => r,
            Err(e) => {
                let error: &dyn std::error::Error = &e;
                tracing::error!(error, "helix languages could not be loaded");
                // Empty registry still lets the server run; it just won't
                // index anything.
                LanguageRegistry::default_empty()
            }
        };
        let indexer = Indexer::new(registry);
        Self {
            client,
            state: Arc::new(State {
                indexer,
                index: RwLock::new(Index::default()),
                task_root: RwLock::new(None),
                docs: RwLock::new(HashMap::new()),
            }),
        }
    }

    fn pick_root(params: &InitializeParams) -> Option<PathBuf> {
        let from_folders = params
            .workspace_folders
            .as_ref()
            .and_then(|f| f.first())
            .and_then(|f| f.uri.to_file_path().ok());
        if let Some(p) = from_folders {
            return Some(p);
        }
        if let Some(uri) = params.root_uri.as_ref().and_then(|u| u.to_file_path().ok()) {
            return Some(uri);
        }
        #[expect(deprecated, reason = "the oldest clients send only rootPath")]
        if let Some(p) = params.root_path.clone().map(PathBuf::from) {
            return Some(p);
        }
        std::env::current_dir().ok()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = Self::pick_root(&params).unwrap_or_else(|| PathBuf::from("."));
        let task = worktree::detect_task_root(&root);
        tracing::info!(
            client_root = %root.display(),
            task_root = %task.display(),
            "initialize"
        );
        *self.state.task_root.write().await = Some(task);

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: None,
                    ..Default::default()
                }),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "treesitter-class-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let task_root = {
            let g = self.state.task_root.read().await;
            g.clone()
        };
        let Some(root) = task_root else {
            tracing::warn!("initialized without a task root; nothing to index");
            return;
        };
        let state = Arc::clone(&self.state);
        let walk_root = root.clone();
        let new_index = run_blocking(move || state.indexer.index_root(&walk_root)).await;
        let count = new_index.entries().len();
        *self.state.index.write().await = new_index;
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "treesitter-class-lsp: indexed {count} classes under {}",
                    root.display()
                ),
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        {
            let mut docs = self.state.docs.write().await;
            docs.insert(uri.clone(), params.text_document.text);
        }
        self.reindex_file(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // FULL sync: exactly one change event with the new full text.
        if let Some(change) = params.content_changes.into_iter().next() {
            let mut docs = self.state.docs.write().await;
            docs.insert(uri.clone(), change.text);
        }
        self.reindex_file(&uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.reindex_file(&params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.state.docs.write().await;
        docs.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let prefix = {
            let docs = self.state.docs.read().await;
            docs.get(&uri)
                .and_then(|text| word_prefix_at(text, pos))
                .unwrap_or_default()
        };

        let index = self.state.index.read().await;
        let completable = index.entries().iter().filter(|e| is_completable(e));
        let ranked = fuzzy::rank(completable, &prefix, COMPLETION_LIMIT);

        let items: Vec<CompletionItem> = ranked
            .into_iter()
            .map(|e| CompletionItem {
                label: e.name.clone(),
                kind: Some(symbol_kind_to_completion_kind(e.kind)),
                detail: Some(detail_for(e)),
                ..Default::default()
            })
            .collect();

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(text) = self.document_text(&uri).await else {
            return Ok(None);
        };
        let Some(name) = definition::identifier_at(&text, pos) else {
            return Ok(None);
        };
        let language = self.language_of(&uri);
        let path = uri.to_file_path().ok();
        let index = self.state.index.read().await;
        let container = definition::receiver_at(&text, pos).and_then(|receiver| {
            definition::receiver_type(&text, pos, &receiver, path.as_deref(), index.entries())
        });
        let locations: Vec<Location> = definition::lookup(
            index.entries(),
            &name,
            language.as_deref(),
            container.as_deref(),
        )
        .into_iter()
        .filter_map(|e| {
            Some(Location {
                uri: e.uri()?,
                range: e.name_range,
            })
        })
        .collect();
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let Some(text) = self.document_text(&uri).await else {
            return Ok(None);
        };
        let Some(name) = definition::identifier_at(&text, pos) else {
            return Ok(None);
        };
        let Some(root) = self.state.task_root.read().await.clone() else {
            return Ok(None);
        };
        let language = self.language_of(&uri);

        let declarations: Vec<(PathBuf, Range)> = if params.context.include_declaration {
            Vec::new()
        } else {
            let index = self.state.index.read().await;
            index
                .entries()
                .iter()
                .filter(|e| e.name == name)
                .map(|e| (e.file.clone(), e.name_range))
                .collect()
        };

        let state = Arc::clone(&self.state);
        let found = run_blocking(move || {
            let needle = name.as_bytes();
            let mut found = Vec::new();
            for path in indexer::source_files(&root) {
                let Some(lang) = state.indexer.registry().resolve(&path) else {
                    continue;
                };
                let Ok(source) = std::fs::read(&path) else {
                    continue;
                };
                if source.contains(&0u8) || !source.windows(needle.len()).any(|w| w == needle) {
                    continue;
                }
                let Some(tree) = state.indexer.parse(lang, &source) else {
                    continue;
                };
                for range in references::occurrences_in(&tree, &source, &name) {
                    found.push(Reference {
                        file: path.clone(),
                        language: lang.name.clone(),
                        range,
                    });
                }
            }
            found
        })
        .await;

        let found = if params.context.include_declaration {
            found
        } else {
            references::without_declarations(found, &declarations)
        };
        let locations: Vec<Location> = references::order(found, language.as_deref())
            .into_iter()
            .filter_map(|r| {
                Some(Location {
                    uri: Url::from_file_path(&r.file).ok()?,
                    range: r.range,
                })
            })
            .collect();
        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Ok(path) = params.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let Some(lang) = self.state.indexer.registry().resolve(&path).cloned() else {
            return Ok(None);
        };
        let entries = self.index_file(&path, lang).await;
        let symbols = document_symbol::tree(&entries);
        self.state.index.write().await.replace_file(&path, entries);
        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        }
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let index = self.state.index.read().await;
        let ranked = fuzzy::rank(index.entries(), &params.query, SYMBOL_LIMIT);
        let syms: Vec<SymbolInformation> = ranked
            .into_iter()
            .filter_map(|e| {
                let uri = e.uri()?;
                #[expect(
                    deprecated,
                    reason = "the protocol's deprecated field still has to be set"
                )]
                let symbol = SymbolInformation {
                    name: e.name.clone(),
                    kind: e.kind,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri,
                        range: e.range,
                    },
                    container_name: Some(e.language.clone()),
                };
                Some(symbol)
            })
            .collect();
        if syms.is_empty() {
            Ok(None)
        } else {
            Ok(Some(syms))
        }
    }
}

impl Backend {
    async fn reindex_file(&self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let Some(lang) = self.state.indexer.registry().resolve(&path).cloned() else {
            return;
        };
        let entries = self.index_file(&path, lang).await;
        self.state.index.write().await.replace_file(&path, entries);
    }

    async fn index_file(&self, path: &std::path::Path, lang: LanguageInfo) -> Vec<IndexEntry> {
        let state = Arc::clone(&self.state);
        let path = path.to_path_buf();
        run_blocking(move || match state.indexer.index_file(&path, &lang) {
            Ok(entries) => entries,
            Err(e) => {
                let error: &dyn std::error::Error = &e;
                tracing::warn!(path = %path.display(), error, "file not indexed");
                Vec::new()
            }
        })
        .await
    }

    async fn document_text(&self, uri: &Url) -> Option<String> {
        if let Some(text) = self.state.docs.read().await.get(uri) {
            return Some(text.clone());
        }
        tokio::fs::read_to_string(uri.to_file_path().ok()?)
            .await
            .ok()
    }

    fn language_of(&self, uri: &Url) -> Option<String> {
        let path = uri.to_file_path().ok()?;
        let info = self.state.indexer.registry().resolve(&path)?;
        Some(info.name.clone())
    }
}

async fn run_blocking<T, F>(work: F) -> T
where
    T: Default + Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(value) => value,
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "blocking task did not finish");
            T::default()
        }
    }
}

/// Simple word-prefix extractor: walks backwards from `pos` over identifier
/// characters (alphanumeric + `_`).
fn word_prefix_at(text: &str, pos: Position) -> Option<String> {
    let line = definition::line_at(text, pos.line)?;
    let head = line.get(..definition::byte_col_at(line, pos.character)?)?;
    let start = head
        .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_')
        .len();
    head.get(start..).map(str::to_string)
}

fn is_completable(e: &IndexEntry) -> bool {
    e.kind != SymbolKind::FUNCTION && e.kind != SymbolKind::METHOD
}

fn symbol_kind_to_completion_kind(k: SymbolKind) -> CompletionItemKind {
    match k {
        SymbolKind::CLASS => CompletionItemKind::CLASS,
        SymbolKind::STRUCT => CompletionItemKind::STRUCT,
        SymbolKind::INTERFACE => CompletionItemKind::INTERFACE,
        SymbolKind::ENUM => CompletionItemKind::ENUM,
        SymbolKind::TYPE_PARAMETER => CompletionItemKind::TYPE_PARAMETER,
        _ => CompletionItemKind::CLASS,
    }
}

fn detail_for(e: &IndexEntry) -> String {
    let file = e.file.file_name().and_then(|n| n.to_str()).unwrap_or("");
    format!("{} · {}", e.language, file)
}
