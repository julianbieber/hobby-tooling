//! The MCP server: the LSP-backed tools it exposes and how each call is
//! answered.

use rmcp::{
    ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorData as McpError, Implementation, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::error_chain;
use crate::fuzzy::fuzzy_search;
use crate::lsp::{LspClient, LspError, LspManager, ManagerError, symbol_kind_to_string};
use crate::tools::completion::{
    CompletionParams as McpCompletionParams, CompletionResult, convert_completion_item,
};
use crate::tools::diagnostics::{
    CodeAction, CodeActionParams, Diagnostic, GetDiagnosticsParams, convert_lsp_diagnostic,
    extract_edits_from_code_action,
};
use crate::tools::document_symbols::{DocumentSymbol, DocumentSymbolParams};
use crate::tools::hover::{HoverParams, HoverResult};
use crate::tools::navigation::{FindReferencesParams, GotoDefinitionParams, Location};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkspaceSymbolParams {
    #[schemars(description = "Search query (fuzzy search)")]
    pub query: String,

    #[schemars(description = "Language identifier to query (e.g., 'rust', 'python')")]
    pub language_id: String,

    #[schemars(description = "Maximum number of results to return (default: 100)")]
    #[serde(default)]
    pub limit: Option<usize>,

    #[schemars(
        description = "Optional path to a file or directory inside the target project. Used to locate the project root (via root markers like Cargo.toml) so the correct language server is queried. If omitted, the server's launch directory is used, which may index the wrong project."
    )]
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

#[derive(Debug, thiserror::Error)]
enum ToolError {
    #[error("invalid URI {uri}")]
    InvalidUri {
        uri: String,
        #[source]
        source: <lsp_types::Url as std::str::FromStr>::Err,
    },
    #[error("not a file URI: {0}")]
    NotAFileUri(String),
    #[error("failed to read file {}", .path.display())]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no language server for '{language_id}'")]
    NoServer {
        language_id: String,
        #[source]
        source: ManagerError,
    },
    #[error("failed to open document in {server}")]
    Open {
        server: String,
        #[source]
        source: LspError,
    },
    #[error("{server} request failed")]
    Request {
        server: String,
        #[source]
        source: LspError,
    },
    #[error("JSON serialization failed")]
    Serialize(#[source] serde_json::Error),
}

impl From<ToolError> for McpError {
    fn from(error: ToolError) -> Self {
        let message = error_chain(&error);
        match error {
            ToolError::InvalidUri { .. } | ToolError::NotAFileUri(_) => {
                McpError::invalid_params(message, None)
            }
            _ => McpError::internal_error(message, None),
        }
    }
}

struct OpenDocument {
    uri: lsp_types::Url,
    client: Arc<LspClient>,
}

impl OpenDocument {
    fn failed(&self) -> impl FnOnce(LspError) -> ToolError + '_ {
        |source| ToolError::Request {
            server: self.client.server_name().to_string(),
            source,
        }
    }
}

