//! The LSP backend: maintains open documents and a cached skill/command index,
//! and answers completion, goto-definition, and hover for slash-command /
//! skill references.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::index::{Entry, Index, Kind, fuzzy_rank};
use crate::words::{RefToken, ref_at};

/// How long a built index is served before the next request rebuilds it. Keeps
/// hot requests cheap while picking up newly added skills within a few seconds.
const INDEX_TTL: Duration = Duration::from_secs(5);

struct CachedIndex {
    index: Index,
    built_at: Instant,
}

pub struct Backend {
    #[allow(dead_code)]
    client: Client,
    /// The Claude config directory — `$CLAUDE_CONFIG_DIR`, else `$HOME/.claude` —
    /// captured at construction. Anchors the user and plugin tiers.
    config_dir: OnceLock<PathBuf>,
    /// Workspace roots announced at `initialize` — the project tier.
    workspace_roots: RwLock<Vec<PathBuf>>,
    documents: Mutex<HashMap<Url, String>>,
    cache: RwLock<Option<CachedIndex>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        // The one place the account is chosen. `hcdev` exports CLAUDE_CONFIG_DIR
        // before launching zellij, so a Helix pane in a hobby session — and this
        // server under it — indexes ~/.claude-hobby; a plain work session gets
        // ~/.claude. See `resolve_config_dir`.
        let config_dir = OnceLock::new();
        if let Some(h) = std::env::var_os("HOME") {
            let resolved = crate::index::resolve_config_dir(
                Path::new(&h),
                std::env::var_os("CLAUDE_CONFIG_DIR").as_deref(),
            );
            tracing::debug!(dir = ?resolved, "claude config directory");
            let _ = config_dir.set(resolved);
        }
        Self {
            client,
            config_dir,
            workspace_roots: RwLock::new(Vec::new()),
            documents: Mutex::new(HashMap::new()),
            cache: RwLock::new(None),
        }
    }

    fn config_dir(&self) -> Option<&Path> {
        self.config_dir.get().map(PathBuf::as_path)
    }

    /// Rebuild the index from scratch and store it in the cache.
    fn rebuild(&self) {
        let roots = self.workspace_roots.read().unwrap().clone();
        let index = Index::build(self.config_dir(), &roots);
        tracing::debug!(entries = index.len(), "rebuilt index");
        *self.cache.write().unwrap() = Some(CachedIndex {
            index,
            built_at: Instant::now(),
        });
    }

    /// Run `f` against a fresh-enough index, rebuilding first if the cache is
    /// empty or older than [`INDEX_TTL`].
    fn with_index<T>(&self, f: impl FnOnce(&Index) -> T) -> T {
        let stale = {
            let guard = self.cache.read().unwrap();
            match guard.as_ref() {
                Some(c) => c.built_at.elapsed() > INDEX_TTL,
                None => true,
            }
        };
        if stale {
            self.rebuild();
        }
        let guard = self.cache.read().unwrap();
        f(&guard.as_ref().expect("cache populated above").index)
    }

    /// A document is in scope iff it is the Ctrl+G prompt buffer
    /// (`claude-prompt-*`) or lives under some `.claude/` directory. Everything
    /// else — ordinary markdown — is left alone.
    fn is_target(uri: &Url) -> bool {
        let Ok(path) = uri.to_file_path() else {
            return false;
        };
        let is_prompt_buffer = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("claude-prompt-"))
            .unwrap_or(false);
        let under_claude = path
            .components()
            .any(|c| c.as_os_str() == std::ffi::OsStr::new(".claude"));
        is_prompt_buffer || under_claude
    }

    fn doc_text(&self, uri: &Url) -> Option<String> {
        self.documents.lock().unwrap().get(uri).cloned()
    }

    /// The reference token under the cursor, read from the in-memory document
    /// (falling back to disk if the editor never sent it).
    fn token_at(&self, uri: &Url, pos: Position) -> Option<RefToken> {
        let text = self.doc_text(uri).or_else(|| {
            uri.to_file_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
        })?;
        ref_at(&text, pos)
    }
}

/// One-line description plus a `kind · tier` tag, for the completion menu.
fn detail_for(entry: &Entry) -> String {
    let tier = entry.tier.label();
    let kind = entry.kind.label();
    let first = entry.description.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        format!("{kind} · {tier}")
    } else {
        format!("{first} · {kind} · {tier}")
    }
}

fn documentation_for(entry: &Entry) -> Documentation {
    let mut body = String::new();
    if let Some(plugin) = &entry.plugin {
        body.push_str(&format!("**{}** _(plugin: {})_\n\n", entry.name, plugin));
    } else {
        body.push_str(&format!("**{}**\n\n", entry.name));
    }
    if !entry.description.is_empty() {
        body.push_str(&entry.description);
        body.push_str("\n\n");
    }
    body.push_str(&format!("`{}`", entry.source.display()));
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: body,
    })
}

