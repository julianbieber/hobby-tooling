//! What a repository's git checkout says about the state of its sources.

use std::path::Path;
use std::process::Command;

/// The commit `HEAD` points at, or `None` when `repo` is not a git checkout,
/// has no commits, or `git` cannot be run.
pub fn head(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