fn json_text<T: Serialize>(heading: &str, value: &T) -> Result<CallToolResult, ToolError> {
    let json = serde_json::to_string_pretty(value).map_err(ToolError::Serialize)?;
    Ok(CallToolResult::success(vec![Content::text(format!(
        "{heading}\n{json}"
    ))]))
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

    /// Parse the URI, read the file, find its language server rooted at the
    /// file's project, and open the document there, as every per-file request
    /// needs.
    async fn open_document(&self, uri: &str, language_id: &str) -> Result<OpenDocument, ToolError> {
        let parsed = lsp_types::Url::parse(uri).map_err(|source| ToolError::InvalidUri {
            uri: uri.to_string(),
            source,
        })?;
        let file_path = parsed
            .to_file_path()
            .map_err(|()| ToolError::NotAFileUri(uri.to_string()))?;
        let file_contents = tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|source| ToolError::ReadFile {
                path: file_path.clone(),
                source,
            })?;
        let client = self
            .lsp_manager
            .get_server_for_path(&file_path, language_id)
            .await
            .map_err(|source| ToolError::NoServer {
                language_id: language_id.to_string(),
                source,
            })?;
        client
            .did_open(&parsed, language_id, &file_contents)
            .await
            .map_err(|source| ToolError::Open {
                server: client.server_name().to_string(),
                source,
            })?;
        Ok(OpenDocument {
            uri: parsed,
            client,
        })
    }

    #[tool(
        description = "Search for symbols across the workspace. Fuzzy matches against symbol names and returns locations ranked by relevance."
    )]
    async fn workspace_symbol(
        &self,
        Parameters(params): Parameters<WorkspaceSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = match &params.path {
            Some(path) => {
                self.lsp_manager
                    .get_server_for_path(&PathBuf::from(path), &params.language_id)
                    .await
            }
            None => {
                self.lsp_manager
                    .get_server_for_language(&params.language_id)
                    .await
            }
        }
        .map_err(|source| ToolError::NoServer {
            language_id: params.language_id.clone(),
            source,
        })?;
        let symbols = client
            .workspace_symbol(&params.query)
            .await
            .map_err(|source| ToolError::Request {
                server: client.server_name().to_string(),
                source,
            })?;

        let results: Vec<WorkspaceSymbolResult> = fuzzy_search(&symbols, &params.query)
            .into_iter()
            .take(params.limit.unwrap_or(100))
            .map(|ranked_symbol| {
                let symbol = ranked_symbol.symbol;
                WorkspaceSymbolResult {
                    name: symbol.name,
                    kind: symbol_kind_to_string(symbol.kind).to_string(),
                    container: symbol.container_name,
                    uri: symbol.location.uri.to_string(),
                    line: symbol.location.range.start.line,
                    character: symbol.location.range.start.character,
                    score: ranked_symbol.score,
                }
            })
            .collect();

        Ok(json_text(
            &format!("Found {} symbols:", results.len()),
            &results,
        )?)
    }

    #[tool(
        description = "Get the current language configuration. Returns the configured language servers and language definitions loaded from Helix config."
    )]
    async fn get_language_config(&self) -> Result<CallToolResult, McpError> {
        let result_json = serde_json::to_string_pretty(self.lsp_manager.config())
            .map_err(ToolError::Serialize)?;
        Ok(CallToolResult::success(vec![Content::text(result_json)]))
    }

    #[tool(
        description = "Go to the definition of a symbol at a given position. Returns the location(s) where the symbol is defined. The position must be exactly on an identifier (e.g., a variable name, function name, or type) - whitespace or punctuation will return no results."
    )]
    async fn goto_definition(
        &self,
        Parameters(params): Parameters<GotoDefinitionParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;
        let response = doc
            .client
            .goto_definition(&doc.uri, params.position.into())
            .await
            .map_err(doc.failed())?;

        let locations: Vec<Location> = match response {
            Some(lsp_types::GotoDefinitionResponse::Scalar(loc)) => vec![loc.into()],
            Some(lsp_types::GotoDefinitionResponse::Array(locs)) => {
                locs.into_iter().map(Location::from).collect()
            }
            Some(lsp_types::GotoDefinitionResponse::Link(links)) => links
                .into_iter()
                .map(|link| Location {
                    uri: link.target_uri.to_string(),
                    range: link.target_selection_range.into(),
                })
                .collect(),
            None => vec![],
        };

        Ok(json_text(
            &format!("Found {} definition(s):", locations.len()),
            &locations,
        )?)
    }

    #[tool(
        description = "Find all references to a symbol at a given position. Returns all locations where the symbol is used. The position must be exactly on an identifier."
    )]
    async fn find_references(
        &self,
        Parameters(params): Parameters<FindReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;
        let response = doc
            .client
            .find_references(&doc.uri, params.position.into(), params.include_declaration)
            .await
            .map_err(doc.failed())?;

        let locations: Vec<Location> = response
            .unwrap_or_default()
            .into_iter()
            .map(Location::from)
            .collect();

        Ok(json_text(
            &format!("Found {} reference(s):", locations.len()),
            &locations,
        )?)
    }

    #[tool(
        description = "Get hover information (type info, documentation) for a symbol at a given position. The position must be exactly on an identifier."
    )]
    async fn hover(
        &self,
        Parameters(params): Parameters<HoverParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;
        let response = doc
            .client
            .hover(&doc.uri, params.position.into())
            .await
            .map_err(doc.failed())?;

        match response {
            Some(hover) => {
                let result = HoverResult {
                    contents: hover_contents_to_string(&hover.contents),
                };
                let result_json =
                    serde_json::to_string_pretty(&result).map_err(ToolError::Serialize)?;
                Ok(CallToolResult::success(vec![Content::text(result_json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                "No hover information available at this position.",
            )])),
        }
    }

    #[tool(
        description = "Get all symbols in a specific document. Returns a hierarchical list of symbols (functions, classes, variables, etc.) in the file."
    )]
    async fn document_symbol(
        &self,
        Parameters(params): Parameters<DocumentSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;
        let response = doc
            .client
            .document_symbol(&doc.uri)
            .await
            .map_err(doc.failed())?;

        let symbols: Vec<DocumentSymbol> = match response {
            Some(lsp_types::DocumentSymbolResponse::Flat(symbol_info)) => symbol_info
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
                .collect(),
            Some(lsp_types::DocumentSymbolResponse::Nested(doc_symbols)) => {
                doc_symbols.into_iter().map(convert_doc_symbol).collect()
            }
            None => vec![],
        };

        Ok(json_text(
            &format!("Found {} symbols:", symbols.len()),
            &symbols,
        )?)
    }

    #[tool(
        description = "Get diagnostics (errors, warnings) for a file. Opens the file in the language server and returns all diagnostic issues found."
    )]
    async fn get_diagnostics(
        &self,
        Parameters(params): Parameters<GetDiagnosticsParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;

        let lsp_diagnostics = match doc.client.pull_diagnostics(&doc.uri).await {
            Ok(diags) => diags,
            Err(_) => doc.client.get_stored_diagnostics(&doc.uri),
        };

        let diagnostics: Vec<Diagnostic> =
            lsp_diagnostics.iter().map(convert_lsp_diagnostic).collect();

        Ok(json_text(
            &format!("Found {} diagnostic(s):", diagnostics.len()),
            &diagnostics,
        )?)
    }

    #[tool(
        description = "Get code actions (quick fixes, refactorings) available at a given range. Typically used after get_diagnostics to get fixes for specific issues."
    )]
    async fn code_action(
        &self,
        Parameters(params): Parameters<CodeActionParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;
        let range: lsp_types::Range = params.range.into();

        let relevant_diags: Vec<lsp_types::Diagnostic> = doc
            .client
            .get_stored_diagnostics(&doc.uri)
            .into_iter()
            .filter(|d| ranges_overlap(&d.range, &range))
            .collect();

        let response = doc
            .client
            .code_action(&doc.uri, range, relevant_diags)
            .await
            .map_err(doc.failed())?;

        let actions: Vec<CodeAction> = response
            .unwrap_or_default()
            .into_iter()
            .map(|action_or_cmd| match action_or_cmd {
                lsp_types::CodeActionOrCommand::CodeAction(action) => {
                    let edits = extract_edits_from_code_action(&action);
                    CodeAction {
                        title: action.title,
                        kind: action.kind.map(|k| k.as_str().to_string()),
                        is_preferred: action.is_preferred,
                        edits,
                    }
                }
                lsp_types::CodeActionOrCommand::Command(cmd) => CodeAction {
                    title: cmd.title,
                    kind: None,
                    is_preferred: None,
                    edits: vec![],
                },
            })
            .collect();

        Ok(json_text(
            &format!("Found {} code action(s):", actions.len()),
            &actions,
        )?)
    }

    #[tool(
        description = "Get completion suggestions at a given position in a file. Returns possible completions based on the cursor context (e.g., after a dot, partial identifier, etc.)."
    )]
    async fn completion(
        &self,
        Parameters(params): Parameters<McpCompletionParams>,
    ) -> Result<CallToolResult, McpError> {
        let doc = self.open_document(&params.uri, &params.language_id).await?;
        let response = doc
            .client
            .completion(&doc.uri, params.position.into())
            .await
            .map_err(doc.failed())?;

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

        let heading = format!(
            "Found {} completion(s){}:",
            result.items.len(),
            if result.is_incomplete {
                " (incomplete list)"
            } else {
                ""
            }
        );
        Ok(json_text(&heading, &result)?)
    }
}

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
