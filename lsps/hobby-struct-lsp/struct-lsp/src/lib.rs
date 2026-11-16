//! `hobby-struct-lsp`: a `tower-lsp` language server for `structure.hstruct`
//! files, built on the `struct-parser` crate.
//!
//! This file is protocol glue only. Cursor logic lives in [`analysis`] as pure
//! functions, and every diagnostic comes from the same parse and check the CLI
//! runs, so the editor and `just structure` can never disagree.

mod analysis;

use std::collections::HashMap;
use std::path::PathBuf;

use parking_lot::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use analysis::{CompletionContext, METRICS, VERIFICATION_KEYS, completion_context, standard_at};
use struct_parser::ast::Verification;
use struct_parser::span::{LineIndex, Span};
use struct_parser::{Spec, check, parse};

/// Everything cached for one open document. Files are small, so a change
/// reparses wholesale rather than editing the tree.
struct DocState {
    text: String,
    spec: Spec,
    line_index: LineIndex,
}

struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, DocState>>,
}

impl Backend {
    fn load(&self, uri: Url, text: String) -> Vec<Diagnostic> {
        let parsed = parse(&text);
        let repo_root = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let checked = check(&parsed.spec, &repo_root);
        let line_index = LineIndex::new(&text);

        let diagnostics = parsed
            .diagnostics
            .into_iter()
            .chain(checked.diagnostics)
            .map(|d| Diagnostic {
                range: range_of(&line_index, d.span),
                severity: Some(match d.severity {
                    struct_parser::Severity::Error => DiagnosticSeverity::ERROR,
                    struct_parser::Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                code: Some(NumberOrString::String(d.code.to_owned())),
                source: Some("hstruct".to_owned()),
                message: d.message,
                ..Diagnostic::default()
            })
            .collect();

        self.documents.lock().insert(
            uri,
            DocState {
                text,
                spec: parsed.spec,
                line_index,
            },
        );
        diagnostics
    }

    async fn refresh(&self, uri: Url, text: String) {
        let diagnostics = self.load(uri.clone(), text);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn range_of(index: &LineIndex, span: Span) -> Range {
    let start = index.position(span.start);
    let end = index.position(span.end);
    Range {
        start: Position::new(start.line, start.character),
        end: Position::new(end.line, end.character),
    }
}

fn offset_of(text: &str, position: Position) -> usize {
    let target_line = usize::try_from(position.line).unwrap_or(usize::MAX);
    let target_unit = usize::try_from(position.character).unwrap_or(usize::MAX);
    let mut offset = 0;
    for (i, line) in text.split('\n').enumerate() {
        if i == target_line {
            let mut units = 0;
            for (byte, c) in line.char_indices() {
                if units >= target_unit {
                    return offset + byte;
                }
                units += c.len_utf16();
            }
            return offset + line.len();
        }
        offset += line.len() + 1;
    }
    text.len()
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["!".to_owned(), " ".to_owned()]),
                    ..CompletionOptions::default()
                }),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "hobby-struct-lsp".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.refresh(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.refresh(params.text_document.uri, change.text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let text = params.text.or_else(|| {
            params
                .text_document
                .uri
                .to_file_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
        });
        if let Some(text) = text {
            self.refresh(params.text_document.uri, text).await;
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let documents = self.documents.lock();
        let Some(doc) = documents.get(&uri) else {
            return Ok(None);
        };
        let offset = offset_of(&doc.text, position);
        let Some(standard) = standard_at(offset, &doc.spec) else {
            return Ok(None);
        };
        let name = &standard.name.value;

        let how = match &standard.verification {
            Some(Verification::Counted { metric, budget }) => {
                format!(
                    "counts `{}`, budget {}",
                    metric.value.render(),
                    budget.value
                )
            }
            Some(Verification::Reviewed { audited, .. }) => format!(
                "reviewed, not counted — audited {}",
                audited
                    .as_ref()
                    .map(|a| a.value.as_str())
                    .unwrap_or("never")
            ),
            None => "no verification stated".to_owned(),
        };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{name}**\n\n{}\n\n_{how}_", standard.prose),
            }),
            range: Some(range_of(&doc.line_index, standard.name.span)),
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let documents = self.documents.lock();
        let Some(doc) = documents.get(&uri) else {
            return Ok(None);
        };
        let offset = offset_of(&doc.text, position);

        let snippets: &[&str] = match completion_context(&doc.text, offset) {
            CompletionContext::Metric => METRICS,
            CompletionContext::VerificationKey => VERIFICATION_KEYS,
            CompletionContext::None => &[],
        };
        let items = snippets
            .iter()
            .map(|form| CompletionItem {
                label: form.replace("${1:", "").replace('}', ""),
                kind: Some(CompletionItemKind::SNIPPET),
                insert_text: Some((*form).to_owned()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..CompletionItem::default()
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }
}

/// Speak the protocol over stdin and stdout until the client disconnects.
pub async fn serve_stdio() {
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Mutex::new(HashMap::new()),
    });
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
