//! The data the server exchanges: the raw `just --dump --dump-format json`
//! model, read tolerantly so it survives `just` version drift, and the
//! responses the tools return.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct DumpFile {
    #[serde(default)]
    pub recipes: BTreeMap<String, DumpRecipe>,
    #[serde(default)]
    pub aliases: BTreeMap<String, DumpAlias>,
    /// Submodules declared with `mod`; each is itself a full dump.
    #[serde(default)]
    pub modules: BTreeMap<String, DumpFile>,
    /// Name of the default (first) recipe, if any.
    #[serde(default)]
    pub first: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DumpAlias {
    pub name: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DumpRecipe {
    pub name: String,
    #[serde(default)]
    pub namepath: Option<String>,
    #[serde(default)]
    pub doc: Option<String>,
    #[serde(default)]
    pub parameters: Vec<DumpParam>,
    #[serde(default)]
    pub dependencies: Vec<serde_json::Value>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub attributes: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct DumpParam {
    pub name: String,
    /// `null` for a required parameter; otherwise the default expression.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub kind: ParamKind,
    #[serde(default)]
    pub help: Option<String>,
}

/// How many arguments a parameter takes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    #[default]
    Singular,
    /// Zero or more (`*args`).
    Star,
    /// One or more (`+args`).
    Plus,
}

impl DumpParam {
    pub fn is_variadic(&self) -> bool {
        matches!(self.kind, ParamKind::Star | ParamKind::Plus)
    }

    /// Required parameters have no default and are not the zero-or-more `star`.
    pub fn is_required(&self) -> bool {
        self.default.is_none() && self.kind != ParamKind::Star
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectEntry {
    /// Absolute path to the justfile.
    pub justfile: String,
    /// Directory the justfile lives in (recipe working directory).
    pub dir: String,
    pub recipe_count: usize,
    pub module_count: usize,
    pub default_recipe: Option<String>,
    /// Set when the justfile could not be parsed by `just`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectsResult {
    pub root: String,
    pub count: usize,
    pub projects: Vec<ProjectEntry>,
}

#[derive(Debug, Serialize)]
pub struct ParamInfo {
    pub name: String,
    pub required: bool,
    pub variadic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecipeInfo {
    /// Bare recipe name.
    pub name: String,
    /// Fully-qualified name including module path (e.g. `frontend::build`).
    /// This is what `just_run` expects as its `recipe`.
    pub namepath: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub private: bool,
    pub parameters: Vec<ParamInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AliasInfo {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct RecipesResult {
    pub justfile: String,
    pub dir: String,
    pub default_recipe: Option<String>,
    pub recipe_count: usize,
    pub recipes: Vec<RecipeInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<AliasInfo>,
}

#[derive(Debug, Serialize)]
pub struct ShowResult {
    pub justfile: String,
    pub recipe: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct RunResult {
    pub justfile: String,
    pub recipe: String,
    /// The exact argv executed (program + arguments).
    pub command: Vec<String>,
    pub cwd: String,
    pub dry_run: bool,
    pub mise_wrapped: bool,
    /// Process exit code; `null` if the process was killed (e.g. timeout).
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
}
