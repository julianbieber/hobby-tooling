use std::path::{Path, PathBuf};
use std::process::Command;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use hobby_tree_extract::extractor::Extractor;
use hobby_tree_extract::md_render::{RenderOptions, render};
use hobby_tree_extract::{gitignore, graph, graph_cache};

// ---------------------------------------------------------------------------
// Tool parameter structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefreshParams {
    /// Repository root to scan. Absolute path recommended.
    pub repo_path: String,
    /// Optional output path. Defaults to <repo>/.project-description.md.
    #[serde(default)]
    pub output_path: Option<String>,
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
    pub repo_path: String,
    /// Optional output path. Defaults to <repo>/.project-graph.json.
    #[serde(default)]
    pub output_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsParams {
    /// Repository root (the graph at <repo>/.project-graph.json is used, built lazily if absent).
    pub repo_path: String,
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
    pub repo_path: String,
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
    pub repo_path: String,
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
    pub repo_path: String,
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
    pub repo_path: String,
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
    pub repo_path: String,
    /// Skip directories with fewer than this many symbols (default 1).
    #[serde(default)]
    pub min_size: Option<usize>,
    /// Number of fan-in/out hotspots to report (default 10, hard cap 30).
    #[serde(default)]
    pub top_hotspots: Option<usize>,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

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
        Ok(text_result(r))
    }

    #[tool(
        description = "(Re)build only the persisted code graph at <repo>/.project-graph.json (call/import/inheritance/containment edges). Returns node/edge counts and resolution stats."
    )]
    async fn build_graph(
        &self,
        Parameters(params): Parameters<BuildGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || run_build_graph(params)).await;
        Ok(text_result(r))
    }

    #[tool(
        description = "Fuzzy-search symbol names in the code graph. Returns matching nodes with id, kind, and file:line."
    )]
    async fn search_symbols(
        &self,
        Parameters(p): Parameters<SearchSymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, String>(h.graph.search(
                &p.query,
                p.kind.as_deref(),
                p.language.as_deref(),
                p.limit,
            ))
        })
        .await;
        Ok(json_result(r))
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
            Ok::<_, String>(h.graph.get_symbol(p.id.as_deref(), p.name.as_deref()))
        })
        .await;
        Ok(json_result(r))
    }

    #[tool(
        description = "1-hop neighbors of a node, filterable by direction (out/in/both) and edge kind. Covers callers, callees, importers, implementers."
    )]
    async fn get_neighbors(
        &self,
        Parameters(p): Parameters<GetNeighborsParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, String>(h.graph.neighbors(
                &p.id,
                p.direction.as_deref(),
                p.edge_kind.as_deref(),
                p.limit,
            ))
        })
        .await;
        Ok(json_result(r))
    }

    #[tool(
        description = "Shortest path between two symbols over the chosen edge kinds (default: calls). BFS, depth-capped."
    )]
    async fn call_path(
        &self,
        Parameters(p): Parameters<CallPathParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, String>(
                h.graph
                    .call_path(&p.from, &p.to, p.edge_kinds.as_deref(), p.max_depth),
            )
        })
        .await;
        Ok(json_result(r))
    }

    #[tool(
        description = "What depends on X: the transitive set of symbols/files that directly or indirectly call or import a symbol or file (reverse reachability), depth-capped and file-grouped by default."
    )]
    async fn impact_of(
        &self,
        Parameters(p): Parameters<ImpactParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(move || {
            let h = graph_cache::get_or_load(&p.repo_path)?;
            Ok::<_, String>(h.graph.impact(
                &p.id,
                p.edge_kinds.as_deref(),
                p.max_depth,
                p.limit,
                p.group_by_file,
            ))
        })
        .await;
        Ok(json_result(r))
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
            Ok::<_, String>(h.graph.list_modules(p.min_size, p.top_hotspots))
        })
        .await;
        Ok(json_result(r))
    }
}

// ---------------------------------------------------------------------------
// Tool implementations / response helpers
// ---------------------------------------------------------------------------

fn run_refresh(params: RefreshParams) -> Result<String, String> {
    let repo = PathBuf::from(&params.repo_path)
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {}", params.repo_path, e))?;
    let output = params
        .output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join(".project-description.md"));

    let head_hash = git_head(&repo).ok();
    if params.skip_if_unchanged.unwrap_or(false)
        && let Some(current) = &head_hash
        && let Ok(existing) = std::fs::read_to_string(&output)
        && front_matter_head(&existing).as_deref() == Some(current.as_str())
    {
        return Ok(format!("skipped (HEAD {}): {}", current, output.display()));
    }

    let extractor = Extractor::new().map_err(|e| e.to_string())?;
    let extracted = extractor.extract_repo(&repo);
    let symbols = &extracted.symbols;

    let mut opts = RenderOptions::new(repo.clone());
    if let Some(kib) = params.size_threshold_kib {
        opts.size_threshold_bytes = kib * 1024;
    }
    opts.head_hash = head_hash.clone();
    let body = render(symbols, &opts);

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&output, &body).map_err(|e| format!("writing {}: {}", output.display(), e))?;
    let _ = gitignore::ensure_entry(&repo, &output);

    // Also refresh the code graph alongside the markdown.
    let g = graph::build_graph(&extracted, &repo, head_hash);
    let gpath = repo.join(".project-graph.json");
    let graph_note = match g.to_json() {
        Ok(json) => match std::fs::write(&gpath, json) {
            Ok(()) => {
                let _ = gitignore::ensure_entry(&repo, &gpath);
                format!(
                    "; graph {} ({} nodes, {} edges)",
                    gpath.display(),
                    g.stats.nodes,
                    g.stats.edges
                )
            }
            Err(e) => format!("; graph write failed: {e}"),
        },
        Err(e) => format!("; graph serialize failed: {e}"),
    };

    Ok(format!(
        "wrote {} ({} symbols, {} bytes){}",
        output.display(),
        symbols.len(),
        body.len(),
        graph_note
    ))
}

fn run_build_graph(params: BuildGraphParams) -> Result<String, String> {
    let repo = PathBuf::from(&params.repo_path)
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {}", params.repo_path, e))?;
    let output = params.output_path.map(PathBuf::from);
    let (g, path) = graph_cache::build_and_write(&repo, output.as_deref())?;
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

/// Wrap a `spawn_blocking` text result into a `CallToolResult`.
fn text_result(r: Result<Result<String, String>, tokio::task::JoinError>) -> CallToolResult {
    match r {
        Ok(Ok(msg)) => CallToolResult::success(vec![Content::text(msg)]),
        Ok(Err(e)) => CallToolResult::error(vec![Content::text(e)]),
        Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
    }
}

/// Wrap a `spawn_blocking` serializable result into a `CallToolResult` (compact JSON).
fn json_result<T: Serialize>(
    r: Result<Result<T, String>, tokio::task::JoinError>,
) -> CallToolResult {
    match r {
        Ok(Ok(v)) => match serde_json::to_string(&v) {
            Ok(s) => CallToolResult::success(vec![Content::text(s)]),
            Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
        },
        Ok(Err(e)) => CallToolResult::error(vec![Content::text(e)]),
        Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
    }
}

fn git_head(repo: &Path) -> std::io::Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other("git rev-parse HEAD failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn front_matter_head(body: &str) -> Option<String> {
    let mut lines = body.lines();
    if lines.next()? != "---" {
        return None;
    }
    for line in lines {
        if line == "---" {
            return None;
        }
        if let Some(rest) = line.strip_prefix("head: ") {
            return Some(rest.trim().to_string());
        }
    }
    None
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let server = TreeExtractServer::new();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
