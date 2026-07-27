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
