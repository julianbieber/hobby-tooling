//! The arguments each tool accepts, as the MCP client sends them, and their
//! parsing into what the server acts on.

use rmcp::schemars;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ProjectsParams {
    /// Directory to search under. Defaults to the configured root, or the
    /// directory the server was started in (the parent of your projects).
    pub root: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecipesParams {
    /// Path to a justfile, or a directory containing one.
    pub justfile: String,
    /// Include private recipes (leading `_` or `[private]`). Default false.
    #[serde(default)]
    pub include_private: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowParams {
    /// Path to a justfile, or a directory containing one.
    pub justfile: String,
    /// Recipe name or namepath (e.g. `build` or `frontend::build`).
    pub recipe: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunParams {
    /// Path to a justfile, or a directory containing one.
    pub justfile: String,
    /// Recipe name or namepath (e.g. `build` or `frontend::build`).
    pub recipe: String,
    /// Arguments keyed by parameter name. Values are strings (numbers/booleans
    /// accepted); a variadic parameter (`*args`/`+args`) takes an array of
    /// strings. Mutually exclusive with `positional`.
    #[serde(default)]
    pub args: Option<serde_json::Value>,
    /// Positional arguments passed in order, bypassing name resolution.
    /// Mutually exclusive with `args`.
    #[serde(default)]
    pub positional: Option<Vec<String>>,
    /// Print the commands just would run without executing them (`just -n`).
    #[serde(default)]
    pub dry_run: bool,
    /// Kill the recipe after this many seconds. Defaults to the configured
    /// timeout (300s).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// How a recipe's arguments were supplied.
#[derive(Debug)]
pub enum Arguments {
    None,
    Named(serde_json::Map<String, serde_json::Value>),
    Positional(Vec<String>),
}

/// The `just_run` arguments are inconsistent with each other.
#[derive(Debug, thiserror::Error)]
pub enum ArgumentsError {
    #[error("provide either `args` or `positional`, not both")]
    Both,
    #[error("`args` must be an object of parameter name to value")]
    NotAnObject,
}

/// A `just_run` call whose arguments are known to be consistent.
#[derive(Debug)]
pub struct RunRequest {
    pub justfile: String,
    pub recipe: String,
    pub arguments: Arguments,
    pub dry_run: bool,
    pub timeout_secs: Option<u64>,
}

impl TryFrom<RunParams> for RunRequest {
    type Error = ArgumentsError;

    fn try_from(params: RunParams) -> Result<Self, Self::Error> {
        let arguments = match (params.args, params.positional) {
            (Some(_), Some(_)) => return Err(ArgumentsError::Both),
            (Some(serde_json::Value::Object(map)), None) => Arguments::Named(map),
            (Some(_), None) => return Err(ArgumentsError::NotAnObject),
            (None, Some(list)) => Arguments::Positional(list),
            (None, None) => Arguments::None,
        };
        Ok(RunRequest {
            justfile: params.justfile,
            recipe: params.recipe,
            arguments,
            dry_run: params.dry_run,
            timeout_secs: params.timeout_secs,
        })
    }
}
