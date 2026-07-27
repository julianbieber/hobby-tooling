use crate::config::Config;
use crate::discover;
use crate::dump;
use crate::models::{ProjectEntry, ProjectsResult, RecipesResult, RunResult, ShowResult};
use crate::params::*;
use crate::run;

use std::path::Path;
use std::time::Duration;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

/// Serialize a success value as pretty JSON, or surface the error string to the
/// client as a tool error (rather than an opaque protocol error).
fn respond<T: serde::Serialize>(result: Result<T, String>) -> Result<CallToolResult, McpError> {
    match result {
        Ok(value) => {
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            Ok(CallToolResult::success(vec![Content::text(json)]))
        }
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
    }
}

#[derive(Clone)]
pub struct JustMcpServer {
    config: Config,
    tool_router: ToolRouter<Self>,
}

impl Default for JustMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl JustMcpServer {
    pub fn new() -> Self {
        Self {
            config: Config::load(),
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
    async fn discover_projects(&self, params: ProjectsParams) -> Result<ProjectsResult, String> {
        let root = discover::resolve_root(params.root.as_deref(), &self.config.roots)?;
        let justfiles = discover::discover(&root, &self.config.ignore, self.config.max_depth);

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
                    error: Some(e),
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

    async fn list_recipes(&self, params: RecipesParams) -> Result<RecipesResult, String> {
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

    async fn show(&self, params: ShowParams) -> Result<ShowResult, String> {
        let justfile = discover::resolve_justfile(Path::new(&params.justfile))?;
        let source = run::show_recipe(&justfile, &params.recipe).await?;
        Ok(ShowResult {
            justfile: justfile.to_string_lossy().into_owned(),
            recipe: params.recipe,
            source,
        })
    }

    async fn run(&self, params: RunParams) -> Result<RunResult, String> {
        if params.args.is_some() && params.positional.is_some() {
            return Err("provide either `args` or `positional`, not both".into());
        }

        let justfile = discover::resolve_justfile(Path::new(&params.justfile))?;
        let dir = justfile
            .parent()
            .ok_or_else(|| "justfile has no parent directory".to_string())?
            .to_path_buf();

        let d = dump::dump(&justfile).await?;

        // Resolve positional arguments.
        let positional = if let Some(list) = params.positional {
            list
        } else if let Some(args) = params.args {
            let map = args
                .as_object()
                .ok_or_else(|| "`args` must be an object of parameter name to value".to_string())?;
            match dump::find_recipe(&d, &params.recipe) {
                Some(recipe) => run::positional_from_map(recipe, map)?,
                None if map.is_empty() => Vec::new(),
                None => {
                    return Err(format!(
                        "recipe '{}' not found; cannot map named args. Use `positional` instead.",
                        params.recipe
                    ));
                }
            }
        } else {
            Vec::new()
        };

        let timeout = Duration::from_secs(
            params
                .timeout_secs
                .unwrap_or(self.config.default_timeout_secs),
        );

        run::run_recipe(
            &justfile,
            &dir,
            &params.recipe,
            &positional,
            params.dry_run,
            self.config.use_mise,
            timeout,
        )
        .await
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
