use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use super::navigation::{Position, Range};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDiagnosticsParams {
    #[schemars(description = "The file URI to get diagnostics for")]
    pub uri: String,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    #[schemars(description = "The range where the diagnostic applies")]
    pub range: Range,
    #[schemars(description = "The severity level (1=Error, 2=Warning, 3=Information, 4=Hint)")]
    pub severity: u32,
    #[schemars(description = "The diagnostic message")]
    pub message: String,
    #[schemars(description = "Optional source of the diagnostic (e.g., 'rustc', 'clippy')")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[schemars(description = "Optional diagnostic code")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CodeActionParams {
    #[schemars(description = "The file URI")]
    pub uri: String,
    #[schemars(description = "The range to get code actions for (typically a diagnostic range)")]
    pub range: Range,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CodeAction {
    #[schemars(description = "The title of the code action")]
    pub title: String,
    #[schemars(description = "The kind of code action (e.g., 'quickfix', 'refactor')")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[schemars(description = "Whether this is the preferred action")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_preferred: Option<bool>,
    #[schemars(description = "The text edits to apply")]
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TextEdit {
    #[schemars(description = "The file URI")]
    pub uri: String,
    #[schemars(description = "The range to replace")]
    pub range: Range,
    #[schemars(description = "The new text to insert")]
    pub new_text: String,
}

/// Convert an lsp_types::Diagnostic to our Diagnostic format
pub fn convert_lsp_diagnostic(d: &lsp_types::Diagnostic) -> Diagnostic {
    let severity = match d.severity {
        Some(lsp_types::DiagnosticSeverity::ERROR) => 1,
        Some(lsp_types::DiagnosticSeverity::WARNING) => 2,
        Some(lsp_types::DiagnosticSeverity::INFORMATION) => 3,
        Some(lsp_types::DiagnosticSeverity::HINT) => 4,
        _ => 0,
    };
    let code = d.code.as_ref().map(|c| match c {
        lsp_types::NumberOrString::Number(n) => n.to_string(),
        lsp_types::NumberOrString::String(s) => s.clone(),
    });
    Diagnostic {
        range: Range {
            start: Position {
                line: d.range.start.line,
                character: d.range.start.character,
            },
            end: Position {
                line: d.range.end.line,
                character: d.range.end.character,
            },
        },
        severity,
        message: d.message.clone(),
        source: d.source.clone(),
        code,
    }
}

/// Extract text edits from an lsp_types::CodeAction into our simplified format
pub fn extract_edits_from_code_action(action: &lsp_types::CodeAction) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    if let Some(workspace_edit) = &action.edit {
        if let Some(changes) = &workspace_edit.changes {
            for (uri, text_edits) in changes {
                for edit in text_edits {
                    edits.push(TextEdit {
                        uri: uri.to_string(),
                        range: Range {
                            start: Position {
                                line: edit.range.start.line,
                                character: edit.range.start.character,
                            },
                            end: Position {
                                line: edit.range.end.line,
                                character: edit.range.end.character,
                            },
                        },
                        new_text: edit.new_text.clone(),
                    });
                }
            }
        }
        if let Some(doc_changes) = &workspace_edit.document_changes {
            let doc_edits = match doc_changes {
                lsp_types::DocumentChanges::Edits(edits) => edits.clone(),
                lsp_types::DocumentChanges::Operations(ops) => ops
                    .iter()
                    .filter_map(|op| match op {
                        lsp_types::DocumentChangeOperation::Edit(edit) => Some(edit.clone()),
                        _ => None,
                    })
                    .collect(),
            };
            for doc_edit in &doc_edits {
                for edit in &doc_edit.edits {
                    let (range, new_text) = match edit {
                        lsp_types::OneOf::Left(text_edit) => {
                            (text_edit.range, text_edit.new_text.clone())
                        }
                        lsp_types::OneOf::Right(annotated) => {
                            (annotated.text_edit.range, annotated.text_edit.new_text.clone())
                        }
                    };
                    edits.push(TextEdit {
                        uri: doc_edit.text_document.uri.to_string(),
                        range: Range {
                            start: Position {
                                line: range.start.line,
                                character: range.start.character,
                            },
                            end: Position {
                                line: range.end.line,
                                character: range.end.character,
                            },
                        },
                        new_text,
                    });
                }
            }
        }
    }
    edits
}
