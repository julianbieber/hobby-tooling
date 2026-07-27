use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ErrorData as McpError, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

use crate::fuzzy::fuzzy_search;
use crate::lsp::{symbol_kind_to_string, LspManager};
use crate::tools::completion::{
    CompletionParams as McpCompletionParams, CompletionResult,
    convert_completion_item,
};
use crate::tools::diagnostics::{
    CodeAction, CodeActionParams, GetDiagnosticsParams, Diagnostic,
    convert_lsp_diagnostic, extract_edits_from_code_action,
};
use crate::tools::document_symbols::{DocumentSymbolParams, DocumentSymbol};
use crate::tools::hover::{HoverParams, HoverResult};
use crate::tools::navigation::{GotoDefinitionParams, FindReferencesParams, Location, Position, Range};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolParams {
    #[schemars(description = "Search query (fuzzy search)")]
    pub query: String,

    #[schemars(description = "Language identifier to query (e.g., 'rust', 'python')")]
    pub language_id: String,

    #[schemars(description = "Maximum number of results to return (default: 100)")]
    #[serde(default)]
    pub limit: Option<usize>,

    #[schemars(description = "Optional path to a file or directory inside the target project. Used to locate the project root (via root markers like Cargo.toml) so the correct language server is queried. If omitted, the server's launch directory is used, which may index the wrong project.")]
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolResult {
    #[schemars(description = "Symbol name")]
    pub name: String,

    #[schemars(description = "Symbol kind (e.g., Function, Class, Variable)")]
    pub kind: String,

    #[schemars(description = "Container name (e.g., parent class or module)")]
    pub container: Option<String>,

    #[schemars(description = "File URI")]
    pub uri: String,

    #[schemars(description = "Line number (0-indexed)")]
    pub line: u32,

    #[schemars(description = "Character position (0-indexed)")]
    pub character: u32,

    #[schemars(description = "Fuzzy match score (higher is better)")]
    pub score: u32,
}

#[derive(Clone)]
pub struct LspMcpServer {
    tool_router: ToolRouter<Self>,
    lsp_manager: Arc<LspManager>,
}

#[tool_router]
impl LspMcpServer {
    pub fn new(lsp_manager: Arc<LspManager>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            lsp_manager,
        }
    }

    #[tool(description = "Search for symbols across the workspace. Fuzzy matches against symbol names and returns locations ranked by relevance.")]
    async fn workspace_symbol(
        &self,
        Parameters(params): Parameters<WorkspaceSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        // Query the language server for the specified language. When a path is
        // provided, root the server at the project containing it; otherwise fall
        // back to the launch directory.
        let client = match &params.path {
            Some(path) => {
                let file_path = std::path::PathBuf::from(path);
                self.lsp_manager
                    .get_server_for_path(&file_path, &params.language_id)
                    .await
            }
            None => self.lsp_manager.get_server_for_language(&params.language_id).await,
        }
        .map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to get language server for '{}': {}", params.language_id, e)),
            data: None,
        })?;
        let symbols = client.workspace_symbol(&params.query).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP query failed: {}", e)),
            data: None,
        })?;

        // Apply fuzzy matching and ranking
        let ranked = fuzzy_search(&symbols, &params.query);

        // Limit results
        let limit = params.limit.unwrap_or(100);
        let limited: Vec<_> = ranked.into_iter().take(limit).collect();

        // Convert to response format
        let results: Vec<WorkspaceSymbolResult> = limited
            .into_iter()
            .map(|ranked_symbol| {
                let symbol = ranked_symbol.symbol;
                WorkspaceSymbolResult {
                    name: symbol.name.clone(),
                    kind: symbol_kind_to_string(symbol.kind).to_string(),
                    container: symbol.container_name.clone(),
                    uri: symbol.location.uri.to_string(),
                    line: symbol.location.range.start.line,
                    character: symbol.location.range.start.character,
                    score: ranked_symbol.score,
                }
            })
            .collect();

        // Format as JSON
        let result_json = serde_json::to_string_pretty(&results).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} symbols:\n{}",
            results.len(),
            result_json
        ))]))
    }

    #[tool(description = "Get the current language configuration. Returns the configured language servers and language definitions loaded from Helix config.")]
    async fn get_language_config(&self) -> Result<CallToolResult, McpError> {
        let config = self.lsp_manager.config();

        let result_json = serde_json::to_string_pretty(config).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(result_json)]))
    }

    #[tool(description = "Go to the definition of a symbol at a given position. Returns the location(s) where the symbol is defined. The position must be exactly on an identifier (e.g., a variable name, function name, or type) - whitespace or punctuation will return no results.")]
    async fn goto_definition(
        &self,
        Parameters(params): Parameters<GotoDefinitionParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the language server
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Convert position
        let position = lsp_types::Position {
            line: params.position.line,
            character: params.position.character,
        };

        // Request definition
        let response = client.goto_definition(&uri, position).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP request failed: {}", e)),
            data: None,
        })?;

        // Convert response to our Location format
        let locations: Vec<Location> = match response {
            Some(lsp_types::GotoDefinitionResponse::Scalar(loc)) => {
                vec![Location {
                    uri: loc.uri.to_string(),
                    range: Range {
                        start: Position {
                            line: loc.range.start.line,
                            character: loc.range.start.character,
                        },
                        end: Position {
                            line: loc.range.end.line,
                            character: loc.range.end.character,
                        },
                    },
                }]
            }
            Some(lsp_types::GotoDefinitionResponse::Array(locs)) => {
                locs.into_iter()
                    .map(|loc| Location {
                        uri: loc.uri.to_string(),
                        range: Range {
                            start: Position {
                                line: loc.range.start.line,
                                character: loc.range.start.character,
                            },
                            end: Position {
                                line: loc.range.end.line,
                                character: loc.range.end.character,
                            },
                        },
                    })
                    .collect()
            }
            Some(lsp_types::GotoDefinitionResponse::Link(links)) => {
                links.into_iter()
                    .map(|link| Location {
                        uri: link.target_uri.to_string(),
                        range: Range {
                            start: Position {
                                line: link.target_selection_range.start.line,
                                character: link.target_selection_range.start.character,
                            },
                            end: Position {
                                line: link.target_selection_range.end.line,
                                character: link.target_selection_range.end.character,
                            },
                        },
                    })
                    .collect()
            }
            None => vec![],
        };

        // Format as JSON
        let result_json = serde_json::to_string_pretty(&locations).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} definition(s):\n{}",
            locations.len(),
            result_json
        ))]))
    }

    #[tool(description = "Find all references to a symbol at a given position. Returns all locations where the symbol is used. The position must be exactly on an identifier.")]
    async fn find_references(
        &self,
        Parameters(params): Parameters<FindReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the language server
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Convert position
        let position = lsp_types::Position {
            line: params.position.line,
            character: params.position.character,
        };

        // Request references
        let response = client.find_references(&uri, position, params.include_declaration).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP request failed: {}", e)),
            data: None,
        })?;

        // Convert response to our Location format
        let locations: Vec<Location> = response
            .unwrap_or_default()
            .into_iter()
            .map(|loc| Location {
                uri: loc.uri.to_string(),
                range: Range {
                    start: Position {
                        line: loc.range.start.line,
                        character: loc.range.start.character,
                    },
                    end: Position {
                        line: loc.range.end.line,
                        character: loc.range.end.character,
                    },
                },
            })
            .collect();

        // Format as JSON
        let result_json = serde_json::to_string_pretty(&locations).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} reference(s):\n{}",
            locations.len(),
            result_json
        ))]))
    }

    #[tool(description = "Get hover information (type info, documentation) for a symbol at a given position. The position must be exactly on an identifier.")]
    async fn hover(
        &self,
        Parameters(params): Parameters<HoverParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the language server
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Convert position
        let position = lsp_types::Position {
            line: params.position.line,
            character: params.position.character,
        };

        // Request hover
        let response = client.hover(&uri, position).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP request failed: {}", e)),
            data: None,
        })?;

        match response {
            Some(hover) => {
                let contents = hover_contents_to_string(&hover.contents);
                let result = HoverResult { contents };
                let result_json = serde_json::to_string_pretty(&result).map_err(|e| McpError {
                    code: rmcp::model::ErrorCode(-32603),
                    message: Cow::from(format!("JSON serialization failed: {}", e)),
                    data: None,
                })?;
                Ok(CallToolResult::success(vec![Content::text(result_json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                "No hover information available at this position.",
            )])),
        }
    }

    #[tool(description = "Get all symbols in a specific document. Returns a hierarchical list of symbols (functions, classes, variables, etc.) in the file.")]
    async fn document_symbol(
        &self,
        Parameters(params): Parameters<DocumentSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the appropriate language server, rooted at the file's project
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server (required before documentSymbol)
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Request document symbols
        let response = client.document_symbol(&uri).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP request failed: {}", e)),
            data: None,
        })?;

        // Convert response to our format
        let symbols = match response {
            Some(lsp_types::DocumentSymbolResponse::Flat(symbol_info)) => {
                // Convert flat SymbolInformation to DocumentSymbol
                symbol_info
                    .into_iter()
                    .map(|info| DocumentSymbol {
                        name: info.name,
                        kind: symbol_kind_to_string(info.kind).to_string(),
                        detail: None,
                        start_line: info.location.range.start.line,
                        start_character: info.location.range.start.character,
                        end_line: info.location.range.end.line,
                        end_character: info.location.range.end.character,
                        children: vec![],
                    })
                    .collect()
            }
            Some(lsp_types::DocumentSymbolResponse::Nested(doc_symbols)) => {
                // Convert hierarchical DocumentSymbol
                fn convert_doc_symbol(symbol: lsp_types::DocumentSymbol) -> DocumentSymbol {
                    DocumentSymbol {
                        name: symbol.name,
                        kind: symbol_kind_to_string(symbol.kind).to_string(),
                        detail: symbol.detail,
                        start_line: symbol.selection_range.start.line,
                        start_character: symbol.selection_range.start.character,
                        end_line: symbol.selection_range.end.line,
                        end_character: symbol.selection_range.end.character,
                        children: symbol
                            .children
                            .unwrap_or_default()
                            .into_iter()
                            .map(convert_doc_symbol)
                            .collect(),
                    }
                }
                doc_symbols.into_iter().map(convert_doc_symbol).collect()
            }
            None => vec![],
        };

        // Format as JSON
        let result_json = serde_json::to_string_pretty(&symbols).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} symbols:\n{}",
            symbols.len(),
            result_json
        ))]))
    }

    #[tool(description = "Get diagnostics (errors, warnings) for a file. Opens the file in the language server and returns all diagnostic issues found.")]
    async fn get_diagnostics(
        &self,
        Parameters(params): Parameters<GetDiagnosticsParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the language server
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Try pull diagnostics first, fall back to stored diagnostics from notifications
        let lsp_diagnostics = match client.pull_diagnostics(&uri).await {
            Ok(diags) => diags,
            Err(_) => client.get_stored_diagnostics(&uri).await,
        };

        // Convert to our format
        let diagnostics: Vec<Diagnostic> = lsp_diagnostics
            .iter()
            .map(convert_lsp_diagnostic)
            .collect();

        let result_json = serde_json::to_string_pretty(&diagnostics).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} diagnostic(s):\n{}",
            diagnostics.len(),
            result_json
        ))]))
    }

    #[tool(description = "Get code actions (quick fixes, refactorings) available at a given range. Typically used after get_diagnostics to get fixes for specific issues.")]
    async fn code_action(
        &self,
        Parameters(params): Parameters<CodeActionParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the language server
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Convert range
        let range = lsp_types::Range {
            start: lsp_types::Position {
                line: params.range.start.line,
                character: params.range.start.character,
            },
            end: lsp_types::Position {
                line: params.range.end.line,
                character: params.range.end.character,
            },
        };

        // Get stored diagnostics that overlap the given range to provide context
        let stored_diags = client.get_stored_diagnostics(&uri).await;
        let relevant_diags: Vec<lsp_types::Diagnostic> = stored_diags
            .into_iter()
            .filter(|d| ranges_overlap(&d.range, &range))
            .collect();

        // Request code actions
        let response = client.code_action(&uri, range, relevant_diags).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP request failed: {}", e)),
            data: None,
        })?;

        // Convert response
        let actions: Vec<CodeAction> = response
            .unwrap_or_default()
            .into_iter()
            .filter_map(|action_or_cmd| match action_or_cmd {
                lsp_types::CodeActionOrCommand::CodeAction(action) => {
                    let edits = extract_edits_from_code_action(&action);
                    Some(CodeAction {
                        title: action.title,
                        kind: action.kind.map(|k| k.as_str().to_string()),
                        is_preferred: action.is_preferred,
                        edits,
                    })
                }
                lsp_types::CodeActionOrCommand::Command(cmd) => Some(CodeAction {
                    title: cmd.title,
                    kind: None,
                    is_preferred: None,
                    edits: vec![],
                }),
            })
            .collect();

        let result_json = serde_json::to_string_pretty(&actions).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} code action(s):\n{}",
            actions.len(),
            result_json
        ))]))
    }

    #[tool(description = "Get completion suggestions at a given position in a file. Returns possible completions based on the cursor context (e.g., after a dot, partial identifier, etc.).")]
    async fn completion(
        &self,
        Parameters(params): Parameters<McpCompletionParams>,
    ) -> Result<CallToolResult, McpError> {
        // Parse URI
        let uri = lsp_types::Url::parse(&params.uri).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from(format!("Invalid URI: {}", e)),
            data: None,
        })?;

        // Get file path and read contents
        let file_path = uri.to_file_path().map_err(|_| McpError {
            code: rmcp::model::ErrorCode(-32602),
            message: Cow::from("Invalid file URI"),
            data: None,
        })?;
        let file_contents = std::fs::read_to_string(&file_path).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to read file: {}", e)),
            data: None,
        })?;

        // Get the language server
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, &params.language_id)
            .await
            .map_err(|e| McpError {
                code: rmcp::model::ErrorCode(-32603),
                message: Cow::from(format!("Failed to get language server: {}", e)),
                data: None,
            })?;

        // Open the document in the LSP server
        client.did_open(&uri, &params.language_id, &file_contents).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("Failed to open document: {}", e)),
            data: None,
        })?;

        // Convert position
        let position = lsp_types::Position {
            line: params.position.line,
            character: params.position.character,
        };

        // Request completions
        let response = client.completion(&uri, position).await.map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("LSP request failed: {}", e)),
            data: None,
        })?;

        // Convert response
        let result = match response {
            Some(lsp_types::CompletionResponse::Array(items)) => CompletionResult {
                is_incomplete: false,
                items: items.iter().map(convert_completion_item).collect(),
            },
            Some(lsp_types::CompletionResponse::List(list)) => CompletionResult {
                is_incomplete: list.is_incomplete,
                items: list.items.iter().map(convert_completion_item).collect(),
            },
            None => CompletionResult {
                is_incomplete: false,
                items: vec![],
            },
        };

        let result_json = serde_json::to_string_pretty(&result).map_err(|e| McpError {
            code: rmcp::model::ErrorCode(-32603),
            message: Cow::from(format!("JSON serialization failed: {}", e)),
            data: None,
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Found {} completion(s){}:\n{}",
            result.items.len(),
            if result.is_incomplete { " (incomplete list)" } else { "" },
            result_json
        ))]))
    }
}

