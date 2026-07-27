use crate::models::{DumpRecipe, RunResult};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// mise config filenames that trigger `mise exec` wrapping when found.
const MISE_MARKERS: &[&str] = &[
    "mise.toml",
    ".mise.toml",
    "mise.local.toml",
    ".mise.local.toml",
    ".tool-versions",
];

/// Walk up from `start` looking for a mise config file. Returns the directory
/// containing it, so `mise exec --cd <dir>` picks up the right environment.
fn find_mise_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(current) = dir {
        for marker in MISE_MARKERS {
            if current.join(marker).is_file() {
                return Some(current.to_path_buf());
            }
        }
        // Nested layout: <dir>/.config/mise/config.toml
        if current.join(".config/mise/config.toml").is_file() {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
}

/// Convert a single JSON scalar into an argument string. Strings pass through
/// verbatim; numbers and booleans use their literal form.
fn scalar_to_string(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Ok(String::new()),
        other => Err(format!("unsupported argument value: {other}")),
    }
}

/// Build positional arguments for a recipe from a name→value map, honouring the
/// recipe's declared parameter order. Because just passes arguments
/// positionally, an omitted optional parameter stops consumption — any later
/// parameter set in the map is then an error.
pub fn positional_from_map(
    recipe: &DumpRecipe,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut stopped = false;

    for param in &recipe.parameters {
        match map.get(&param.name) {
            Some(value) => {
                if stopped {
                    return Err(format!(
                        "cannot set '{}': an earlier optional parameter was omitted, \
                         and just arguments are positional",
                        param.name
                    ));
                }
                if param.is_variadic() {
                    match value {
                        serde_json::Value::Array(items) => {
                            for item in items {
                                result.push(scalar_to_string(item)?);
                            }
                        }
                        other => result.push(scalar_to_string(other)?),
                    }
                } else {
                    result.push(scalar_to_string(value)?);
                }
            }
            None => {
                if param.is_required() {
                    return Err(format!("missing required parameter '{}'", param.name));
                }
                stopped = true;
            }
        }
    }
    Ok(result)
}

/// Assemble the full argv for a recipe invocation.
fn build_command(
    justfile: &Path,
    dir: &Path,
    recipe: &str,
    args: &[String],
    dry_run: bool,
    use_mise: bool,
) -> (Vec<String>, bool) {
    let mut just_args: Vec<String> =
        vec!["--justfile".into(), justfile.to_string_lossy().into_owned()];
    if dry_run {
        just_args.push("--dry-run".into());
    }
    just_args.push(recipe.to_string());
    just_args.extend(args.iter().cloned());

    let mise_dir = if use_mise { find_mise_dir(dir) } else { None };

    match mise_dir {
        Some(md) => {
            let mut argv = vec![
                "mise".into(),
                "exec".into(),
                "--cd".into(),
                md.to_string_lossy().into_owned(),
                "--".into(),
                "just".into(),
            ];
            argv.extend(just_args);
            (argv, true)
        }
        None => {
            let mut argv = vec!["just".into()];
            argv.extend(just_args);
            (argv, false)
        }
    }
}

/// Execute a recipe and capture its output, killing it after `timeout`.
pub async fn run_recipe(
    justfile: &Path,
    dir: &Path,
    recipe: &str,
    args: &[String],
    dry_run: bool,
    use_mise: bool,
    timeout: Duration,
) -> Result<RunResult, String> {
    let (argv, mise_wrapped) = build_command(justfile, dir, recipe, args, dry_run, use_mise);

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let started = Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn `{}`: {e}", argv[0]))?;

    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");

    // Drain pipes concurrently so large output cannot deadlock the child.
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf).await;
        buf
    });

    let (exit_code, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => (status.code(), false),
        Ok(Err(e)) => return Err(format!("error waiting for recipe: {e}")),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            (None, true)
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).into_owned();

    Ok(RunResult {
        justfile: justfile.to_string_lossy().into_owned(),
        recipe: recipe.to_string(),
        command: argv,
        cwd: dir.to_string_lossy().into_owned(),
        dry_run,
        mise_wrapped,
        exit_code,
        timed_out,
        duration_ms: started.elapsed().as_millis(),
        stdout,
        stderr,
    })
}

/// Fetch the source of a single recipe via `just --show`.
pub async fn show_recipe(justfile: &Path, recipe: &str) -> Result<String, String> {
    let output = Command::new("just")
        .arg("--justfile")
        .arg(justfile)
        .arg("--show")
        .arg(recipe)
        .output()
        .await
        .map_err(|e| format!("failed to invoke `just`: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "`just --show {recipe}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
