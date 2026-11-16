//! Shared helper for keeping generated artifacts (`.project-description.md`,
//! `.project-graph.json`) out of git via `<repo>/.git/info/exclude`.

use std::path::Path;

/// Add `path` to `<repo>/.git/info/exclude` if `path` is under `repo` and not
/// already excluded. No-op if `repo` isn't a git checkout.
pub fn ensure_entry(repo: &Path, path: &Path) -> std::io::Result<()> {
    match path.strip_prefix(repo) {
        Ok(rel) => ensure_entry_rel(repo, &rel.to_string_lossy()),
        Err(_) => Ok(()),
    }
}

/// As [`ensure_entry`], logging a failure as a warning rather than returning
/// it: an artifact that is written but not excluded is still usable.
pub fn ensure_entry_or_warn(repo: &Path, path: &Path) {
    if let Err(e) = ensure_entry(repo, path) {
        let error: &dyn std::error::Error = &e;
        tracing::warn!(error, path = %path.display(), "cannot add to the git excludes");
    }
}

/// As [`ensure_entry`], but takes the repo-relative path string directly.
pub fn ensure_entry_rel(repo: &Path, rel: &str) -> std::io::Result<()> {
    let exclude_path = repo.join(".git/info/exclude");
    if !exclude_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return Ok(()); // not a git repo (or worktree without .git/info)
    }
    let existing = std::fs::read_to_string(&exclude_path).unwrap_or_default();
    let needle = format!("/{rel}");
    if existing
        .lines()
        .any(|l| l.trim() == rel || l.trim() == needle)
    {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str("# added by hobby-tree-extract\n");
    updated.push_str(&format!("/{rel}\n"));
    std::fs::write(&exclude_path, updated)
}