/// Convert LSP HoverContents to a single markdown string
fn hover_contents_to_string(contents: &lsp_types::HoverContents) -> String {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => marked_string_to_string(marked),
        lsp_types::HoverContents::Array(entries) => entries
            .iter()
            .map(marked_string_to_string)
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
        lsp_types::HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn marked_string_to_string(marked: &lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s.clone(),
        lsp_types::MarkedString::LanguageString(ls) => {
            format!("```{}\n{}\n```", ls.language, ls.value)
        }
    }
}

/// Check if two LSP ranges overlap
fn ranges_overlap(a: &lsp_types::Range, b: &lsp_types::Range) -> bool {
    let a_before_b = a.end.line < b.start.line
        || (a.end.line == b.start.line && a.end.character < b.start.character);
    let b_before_a = b.end.line < a.start.line
        || (b.end.line == a.start.line && b.end.character < a.start.character);
    !(a_before_b || b_before_a)
}

#[tool_handler]
impl ServerHandler for LspMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Provides Language Server Protocol (LSP) capabilities through the Model Context Protocol (MCP). \
                Supports fuzzy symbol search across your workspace using configured language servers from Helix."
                    .into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_symbol_result_serialization() {
        let result = WorkspaceSymbolResult {
            name: "test_function".to_string(),
            kind: "Function".to_string(),
            container: Some("MyModule".to_string()),
            uri: "file:///test.rs".to_string(),
            line: 10,
            character: 5,
            score: 100,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test_function"));
        assert!(json.contains("Function"));
        assert!(json.contains("MyModule"));
    }

    #[test]
    fn test_workspace_symbol_result_no_container() {
        let result = WorkspaceSymbolResult {
            name: "standalone".to_string(),
            kind: "Variable".to_string(),
            container: None,
            uri: "file:///main.rs".to_string(),
            line: 0,
            character: 0,
            score: 50,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("standalone"));
        assert!(json.contains("null"));
    }

    #[test]
    fn test_workspace_symbol_params_default_limit() {
        let json = r#"{"query": "test", "language_id": "rust"}"#;
        let params: WorkspaceSymbolParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.query, "test");
        assert_eq!(params.limit, None);
        assert_eq!(params.path, None);
    }

    #[test]
    fn test_workspace_symbol_params_with_limit() {
        let json = r#"{"query": "test", "language_id": "rust", "limit": 50}"#;
        let params: WorkspaceSymbolParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.query, "test");
        assert_eq!(params.limit, Some(50));
    }
}
