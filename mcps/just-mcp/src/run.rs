//! Running a recipe: turning named arguments into just's positional ones,
//! assembling the command line (optionally under `mise exec`), and executing
//! it with a timeout.

use crate::dump::{JustError, introspect};
use crate::models::{DumpRecipe, RunResult};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

/// Named arguments that cannot be mapped onto the recipe's parameters.
#[derive(Debug, thiserror::Error)]
pub enum ArgsError {
    #[error("unsupported argument value: {0}")]
    UnsupportedValue(serde_json::Value),
    #[error(
        "cannot set '{0}': an earlier optional parameter was omitted, and just arguments are positional"
    )]
    AfterOmittedOptional(String),
    #[error("missing required parameter '{0}'")]
    MissingRequired(String),
}

/// The recipe process could not be run to completion.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("failed to spawn `{program}`")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("error waiting for recipe")]
    Wait(#[source] std::io::Error),
}

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
fn scalar_to_string(value: &serde_json::Value) -> Result<String, ArgsError> {
    match value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Ok(String::new()),
        other => Err(ArgsError::UnsupportedValue(other.clone())),
    }
}

/// Build positional arguments for a recipe from a name→value map, honouring the
/// recipe's declared parameter order. Because just passes arguments
/// positionally, an omitted optional parameter stops consumption — any later
/// parameter set in the map is then an error.
pub fn positional_from_map(
    recipe: &DumpRecipe,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<String>, ArgsError> {
    let mut result = Vec::new();
    let mut stopped = false;

    for param in &recipe.parameters {
        match map.get(&param.name) {
            Some(value) => {
                if stopped {
                    return Err(ArgsError::AfterOmittedOptional(param.name.clone()));
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
                    return Err(ArgsError::MissingRequired(param.name.clone()));
                }
                stopped = true;
            }
        }
    }
    Ok(result)
}

struct Invocation {
    program: String,
    args: Vec<String>,
    mise_wrapped: bool,
}

impl Invocation {
    fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

fn build_command(
    justfile: &Path,
    dir: &Path,
    recipe: &str,
    args: &[String],
    dry_run: bool,
    use_mise: bool,
) -> Invocation {
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
            let mut args = vec![
                "exec".into(),
                "--cd".into(),
                md.to_string_lossy().into_owned(),
                "--".into(),
                "just".into(),
            ];
            args.extend(just_args);
            Invocation {
                program: "mise".into(),
                args,
                mise_wrapped: true,
            }
        }
        None => Invocation {
            program: "just".into(),
            args: just_args,
            mise_wrapped: false,
        },
    }
}

#[expect(
    clippy::expect_used,
    reason = "stdout and stderr are set to piped on the command just before spawning"
)]
fn spawn_piped(cmd: &mut Command) -> std::io::Result<(Child, ChildStdout, ChildStderr)> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    Ok((child, stdout, stderr))
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
) -> Result<RunResult, RunError> {
    let invocation = build_command(justfile, dir, recipe, args, dry_run, use_mise);

    let mut cmd = Command::new(&invocation.program);
    cmd.args(&invocation.args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .kill_on_drop(true);

    let started = Instant::now();
    let (mut child, mut stdout_pipe, mut stderr_pipe) =
        spawn_piped(&mut cmd).map_err(|source| RunError::Spawn {
            program: invocation.program.clone(),
            source,
        })?;

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
        Ok(Err(e)) => return Err(RunError::Wait(e)),
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
        command: invocation.argv(),
        cwd: dir.to_string_lossy().into_owned(),
        dry_run,
        mise_wrapped: invocation.mise_wrapped,
        exit_code,
        timed_out,
        duration_ms: started.elapsed().as_millis(),
        stdout,
        stderr,
    })
}

/// Fetch the source of a single recipe via `just --show`.
pub async fn show_recipe(justfile: &Path, recipe: &str) -> Result<String, JustError> {
    let stdout = introspect(justfile, &["--show", recipe], &format!("--show {recipe}")).await?;
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}
