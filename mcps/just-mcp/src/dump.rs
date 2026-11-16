//! Introspecting a justfile through `just` itself, and flattening what it
//! reports into the shapes the tools return.

use crate::models::{AliasInfo, DumpFile, DumpParam, DumpRecipe, ParamInfo, RecipeInfo};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const INTROSPECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// `just` could not be asked about a justfile, or its answer was unusable.
#[derive(Debug, thiserror::Error)]
pub enum JustError {
    #[error("failed to invoke `just` (is it installed?)")]
    Spawn(#[source] std::io::Error),
    #[error("`just {action}` for {} did not finish within {}s", .justfile.display(), INTROSPECTION_TIMEOUT.as_secs())]
    TimedOut { action: String, justfile: PathBuf },
    #[error("`just {action}` failed for {}: {stderr}", .justfile.display())]
    Failed {
        action: String,
        justfile: PathBuf,
        stderr: String,
    },
    #[error("could not parse `just` JSON dump of {}", .justfile.display())]
    Parse {
        justfile: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Run `just --justfile <justfile> <args…>` and return its stdout, with
/// `action` naming the invocation in errors.
pub(crate) async fn introspect(
    justfile: &Path,
    args: &[&str],
    action: &str,
) -> Result<Vec<u8>, JustError> {
    let output = Command::new("just")
        .arg("--justfile")
        .arg(justfile)
        .args(args)
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(INTROSPECTION_TIMEOUT, output)
        .await
        .map_err(|_| JustError::TimedOut {
            action: action.to_string(),
            justfile: justfile.to_path_buf(),
        })?
        .map_err(JustError::Spawn)?;

    if !output.status.success() {
        return Err(JustError::Failed {
            action: action.to_string(),
            justfile: justfile.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
}

/// Run `just --justfile <path> --dump --dump-format json` and parse the result.
pub async fn dump(justfile: &Path) -> Result<DumpFile, JustError> {
    let stdout = introspect(justfile, &["--dump", "--dump-format", "json"], "--dump").await?;
    serde_json::from_slice(&stdout).map_err(|source| JustError::Parse {
        justfile: justfile.to_path_buf(),
        source,
    })
}

/// Best-effort extraction of a group name from a recipe's attributes.
fn group_of(recipe: &DumpRecipe) -> Option<String> {
    for attr in &recipe.attributes {
        if let Some(s) = attr.get("group").and_then(|g| g.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

/// Render a parameter default expression as a display string.
fn default_string(default: &Option<serde_json::Value>) -> Option<String> {
    match default {
        None => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// Best-effort human label for a recipe dependency.
fn dependency_label(dep: &serde_json::Value) -> String {
    if let Some(s) = dep.as_str() {
        return s.to_string();
    }
    if let Some(recipe) = dep.get("recipe") {
        if let Some(s) = recipe.as_str() {
            return s.to_string();
        }
        if let Some(s) = recipe.get("name").and_then(|n| n.as_str()) {
            return s.to_string();
        }
    }
    dep.to_string()
}

fn param_info(p: &DumpParam) -> ParamInfo {
    ParamInfo {
        name: p.name.clone(),
        required: p.is_required(),
        variadic: p.is_variadic(),
        default: default_string(&p.default),
        help: p.help.clone(),
    }
}

fn recipe_info(recipe: &DumpRecipe) -> RecipeInfo {
    let private = recipe.private || recipe.name.starts_with('_');
    RecipeInfo {
        name: recipe.name.clone(),
        namepath: recipe
            .namepath
            .clone()
            .unwrap_or_else(|| recipe.name.clone()),
        doc: recipe.doc.clone(),
        group: group_of(recipe),
        private,
        parameters: recipe.parameters.iter().map(param_info).collect(),
        dependencies: recipe.dependencies.iter().map(dependency_label).collect(),
    }
}

/// Flatten all recipes from a dump (including submodules) into a sorted list.
/// Private recipes are dropped unless `include_private` is set.
pub fn flatten_recipes(dump: &DumpFile, include_private: bool) -> Vec<RecipeInfo> {
    let mut out = Vec::new();
    collect_recipes(dump, include_private, &mut out);
    out.sort_by(|a, b| a.namepath.cmp(&b.namepath));
    out
}

fn collect_recipes(dump: &DumpFile, include_private: bool, out: &mut Vec<RecipeInfo>) {
    for recipe in dump.recipes.values() {
        let info = recipe_info(recipe);
        if include_private || !info.private {
            out.push(info);
        }
    }
    for module in dump.modules.values() {
        collect_recipes(module, include_private, out);
    }
}

/// Count recipes across a dump and all its submodules (used for `just_projects`).
pub fn recipe_count(dump: &DumpFile) -> usize {
    dump.recipes.len() + dump.modules.values().map(recipe_count).sum::<usize>()
}

pub fn aliases(dump: &DumpFile) -> Vec<AliasInfo> {
    let mut out: Vec<AliasInfo> = dump
        .aliases
        .values()
        .filter_map(|a| {
            a.target.as_ref().map(|t| AliasInfo {
                name: a.name.clone(),
                target: t.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Find a recipe by its namepath (or bare name) across the dump tree.
pub fn find_recipe<'a>(dump: &'a DumpFile, recipe: &str) -> Option<&'a DumpRecipe> {
    for r in dump.recipes.values() {
        let namepath = r.namepath.as_deref().unwrap_or(&r.name);
        if namepath == recipe || r.name == recipe {
            return Some(r);
        }
    }
    for module in dump.modules.values() {
        if let Some(found) = find_recipe(module, recipe) {
            return Some(found);
        }
    }
    None
}
