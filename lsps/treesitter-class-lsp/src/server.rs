use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::definition;
use crate::fuzzy;
use crate::indexer::{Index, IndexEntry, Indexer};
use crate::languages::LanguageRegistry;
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
                tracing::error!("failed to load helix languages.toml: {e}");
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

    async fn pick_root(&self, params: &InitializeParams) -> Option<PathBuf> {
        let from_folders = params
            .workspace_folders
            .as_ref()
            .and_then(|f| f.first())
            .and_then(|f| f.uri.to_file_path().ok());
        if let Some(p) = from_folders {
            return Some(p);
        }
        #[allow(deprecated)]
        if let Some(uri) = params.root_uri.as_ref().and_then(|u| u.to_file_path().ok()) {
            return Some(uri);
        }
        #[allow(deprecated)]
        if let Some(p) = params.root_path.clone().map(PathBuf::from) {
            return Some(p);
        }
        std::env::current_dir().ok()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = self.pick_root(&params).await.unwrap_or_else(|| PathBuf::from("."));
        let task = worktree::detect_task_root(&root);
        tracing::info!(
            "initialize: client root={}, task root={}",
            root.display(),
            task.display()
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
        let client = self.client.clone();
        // Run the initial walk on a blocking thread so we don't stall the
        // async runtime on large workspaces.
        tokio::task::spawn_blocking(move || {
            let new_index = state.indexer.index_root(&root);
            let count = new_index.entries().len();
            let mut guard = futures_executor_block_on(state.index.write());
            *guard = new_index;
            drop(guard);
            let _ = futures_executor_block_on(client.log_message(
                MessageType::INFO,
                format!("treesitter-class-lsp: indexed {count} classes under {}", root.display()),
            ));
        });
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
            docs.get(&uri).and_then(|text| word_prefix_at(text, pos)).unwrap_or_default()
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

        let open_text = self.state.docs.read().await.get(&uri).cloned();
        let text = match open_text {
            Some(t) => t,
            None => match uri.to_file_path().ok().and_then(|p| std::fs::read_to_string(p).ok()) {
                Some(t) => t,
                None => return Ok(None),
            },
        };
        let Some(name) = definition::identifier_at(&text, pos) else {
            return Ok(None);
        };

        let index = self.state.index.read().await;
        let locations: Vec<Location> = definition::lookup(index.entries(), &name)
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
                #[allow(deprecated)]
                Some(SymbolInformation {
                    name: e.name.clone(),
                    kind: e.kind,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri,
                        range: e.range,
                    },
                    container_name: Some(e.language.clone()),
                })
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
        let lang = match self.state.indexer.registry().resolve(&path) {
            Some(l) => l.clone(),
            None => return,
        };
        // Parse from disk — we use FULL sync but the client's unsaved buffer
        // isn't guaranteed to match what's on disk. For MVP we prefer the
        // on-disk representation; saves will reconcile quickly.
        let indexer = unsafe { extend_lifetime_ref(&self.state.indexer) };
        let path_cloned = path.clone();
        let lang_cloned = lang.clone();
        let entries = tokio::task::spawn_blocking(move || {
            indexer.index_file(&path_cloned, &lang_cloned).unwrap_or_default()
        })
        .await
        .unwrap_or_default();

        let mut index = self.state.index.write().await;
        index.replace_file(&path, entries);
    }
}

/// Simple word-prefix extractor: walks backwards from `pos` over identifier
/// characters (alphanumeric + `_`).
fn word_prefix_at(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let byte_col = definition::char_col_to_byte_col(line, pos.character as usize)?;
    let prefix: String = line[..byte_col]
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(prefix)
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
    let file = e
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    format!("{} · {}", e.language, file)
}

/// SAFETY: we only extend the lifetime for the duration of a single
/// `spawn_blocking` whose output we await immediately below; `state` lives
/// inside `Arc<State>` held by `self` and therefore outlives the blocking
/// task.
unsafe fn extend_lifetime_ref<T>(r: &T) -> &'static T {
    unsafe { &*(r as *const T) }
}

/// Block on a future from a synchronous (spawn_blocking) context without
/// creating a nested runtime. `tokio::task::spawn_blocking` doesn't provide a
/// clean way to call back into async code without a handle, so we use the
/// current handle.
fn futures_executor_block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Handle::current().block_on(fut)
}
