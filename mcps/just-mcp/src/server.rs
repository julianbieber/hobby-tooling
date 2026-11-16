//! The MCP server: the tools it exposes and how each call is answered.

use crate::config::{Config, ConfigError};
use crate::discover::{self, ResolveError};
use crate::dump::{self, JustError};
use crate::models::{ProjectEntry, ProjectsResult, RecipesResult, RunResult, ShowResult};
use crate::params::*;
use crate::run::{self, ArgsError, RunError};

use std::path::Path;
use std::time::Duration;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

/// Serving the protocol could not start or ended abnormally.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("MCP handshake failed")]
    Initialize(#[source] Box<rmcp::service::ServerInitializeError>),
    #[error("MCP service task failed")]
    Service(#[from] tokio::task::JoinError),
}

/// Load the configuration and serve MCP on stdin/stdout until the client
/// disconnects.
pub async fn serve_stdio() -> Result<(), ServeError> {
    let config = Config::load()?;
    let service = JustMcpServer::new(config)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| ServeError::Initialize(Box::new(e)))?;
    service.waiting().await?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum ToolError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Just(#[from] JustError),
    #[error(transparent)]
    Arguments(#[from] ArgumentsError),
    #[error(transparent)]
    Args(#[from] ArgsError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("recipe '{0}' not found; cannot map named args. Use `positional` instead.")]
    UnknownRecipe(String),
    #[error("justfile has no parent directory")]
    NoParentDir,
    #[error("justfile discovery did not complete")]
    Discovery(#[source] tokio::task::JoinError),
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

fn respond<T: serde::Serialize>(result: Result<T, ToolError>) -> Result<CallToolResult, McpError> {
    match result {
        Ok(value) => {
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            Ok(CallToolResult::success(vec![Content::text(json)]))
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(error_chain(&e))])),
    }
}

#[derive(Clone)]
pub struct JustMcpServer {
    config: Config,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl JustMcpServer {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Discover every justfile beneath a root directory (defaults to where the server was started — the parent of your projects). Respects .gitignore and skips build/vendor dirs. Returns each justfile's path, working directory, recipe count and default recipe. Use this first to find which projects have just recipes, then call just_recipes on one."
    )]
    async fn just_projects(
        &self,
        Parameters(params): Parameters<ProjectsParams>,
    ) -> Result<CallToolResult, McpError> {
        respond(self.discover_projects(params).await)
    }

    #[tool(
        description = "List the recipes in one justfile, including recipes from its submodules. Returns each recipe's namepath (pass this to just_run), doc comment, parameters (name, required, default, variadic, help), group and dependencies, plus any aliases. Accepts a justfile path or a directory containing one. Private recipes are hidden unless include_private is set."
    )]
    async fn just_recipes(
        &self,
        Parameters(params): Parameters<RecipesParams>,
    ) -> Result<CallToolResult, McpError> {
        respond(self.list_recipes(params).await)
    }

    #[tool(
        description = "Show the source of a single recipe (its body and signature) via `just --show`. Useful to inspect exactly what a recipe will run before invoking it with just_run."
    )]
    async fn just_show(
        &self,
        Parameters(params): Parameters<ShowParams>,
    ) -> Result<CallToolResult, McpError> {
        respond(self.show(params).await)
    }

    #[tool(
        description = "Run a recipe with arguments and capture stdout, stderr, exit code and duration. Pass arguments either by parameter name via `args` (a variadic parameter takes an array) or positionally via `positional`. Runs with the justfile's directory as the working directory, wrapping in `mise exec` when a mise config is present so project tool versions activate. Use dry_run to preview the commands without executing. Recipes run arbitrary shell code."
    )]
    async fn just_run(
        &self,
        Parameters(params): Parameters<RunParams>,
    ) -> Result<CallToolResult, McpError> {
        respond(self.run(params).await)
    }
}

impl JustMcpServer {
    async fn discover_projects(&self, params: ProjectsParams) -> Result<ProjectsResult, ToolError> {
        let root = discover::resolve_root(params.root.as_deref(), &self.config.roots)?;
        let walk_root = root.clone();
        let ignore = self.config.ignore.clone();
        let max_depth = self.config.max_depth;
        let justfiles =
            tokio::task::spawn_blocking(move || discover::discover(&walk_root, &ignore, max_depth))
                .await
                .map_err(ToolError::Discovery)?;

        let mut projects = Vec::with_capacity(justfiles.len());
        for justfile in justfiles {
            let dir = justfile
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let entry = match dump::dump(&justfile).await {
                Ok(d) => ProjectEntry {
                    justfile: justfile.to_string_lossy().into_owned(),
                    dir,
                    recipe_count: dump::recipe_count(&d),
                    module_count: d.modules.len(),
                    default_recipe: d.first.clone(),
                    error: None,
                },
                Err(e) => ProjectEntry {
                    justfile: justfile.to_string_lossy().into_owned(),
                    dir,
                    recipe_count: 0,
                    module_count: 0,
                    default_recipe: None,
                    error: Some(error_chain(&e)),
                },
            };
            projects.push(entry);
        }

        Ok(ProjectsResult {
            root: root.to_string_lossy().into_owned(),
            count: projects.len(),
            projects,
        })
    }

    async fn list_recipes(&self, params: RecipesParams) -> Result<RecipesResult, ToolError> {
        let justfile = discover::resolve_justfile(Path::new(&params.justfile))?;
        let d = dump::dump(&justfile).await?;
        let recipes = dump::flatten_recipes(&d, params.include_private);
        let dir = justfile
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        Ok(RecipesResult {
            justfile: justfile.to_string_lossy().into_owned(),
            dir,
            default_recipe: d.first.clone(),
            recipe_count: recipes.len(),
            recipes,
            aliases: dump::aliases(&d),
        })
    }

    async fn show(&self, params: ShowParams) -> Result<ShowResult, ToolError> {
        let justfile = discover::resolve_justfile(Path::new(&params.justfile))?;
        let source = run::show_recipe(&justfile, &params.recipe).await?;
        Ok(ShowResult {
            justfile: justfile.to_string_lossy().into_owned(),
            recipe: params.recipe,
            source,
        })
    }

    async fn run(&self, params: RunParams) -> Result<RunResult, ToolError> {
        let request = RunRequest::try_from(params)?;

        let justfile = discover::resolve_justfile(Path::new(&request.justfile))?;
        let dir = justfile
            .parent()
            .ok_or(ToolError::NoParentDir)?
            .to_path_buf();

        let d = dump::dump(&justfile).await?;

        let positional = match request.arguments {
            Arguments::Positional(list) => list,
            Arguments::Named(map) => match dump::find_recipe(&d, &request.recipe) {
                Some(recipe) => run::positional_from_map(recipe, &map)?,
                None if map.is_empty() => Vec::new(),
                None => return Err(ToolError::UnknownRecipe(request.recipe)),
            },
            Arguments::None => Vec::new(),
        };

        let timeout = Duration::from_secs(
            request
                .timeout_secs
                .unwrap_or(self.config.default_timeout_secs),
        );

        Ok(run::run_recipe(
            &justfile,
            &dir,
            &request.recipe,
            &positional,
            request.dry_run,
            self.config.use_mise,
            timeout,
        )
        .await?)
    }
}

#[tool_handler]
impl ServerHandler for JustMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Discover and run `just` recipes across a multi-project tree. \
                 Start with just_projects to find justfiles, just_recipes to see a \
                 justfile's recipes and their parameters, just_show to inspect a \
                 recipe's source, and just_run to execute one."
                    .into(),
            ),
            ..Default::default()
        }
    }
}
