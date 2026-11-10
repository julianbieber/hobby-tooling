//! `hobby-mml-lsp`: a `tower-lsp` language server for HMML v3 deltas, built on
//! the `hobby-mml-parser` crate.
//!
//! This file is protocol glue only: it caches a parsed [`DocState`] per open
//! document, publishes diagnostics on open and change, and turns the results
//! of [`analysis`] into `lsp_types` values.

mod analysis;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use analysis::{Definition, ItemKind, Sym, SymKind};
use mml_parser::git::{GitCache, check_links, home_dir};
use mml_parser::span::Span;
use mml_parser::{Document, LineIndex, Severity, analyze};

struct DocState {
    text: String,
    doc: Document,
    line_index: LineIndex,
}

impl DocState {
    fn range(&self, span: Span) -> Range {
        range_of(&self.line_index, span)
    }

    fn offset(&self, pos: Position) -> usize {
        analysis::offset_of(
            &self.text,
            mml_parser::Position {
                line: pos.line,
                character: pos.character,
            },
        )
    }
}

fn range_of(li: &LineIndex, span: Span) -> Range {
    let p = |o| {
        let p = li.position(o);
        Position {
            line: p.line,
            character: p.character,
        }
    };
    Range {
        start: p(span.start),
        end: p(span.end),
    }
}

struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, DocState>>,
    git: Arc<Mutex<GitCache>>,
    home: PathBuf,
    cache_dir: PathBuf,
}

fn cache_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".cache"))
        .join("hobby-mml-lsp")
}

impl Backend {
    fn new(client: Client) -> Backend {
        let home = home_dir().unwrap_or_default();
        Backend {
            client,
            documents: Mutex::new(HashMap::new()),
            git: Arc::new(Mutex::new(GitCache::new())),
            cache_dir: cache_dir(&home),
            home,
        }
    }

    async fn refresh(&self, uri: Url, text: String, version: Option<i32>) {
        let git = self.git.clone();
        let home = self.home.clone();
        let file = uri.to_file_path().ok();
        let computed = tokio::task::spawn_blocking(move || {
            let (doc, mut all) = analyze(&text);
            if let Some(file) = file {
                let mut cache = git.lock().unwrap();
                all.extend(check_links(&mut cache, &doc, &file, &home));
                all.sort_by_key(|d| (d.span.start, d.span.end));
            }
            let line_index = LineIndex::new(&text);
            let diags: Vec<Diagnostic> = all
                .iter()
                .map(|d| to_lsp_diagnostic(&line_index, d))
                .collect();
            (
                DocState {
                    text,
                    doc,
                    line_index,
                },
                diags,
            )
        })
        .await;
        let Ok((ds, diags)) = computed else { return };
        self.documents.lock().unwrap().insert(uri.clone(), ds);
        self.client.publish_diagnostics(uri, diags, version).await;
    }

    fn definition(&self, uri: &Url, pos: Position) -> Option<GotoDefinitionResponse> {
        let docs = self.documents.lock().unwrap();
        let ds = docs.get(uri)?;
        let offset = ds.offset(pos);
        match analysis::definition(&ds.doc, &ds.text, offset)? {
            Definition::Local(span) => Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: ds.range(span),
            })),
            Definition::Link(link, side) => {
                let delta = uri.to_file_path().ok()?;
                let mut cache = self.git.lock().unwrap();
                let (file, line) = analysis::link_target(
                    &mut cache,
                    &ds.doc,
                    &delta,
                    &self.home,
                    &self.cache_dir,
                    &link,
                    side,
                )?;
                let at = Position { line, character: 0 };
                Some(GotoDefinitionResponse::Scalar(Location {
                    uri: Url::from_file_path(file).ok()?,
                    range: Range { start: at, end: at },
                }))
            }
        }
    }
}

fn to_lsp_diagnostic(li: &LineIndex, d: &mml_parser::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: range_of(li, d.span),
        severity: Some(match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
        }),
        code: Some(NumberOrString::String(d.code.to_string())),
        source: Some("hmml".to_string()),
        message: d.message.clone(),
        ..Default::default()
    }
}

fn symbol_kind(k: SymKind) -> SymbolKind {
    match k {
        SymKind::Namespace => SymbolKind::NAMESPACE,
        SymKind::Unit => SymbolKind::FUNCTION,
        SymKind::Anchor => SymbolKind::KEY,
        SymKind::Struct => SymbolKind::STRUCT,
        SymKind::Enum => SymbolKind::ENUM,
        SymKind::Field => SymbolKind::FIELD,
        SymKind::Variant => SymbolKind::ENUM_MEMBER,
        SymKind::Module => SymbolKind::MODULE,
        SymKind::Function => SymbolKind::METHOD,
        SymKind::Test => SymbolKind::EVENT,
        SymKind::Text => SymbolKind::STRING,
    }
}

#[allow(deprecated)]
fn to_document_symbol(ds: &DocState, s: &Sym) -> DocumentSymbol {
    DocumentSymbol {
        name: if s.name.is_empty() {
            "?".into()
        } else {
            s.name.clone()
        },
        detail: s.detail.clone(),
        kind: symbol_kind(s.kind),
        tags: None,
        deprecated: None,
        range: ds.range(s.span),
        selection_range: ds.range(s.selection),
        children: (!s.children.is_empty()).then(|| {
            s.children
                .iter()
                .map(|c| to_document_symbol(ds, c))
                .collect()
        }),
    }
}

fn completion_kind(k: ItemKind) -> CompletionItemKind {
    match k {
        ItemKind::Section => CompletionItemKind::MODULE,
        ItemKind::Key => CompletionItemKind::PROPERTY,
        ItemKind::Member => CompletionItemKind::METHOD,
        ItemKind::Anchor => CompletionItemKind::REFERENCE,
        ItemKind::Keyword => CompletionItemKind::KEYWORD,
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
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["#".into(), " ".into(), "^".into(), ":".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "hobby-mml-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let td = params.text_document;
        self.refresh(td.uri, td.text, Some(td.version)).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.refresh(
                params.text_document.uri,
                change.text,
                Some(params.text_document.version),
            )
            .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let p = params.text_document_position_params;
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&p.text_document.uri).and_then(|ds| {
            let text = analysis::hover(&ds.doc, &ds.text, ds.offset(p.position))?;
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: text,
                }),
                range: None,
            })
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let p = params.text_document_position_params;
        Ok(self.definition(&p.text_document.uri, p.position))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let p = params.text_document_position;
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&p.text_document.uri).map(|ds| {
            let items = analysis::completions(&ds.doc, &ds.text, ds.offset(p.position))
                .into_iter()
                .map(|i| CompletionItem {
                    label: i.label,
                    kind: Some(completion_kind(i.kind)),
                    detail: i.detail,
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: ds.range(i.replace),
                        new_text: i.insert,
                    })),
                    ..Default::default()
                })
                .collect();
            CompletionResponse::Array(items)
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&params.text_document.uri).map(|ds| {
            DocumentSymbolResponse::Nested(
                analysis::symbols(&ds.doc)
                    .iter()
                    .map(|s| to_document_symbol(ds, s))
                    .collect(),
            )
        }))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let p = params.text_document_position;
        let uri = p.text_document.uri;
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&uri).map(|ds| {
            analysis::references(
                &ds.doc,
                &ds.text,
                ds.offset(p.position),
                params.context.include_declaration,
            )
            .into_iter()
            .map(|s| Location {
                uri: uri.clone(),
                range: ds.range(s),
            })
            .collect()
        }))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
