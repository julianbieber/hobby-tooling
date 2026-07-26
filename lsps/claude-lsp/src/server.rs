use std::path::PathBuf;
use std::sync::OnceLock;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::actions;
use crate::bridge::ZellijBridge;
use crate::symbols;

pub struct ClaudeLspServer {
    client: Client,
    bridge: ZellijBridge,
    workspace_root: OnceLock<PathBuf>,
}

impl ClaudeLspServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            bridge: ZellijBridge::new(),
            workspace_root: OnceLock::new(),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for ClaudeLspServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Prefer the first workspace folder, then the deprecated root_uri, then root_path.
        let root = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|f| f.uri.to_file_path().ok())
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(|u| u.to_file_path().ok())
            })
            .or_else(|| {
                #[allow(deprecated)]
                params.root_path.clone().map(PathBuf::from)
            });

        if let Some(root) = root {
            // Err only if `initialize` is called twice; first workspace root wins.
            let _ = self.workspace_root.set(root);
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // We need full file sync to detect claude comments
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // We provide code actions
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                // We handle command execution (for "Send to Claude")
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["claude-lsp.send".into()],
                    ..Default::default()
                }),
                // Expose claude comments through the symbol pickers
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("claude-lsp initialized");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        tracing::debug!("opened: {}", params.text_document.uri);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        tracing::debug!("changed: {}", params.text_document.uri);
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let range = params.range;

        // Read file content from disk (we need the full file for context)
        let file_path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("invalid file URI"))?;

        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            tower_lsp::jsonrpc::Error::invalid_params(format!("cannot read file: {e}"))
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let path_str = file_path.to_string_lossy();

        let mut code_actions = Vec::new();

        // --- "Send to Claude" actions for comments near the selection ---
        let claude_instructions = actions::find_claude_comments(&lines, &range, &path_str);

        for instruction in &claude_instructions {
            let title = format!("Claude: {}", actions::truncate(&instruction.text, 50));

            let request = actions::build_request(&path_str, &lines, &range, instruction);

            let args = serde_json::to_value(&request).ok();

            code_actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title,
                kind: Some(CodeActionKind::REFACTOR),
                command: Some(Command {
                    title: "Send to Claude".into(),
                    command: "claude-lsp.send".into(),
                    arguments: args.map(|a| vec![a]),
                }),
                ..Default::default()
            }));
        }

        // --- "Insert claude comment" action (always available) ---
        {
            let insert_line = range.start.line;
            let indent = lines
                .get(insert_line as usize)
                .map(|l| {
                    let trimmed = l.trim_start();
                    &l[..l.len() - trimmed.len()]
                })
                .unwrap_or("");
            let prefix = actions::comment_prefix_for_file(&path_str);
            let new_text = format!("{indent}{prefix}\n");

            let insert_pos = Position {
                line: insert_line,
                character: 0,
            };

            let mut changes = std::collections::HashMap::new();
            changes.insert(
                uri.clone(),
                vec![TextEdit {
                    range: Range {
                        start: insert_pos,
                        end: insert_pos,
                    },
                    new_text,
                }],
            );

            code_actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "Insert claude comment".into(),
                kind: Some(CodeActionKind::REFACTOR),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            }));
        }

        // --- Global "Clean up claude comments" action ---
        let comment_lines = actions::find_all_claude_comment_lines(&lines, &path_str);

        if !comment_lines.is_empty() {
            let count = comment_lines.len();

            // Build text edits that delete each comment line (including its newline)
            let edits: Vec<TextEdit> = comment_lines
                .iter()
                .map(|&line_idx| {
                    let start = Position {
                        line: line_idx,
                        character: 0,
                    };
                    // Delete up to the start of the next line (consuming the newline).
                    // If it's the last line, delete to the end of that line.
                    let end = if (line_idx as usize + 1) < lines.len() {
                        Position {
                            line: line_idx + 1,
                            character: 0,
                        }
                    } else {
                        Position {
                            line: line_idx,
                            character: lines[line_idx as usize].len() as u32,
                        }
                    };
                    TextEdit {
                        range: Range { start, end },
                        new_text: String::new(),
                    }
                })
                .collect();

            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), edits);

            code_actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Clean up claude comments ({count})"),
                kind: Some(CodeActionKind::SOURCE),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            }));
        }

        if code_actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(code_actions))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let file_path = uri
            .to_file_path()
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("invalid file URI"))?;
        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            tower_lsp::jsonrpc::Error::invalid_params(format!("cannot read file: {e}"))
        })?;
        let lines: Vec<&str> = content.lines().collect();
        let path_str = file_path.to_string_lossy();

        let syms = symbols::document_symbols(&lines, &path_str);
        if syms.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(syms)))
        }
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let Some(root) = self.workspace_root.get() else {
            return Ok(None);
        };
        let syms = symbols::workspace_symbols(root, &params.query);
        if syms.is_empty() {
            Ok(None)
        } else {
            Ok(Some(syms))
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "claude-lsp.send" => {
                if let Some(arg) = params.arguments.first() {
                    let request: actions::ClaudeRequest = serde_json::from_value(arg.clone())
                        .map_err(|e| {
                            tower_lsp::jsonrpc::Error::invalid_params(format!(
                                "invalid request: {e}"
                            ))
                        })?;

                    match self.bridge.send(&request).await {
                        Ok(()) => {
                            self.client
                                .show_message(MessageType::INFO, "Sent to Claude")
                                .await;
                        }
                        Err(e) => {
                            self.client
                                .show_message(
                                    MessageType::ERROR,
                                    format!("Failed to send to Claude: {e}"),
                                )
                                .await;
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
