//! The MCP server exposing extraction and graph queries as tools.

use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::{EdgeKind, UnknownKind};
use crate::graph_cache::{self, GraphError};
use crate::refresh::{GraphOutput, Outcome, RefreshError, Request, refresh};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefreshParams {
    /// Repository root to scan. Absolute path recommended.
    pub repo_path: PathBuf,
    /// Optional output path. Defaults to <repo>/.project-description.md.
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    /// Soft size threshold in KiB before method docstrings are dropped.
    #[serde(default)]
    pub size_threshold_kib: Option<usize>,
    /// Skip writing if the existing front matter `head:` matches current HEAD.
    #[serde(default)]
    pub skip_if_unchanged: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildGraphParams {
    /// Repository root to scan. Absolute path recommended.
    pub repo_path: PathBuf,
    /// Optional output path. Defaults to <repo>/.project-graph.json.
    #[serde(default)]
    pub output_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsParams {
    /// Repository root (the graph at <repo>/.project-graph.json is used, built lazily if absent).
    pub repo_path: PathBuf,
    /// Fuzzy search pattern matched against symbol names.
    pub query: String,
    /// Optional kind filter: file/class/struct/interface/enum/trait/type/function/method.
    #[serde(default)]
    pub kind: Option<String>,
    /// Optional language filter (e.g. "rust", "python").
    #[serde(default)]
    pub language: Option<String>,
    /// Max results (default 30, hard cap 100).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolParams {
    /// Repository root.
    pub repo_path: PathBuf,
    /// Node id ("relpath:Lstart"). Takes precedence over `name`.
    #[serde(default)]
    pub id: Option<String>,
    /// Exact symbol name (used when `id` is absent; ambiguous names return candidates).
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNeighborsParams {
    /// Repository root.
    pub repo_path: PathBuf,
    /// Node id ("relpath:Lstart") or a unique symbol name.
    pub id: String,
    /// "out" (default), "in", or "both".
    #[serde(default)]
    pub direction: Option<String>,
    /// Edge-kind filter: calls/imports/implements/contains (default: any).
    #[serde(default)]
    pub edge_kind: Option<String>,
    /// Max results (default 50, hard cap 200).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CallPathParams {
    /// Repository root.
    pub repo_path: PathBuf,
    /// Source node id or symbol name.
    pub from: String,
    /// Target node id or symbol name.
    pub to: String,
    /// Edge kinds to traverse (default ["calls"]).
    #[serde(default)]
    pub edge_kinds: Option<Vec<String>>,
    /// Max BFS depth (default 12, hard cap 25).
    #[serde(default)]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImpactParams {
    /// Repository root.
    pub repo_path: PathBuf,
    /// Symbol node id, symbol name, or a file relpath (expands to its symbols).
    pub id: String,
    /// Edge kinds for reverse reachability (default ["calls","imports"]).
    #[serde(default)]
    pub edge_kinds: Option<Vec<String>>,
    /// Max depth of the dependency walk (default 5, hard cap 10).
    #[serde(default)]
    pub max_depth: Option<usize>,
    /// Max dependents/files returned (default 100, hard cap 300).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Group dependents by file (default true) for a compact rollup.
    #[serde(default)]
    pub group_by_file: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListModulesParams {
    /// Repository root.
    pub repo_path: PathBuf,
    /// Skip directories with fewer than this many symbols (default 1).
    #[serde(default)]
    pub min_size: Option<usize>,
    /// Number of fan-in/out hotspots to report (default 10, hard cap 30).
    #[serde(default)]
    pub top_hotspots: Option<usize>,
}

/// The MCP server: one tool per extraction or graph query.
#[derive(Clone)]
pub struct TreeExtractServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TreeExtractServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Walk a repository with tree-sitter and write a markdown symbol map to <repo>/.project-description.md AND a queryable code graph to <repo>/.project-graph.json. The markdown lists classes/structs/traits/interfaces/enums + functions + methods with file:line ranges and docstrings; the graph adds call/import/inheritance/containment edges."
    )]
    async fn refresh_project_description(
        &self,
        Parameters(params): Parameters<RefreshParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || run_refresh(params)).await;
        Ok(tool_result(r, Ok))
    }

    #[tool(
        description = "(Re)build only the persisted code graph at <repo>/.project-graph.json (call/import/inheritance/containment edges). Returns node/edge counts and resolution stats."
    )]
    async fn build_graph(
        &self,
        Parameters(params): Parameters<BuildGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || run_build_graph(params)).await;
        Ok(tool_result(r, Ok))
    }

    #[tool(
        description = "Fuzzy-search symbol names in the code graph. Returns matching nodes with id, kind, and file:line."
    )]
    async fn search_symbols(
        &self,
        Parameters(p): Parameters<SearchSymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let kind = p.kind.as_deref().map(str::parse).transpose()?;
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, ToolError>(
                h.graph
                    .search(&p.query, kind, p.language.as_deref(), p.limit),
            )
        })
        .await;
        Ok(tool_result(r, json_text))
    }

    #[tool(
        description = "Full details for one graph node (by id or exact name): kind, file:line, docstring, and in/out edge counts by edge kind. Ambiguous names return candidate ids."
    )]
    async fn get_symbol(
        &self,
        Parameters(p): Parameters<GetSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, ToolError>(h.graph.get_symbol(p.id.as_deref(), p.name.as_deref()))
        })
        .await;
        Ok(tool_result(r, json_text))
    }

    #[tool(
        description = "1-hop neighbors of a node, filterable by direction (out/in/both) and edge kind. Covers callers, callees, importers, implementers."
    )]
    async fn get_neighbors(
        &self,
        Parameters(p): Parameters<GetNeighborsParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let direction = p
                .direction
                .as_deref()
                .map(str::parse)
                .transpose()?
                .unwrap_or_default();
            let edge_kind = p.edge_kind.as_deref().map(str::parse).transpose()?;
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, ToolError>(h.graph.neighbors(&p.id, direction, edge_kind, p.limit))
        })
        .await;
        Ok(tool_result(r, json_text))
    }

    #[tool(
        description = "Shortest path between two symbols over the chosen edge kinds (default: calls). BFS, depth-capped."
    )]
    async fn call_path(
        &self,
        Parameters(p): Parameters<CallPathParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let kinds = parse_edge_kinds(p.edge_kinds.as_deref())?;
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, ToolError>(
                h.graph
                    .call_path(&p.from, &p.to, kinds.as_deref(), p.max_depth),
            )
        })
        .await;
        Ok(tool_result(r, json_text))
    }

    #[tool(
        description = "What depends on X: the transitive set of symbols/files that directly or indirectly call or import a symbol or file (reverse reachability), depth-capped and file-grouped by default."
    )]
    async fn impact_of(
        &self,
        Parameters(p): Parameters<ImpactParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let kinds = parse_edge_kinds(p.edge_kinds.as_deref())?;
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, ToolError>(h.graph.impact(
                &p.id,
                kinds.as_deref(),
                p.max_depth,
                p.limit,
                p.group_by_file,
            ))
        })
        .await;
        Ok(tool_result(r, json_text))
    }

    #[tool(
        description = "List directory modules (with coupling counts), call fan-in/out hotspots ('god nodes'), and circular-dependency groups (strongly-connected components over calls + imports)."
    )]
    async fn list_modules(
        &self,
        Parameters(p): Parameters<ListModulesParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, ToolError>(h.graph.list_modules(p.min_size, p.top_hotspots))
        })
        .await;
        Ok(tool_result(r, json_text))
    }
}