fn completion_item(entry: &Entry, replace: Range) -> CompletionItem {
    let kind = match entry.kind {
        Kind::Command => CompletionItemKind::FUNCTION,
        Kind::Skill => CompletionItemKind::MODULE,
    };
    CompletionItem {
        label: entry.name.clone(),
        kind: Some(kind),
        detail: Some(detail_for(entry)),
        documentation: Some(documentation_for(entry)),
        // Filter on the bare name (the sigil is outside the replace range).
        filter_text: Some(entry.name.clone()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: replace,
            new_text: entry.name.clone(),
        })),
        ..Default::default()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(folders) = params.workspace_folders.as_ref() {
            for f in folders {
                if let Ok(p) = f.uri.to_file_path() {
                    roots.push(p);
                }
            }
        }
        if roots.is_empty() {
            #[allow(deprecated)]
            if let Some(root) = params.root_uri.as_ref().and_then(|u| u.to_file_path().ok()) {
                roots.push(root);
            }
        }
        *self.workspace_roots.write().unwrap() = roots;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec!["/".to_string(), ":".to_string()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "claude_skills_lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.rebuild();
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.documents
                .lock()
                .unwrap()
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Saving a command/skill source is the common "I just edited a command"
        // case — refresh immediately instead of waiting out the TTL.
        if let Ok(path) = params.text_document.uri.to_file_path() {
            let touches_index = path.components().any(|c| {
                let s = c.as_os_str();
                s == std::ffi::OsStr::new("commands") || s == std::ffi::OsStr::new("skills")
            }) && path
                .components()
                .any(|c| c.as_os_str() == std::ffi::OsStr::new(".claude"));
            if touches_index {
                self.rebuild();
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        if !Self::is_target(&uri) {
            return Ok(None);
        }
        let pos = params.text_document_position.position;
        let Some(token) = self.token_at(&uri, pos) else {
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        };

        let items = self.with_index(|index| {
            fuzzy_rank(index.names(), &token.query)
                .into_iter()
                .filter_map(|name| index.get(name))
                .map(|entry| completion_item(entry, token.replace))
                .collect::<Vec<_>>()
        });

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        if !Self::is_target(&uri) {
            return Ok(None);
        }
        let pos = params.text_document_position_params.position;
        let Some(token) = self.token_at(&uri, pos) else {
            return Ok(None);
        };
        if token.query.is_empty() {
            return Ok(None);
        }

        // Only resolve a token that matches a known name — this is what keeps a
        // unix path like `/home` from being treated as a `/home` reference.
        let locations = self.with_index(|index| {
            index
                .locations(&token.query)
                .into_iter()
                .filter_map(|e| {
                    Url::from_file_path(&e.source).ok().map(|uri| Location {
                        uri,
                        range: Range::default(),
                    })
                })
                .collect::<Vec<_>>()
        });

        match locations.len() {
            0 => Ok(None),
            1 => Ok(Some(GotoDefinitionResponse::Scalar(
                locations.into_iter().next().unwrap(),
            ))),
            _ => Ok(Some(GotoDefinitionResponse::Array(locations))),
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        if !Self::is_target(&uri) {
            return Ok(None);
        }
        let pos = params.text_document_position_params.position;
        let Some(token) = self.token_at(&uri, pos) else {
            return Ok(None);
        };
        if token.query.is_empty() {
            return Ok(None);
        }

        let markdown = self.with_index(|index| {
            index.get(&token.query).map(|entry| {
                let tier = entry.tier.label();
                let kind = entry.kind.label();
                let mut body = format!("**{}**  ({} · {})\n\n", entry.name, kind, tier);
                if !entry.description.is_empty() {
                    body.push_str(&entry.description);
                    body.push_str("\n\n");
                }
                body.push_str(&format!("`{}`", entry.source.display()));
                body
            })
        });

        Ok(markdown.map(|value| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(path: &str) -> Url {
        Url::from_file_path(path).unwrap()
    }

    #[test]
    fn prompt_buffer_is_target() {
        assert!(Backend::is_target(&uri(
            "/tmp/claude-1000/claude-prompt-abc123.md"
        )));
    }

    #[test]
    fn claude_dir_file_is_target() {
        assert!(Backend::is_target(&uri(
            "/home/u/.claude/commands/julian/mml-extract.md"
        )));
        assert!(Backend::is_target(&uri(
            "/home/u/projects/repo/.claude/skills/x/SKILL.md"
        )));
    }

    #[test]
    fn ordinary_markdown_is_not_target() {
        assert!(!Backend::is_target(&uri("/home/u/notes/todo.md")));
        assert!(!Backend::is_target(&uri("/tmp/claude-1000/other.md")));
    }
}