#[derive(Debug, thiserror::Error)]
enum ToolError {
    #[error(transparent)]
    Argument(#[from] UnknownKind),
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error(transparent)]
    Refresh(#[from] RefreshError),
    #[error("cannot serialize the tool result")]
    Serialize(#[from] serde_json::Error),
}

fn parse_edge_kinds(raw: Option<&[String]>) -> Result<Option<Vec<EdgeKind>>, UnknownKind> {
    raw.map(|kinds| kinds.iter().map(|k| k.parse()).collect())
        .transpose()
}

fn run_refresh(params: RefreshParams) -> Result<String, ToolError> {
    let request = Request {
        repo: params.repo_path,
        output: params.output_path,
        size_threshold_bytes: params
            .size_threshold_kib
            .map(|kib| kib.saturating_mul(1024)),
        skip_if_unchanged: params.skip_if_unchanged.unwrap_or(false),
        exclude_from_git: true,
        graph: GraphOutput::Default,
    };
    Ok(match refresh(&request)? {
        Outcome::UpToDate { output, head } => {
            format!("skipped (HEAD {head}): {}", output.display())
        }
        Outcome::Written(w) => {
            let graph_note = w
                .graph
                .map(|g| {
                    format!(
                        "; graph {} ({} nodes, {} edges)",
                        g.path.display(),
                        g.stats.nodes,
                        g.stats.edges
                    )
                })
                .unwrap_or_default();
            format!(
                "wrote {} ({} symbols, {} bytes){graph_note}",
                w.output.display(),
                w.symbols,
                w.bytes
            )
        }
    })
}

fn run_build_graph(params: BuildGraphParams) -> Result<String, ToolError> {
    let (g, path) = graph_cache::build_and_write(&params.repo_path, params.output_path.as_deref())?;
    Ok(format!(
        "wrote {} ({} nodes, {} edges, {} ambiguous, {} unresolved calls, {} external imports)",
        path.display(),
        g.stats.nodes,
        g.stats.edges,
        g.stats.ambiguous_edges,
        g.stats.unresolved_calls,
        g.stats.external_imports
    ))
}

fn json_text<T: Serialize>(value: T) -> Result<String, ToolError> {
    Ok(serde_json::to_string(&value)?)
}

fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn tool_result<T>(
    r: Result<Result<T, ToolError>, tokio::task::JoinError>,
    render: impl FnOnce(T) -> Result<String, ToolError>,
) -> CallToolResult {
    let text = match r {
        Ok(result) => result.and_then(render).map_err(|e| error_chain(&e)),
        Err(e) => Err(error_chain(&e)),
    };
    match text {
        Ok(msg) => CallToolResult::success(vec![Content::text(msg)]),
        Err(msg) => CallToolResult::error(vec![Content::text(msg)]),
    }
}

impl Default for TreeExtractServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler]
impl ServerHandler for TreeExtractServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Extracts a repo's symbol structure into a markdown map (.project-description.md) \
                 and a queryable code graph (.project-graph.json) with call/import/inheritance/\
                 containment edges. Use refresh_project_description or build_graph to (re)build, \
                 then query with search_symbols, get_symbol, get_neighbors, call_path, impact_of, \
                 and list_modules. Edges are resolved by name (AST-level), so cross-file calls may \
                 be flagged ambiguous and external/stdlib calls are omitted."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
