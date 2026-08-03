//! File-path resolution for source linkage — shared by the LSP's
//! goto-into-real-source and the CLI's `--lint` filesystem checks.
//!
//! An MML module's `@ path` and the header `source.paths` locate the real code
//! a spec describes. goto-definition on one of those resolves it to a file on
//! disk; the linter warns when it cannot. The logic is ported from
//! `julian-path-lsp`: tilde expansion, an optional `:line:col` suffix, and
//! resolution of relative paths against an ordered list of search directories —
//! the spec's declared `base:` first, then the `source.repo` checkout roots
//! (see [`repo_dirs`]), then the spec file's directory, then the workspace
//! root.

use std::path::{Path, PathBuf};

/// Split a `path:line:col` token into its path and optional 1-based line/column.
/// Trailing all-digit `:`-segments (at most two) are treated as a position;
/// everything else stays part of the path. A bare trailing `:` is trimmed.
pub fn split_suffix(token: &str) -> (&str, Option<u32>, Option<u32>) {
    let parts: Vec<&str> = token.split(':').collect();
    let mut nums: Vec<u32> = Vec::new();
    let mut end = parts.len();
    while end > 1 && nums.len() < 2 {
        match parts[end - 1].parse::<u32>() {
            Ok(n) => {
                nums.push(n);
                end -= 1;
            }
            Err(_) => break,
        }
    }
    let consumed: usize = parts[end..].iter().map(|s| s.len() + 1).sum();
    let path = token[..token.len() - consumed].trim_end_matches(':');

    nums.reverse(); // collected right-to-left → [line, col?]
    (path, nums.first().copied(), nums.get(1).copied())
}

pub fn expand_tilde_with(s: &str, home: &str) -> String {
    if s == "~" {
        home.to_string()
    } else if let Some(rest) = s.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        s.to_string()
    }
}

fn expand_tilde(s: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) => expand_tilde_with(s, &home),
        Err(_) => s.to_string(),
    }
}

/// Turn a declared `base:` value into a directory to resolve against. Tilde is
/// expanded; an absolute base is used as-is; a relative base is resolved against
/// the current file's directory then the workspace root.
pub fn base_dir(
    raw: &str,
    current_dir: Option<&Path>,
    workspace: Option<&Path>,
) -> Option<PathBuf> {
    let expanded = expand_tilde(raw);
    let p = Path::new(&expanded);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    current_dir
        .map(|d| d.join(p))
        .or_else(|| workspace.map(|w| w.join(p)))
}

/// Resolve `raw` to the first existing path. Absolute (and tilde) paths are used
/// as-is; a relative path is tried against each of `search_dirs` in order.
pub fn resolve(raw: &str, search_dirs: &[&Path]) -> Option<PathBuf> {
    let expanded = expand_tilde(raw);
    let p = Path::new(&expanded);
    let candidates: Vec<PathBuf> = if p.is_absolute() {
        vec![p.to_path_buf()]
    } else {
        search_dirs.iter().map(|d| d.join(p)).collect()
    };
    candidates
        .into_iter()
        .find(|c| c.exists())
        .and_then(|c| c.canonicalize().ok())
}

/// The directories a spec's source paths resolve against, in order: the
/// declared `base:` first, then the `source.repo` checkout roots, then the spec
/// file's own directory, then the workspace root.
///
/// This is the one definition of that order. It backs goto-into-real-source,
/// the `unresolved-source-path` lint, and the Rust-reference checks
/// (grammar §4.7) alike — three consumers that must agree, or goto silently
/// no-ops on a path the linter called fine.
pub fn source_search_dirs(
    doc: &crate::ast::Document,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Vec<PathBuf> {
    let current_dir = current_file.parent().map(Path::to_path_buf);
    let base = doc
        .header
        .base
        .as_ref()
        .and_then(|b| base_dir(&b.value, current_dir.as_deref(), workspace_root));
    let repo_dirs = doc
        .header
        .source
        .as_ref()
        .and_then(|s| s.repo.as_ref())
        .map(|r| repo_dirs(&r.value, current_file, workspace_root))
        .unwrap_or_default();
    base.into_iter()
        .chain(repo_dirs)
        .chain(current_dir)
        .chain(workspace_root.map(Path::to_path_buf))
        .collect()
}

/// Existing checkout directories for the header's `source.repo`, in resolution
/// order. `source.paths` are relative to the repo checkout root (linkage.md).
///
/// Hobby layout: the spec lives at `.../notes/{project}/{topic}/{task}/<unit>.hmml`
/// and the matching checkout is the task worktree itself,
/// `~/fun_worktrees/{project}/{topic}/{task}` — a hobby task is one repo, so
/// there is **no repo level** under the task. Resolution order is that worktree,
/// then the workspace root, then the main checkout `~/fun_repos/<repo>`.
pub fn repo_dirs(repo: &str, current_file: &Path, workspace_root: Option<&Path>) -> Vec<PathBuf> {
    repo_dirs_with(
        repo,
        current_file,
        workspace_root,
        std::env::var("HOME").ok().as_deref(),
    )
}

/// The task worktree for the open spec: `{home}/fun_worktrees/{project}/{topic}/{task}`,
/// where the spec is at `.../{project}/{topic}/{task}/<unit>.hmml` (task = parent,
/// topic = grandparent, project = great-grandparent).
///
/// `repo` is deliberately unused: the worktree *is* the repo checkout, so the
/// path-derived project wins over the header's `source.repo`. If the two
/// disagree, the on-disk task is still where this task's code lives, and the
/// `~/fun_repos/<repo>` fallback below covers the header's value.
fn worktree_dir(current_file: &Path, home: Option<&str>) -> Option<PathBuf> {
    let home = home?;
    let task_dir = current_file.parent()?;
    let task = task_dir.file_name()?.to_str()?;
    let topic_dir = task_dir.parent()?;
    let topic = topic_dir.file_name()?.to_str()?;
    let project = topic_dir.parent()?.file_name()?.to_str()?;
    Some(
        Path::new(home)
            .join("fun_worktrees")
            .join(project)
            .join(topic)
            .join(task),
    )
}

fn repo_dirs_with(
    repo: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
    home: Option<&str>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let candidates = [
        worktree_dir(current_file, home),
        workspace_root.map(|w| w.join(repo)),
        home.map(|h| Path::new(h).join("fun_repos").join(repo)),
    ];
    for dir in candidates.into_iter().flatten() {
        if dir.is_dir() && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

// ---- cross-spec references (grammar §5.4) -----------------------------------

/// The namespace root for a spec file's cross-spec refs — its own directory.
/// Specs for one task are flat siblings in `.../notes/{project}/{topic}/{task}/`, so a
/// ref addresses a `.hmml` in the same directory as the open file.
pub fn spec_root(current_file: &Path) -> Option<PathBuf> {
    current_file.parent().map(Path::to_path_buf)
}

/// Resolve an `@ spec` ref to a sibling `<unit>.hmml` in the open file's own
/// directory. A ref is a bare `<unit>`; a qualified `sys/unit` ref (deprecated)
/// has its `sys` part ignored and `unit` resolved in the same directory. Tries
/// the current file's directory first, then the workspace root. Returns the
/// target path plus whether it exists (the non-existing candidate is still
/// useful for diagnostics).
pub fn resolve_spec_ref(
    raw: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Option<(PathBuf, bool)> {
    let (_system, unit) = crate::ast::split_spec_ref(raw);
    let rel = format!("{unit}.{}", crate::SPEC_EXT);
    let mut first_candidate = None;
    for root in [
        spec_root(current_file),
        workspace_root.map(Path::to_path_buf),
    ]
    .into_iter()
    .flatten()
    {
        let candidate = root.join(&rel);
        if candidate.is_file() {
            return Some((candidate, true));
        }
        first_candidate.get_or_insert(candidate);
    }
    first_candidate.map(|c| (c, false))
}

/// The PascalCase type names (structs + enums) defined in the spec file at
/// `target`'s `## data` section — the set importable via `use` (grammar §4.5).
/// A missing/unreadable file yields an empty list.
pub fn defined_type_names(target: &Path) -> Vec<String> {
    let Ok(src) = std::fs::read_to_string(target) else {
        return Vec::new();
    };
    // Both dialects: `## data` types and the Bevy world sections alike, so a
    // `use enemy-waves: Wave` import resolves against a plugin's components.
    crate::parse(&src)
        .all_type_defs()
        .map(|t| t.name().value.clone())
        .collect()
}

/// The unit names of the spec files directly in `root` — the `.hmml` filename
/// stems in a task's flat spec directory. A unit is just the filename stem;
/// there is no system dimension.
pub fn list_spec_entries(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(files) = std::fs::read_dir(root) else {
        return out;
    };
    for file in files.flatten() {
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) == Some(crate::SPEC_EXT)
            && let Some(unit) = path.file_stem().and_then(|s| s.to_str())
            && !unit.starts_with('.')
        {
            out.push(unit.to_string());
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn suffix_line_and_col() {
        assert_eq!(
            split_suffix("src/chunk/mesher.rs:42:10"),
            ("src/chunk/mesher.rs", Some(42), Some(10))
        );
    }

    #[test]
    fn suffix_none() {
        assert_eq!(
            split_suffix("src/chunk/scheduler.rs"),
            ("src/chunk/scheduler.rs", None, None)
        );
    }

    #[test]
    fn suffix_trailing_colon_trimmed() {
        assert_eq!(split_suffix("a/b.rs:"), ("a/b.rs", None, None));
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(expand_tilde_with("~", "/home/me"), "/home/me");
        assert_eq!(
            expand_tilde_with("~/notes.txt", "/home/me"),
            "/home/me/notes.txt"
        );
        assert_eq!(expand_tilde_with("./rel", "/home/me"), "./rel");
    }

    #[test]
    fn resolve_first_matching_search_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.py"), "x").unwrap();
        let got = resolve("a.py", &[dir.path()]).unwrap();
        assert_eq!(got, dir.path().join("a.py").canonicalize().unwrap());
    }

    #[test]
    fn resolve_tries_search_dirs_in_order() {
        let base = tempfile::tempdir().unwrap();
        let cur = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        // Only the workspace has the file; base and current miss.
        fs::write(ws.path().join("a.py"), "x").unwrap();
        let got = resolve("a.py", &[base.path(), cur.path(), ws.path()]).unwrap();
        assert_eq!(got, ws.path().join("a.py").canonicalize().unwrap());
    }

    #[test]
    fn resolve_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve("nope.py", &[dir.path()]), None);
    }

    /// A flat task spec dir with sibling units `one`, `two`, `other`.
    fn spec_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for unit in ["one", "two", "other"] {
            fs::write(dir.path().join(format!("{unit}.hmml")), "x").unwrap();
        }
        dir
    }

    #[test]
    fn resolve_spec_ref_bare_and_qualified() {
        let dir = spec_tree();
        let current = dir.path().join("one.hmml");
        // Bare ref → sibling in the same directory.
        let (p, exists) = resolve_spec_ref("two", &current, None).unwrap();
        assert!(exists);
        assert_eq!(p, dir.path().join("two.hmml"));
        // Qualified ref (deprecated): the `sys` part is ignored, `unit`
        // resolved in the same directory.
        let (p, exists) = resolve_spec_ref("sys-b/other", &current, None).unwrap();
        assert!(exists);
        assert_eq!(p, dir.path().join("other.hmml"));
    }

    #[test]
    fn resolve_spec_ref_missing_reports_candidate() {
        let dir = spec_tree();
        let current = dir.path().join("one.hmml");
        let (p, exists) = resolve_spec_ref("nope", &current, None).unwrap();
        assert!(!exists);
        assert_eq!(p, dir.path().join("nope.hmml"));
    }

    #[test]
    fn resolve_spec_ref_falls_back_to_workspace_root() {
        let dir = spec_tree();
        // Current file's own directory holds no sibling `other.hmml`.
        let loose = tempfile::tempdir().unwrap();
        let current = loose.path().join("one.hmml");
        let (p, exists) = resolve_spec_ref("other", &current, Some(dir.path())).unwrap();
        assert!(exists);
        assert_eq!(p, dir.path().join("other.hmml"));
    }

    #[test]
    fn list_spec_entries_lists_units_in_dir() {
        let dir = spec_tree();
        // A dotfile-stemmed .hmml is skipped; subdirectories are not descended.
        fs::create_dir_all(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/nested.hmml"), "x").unwrap();
        let entries = list_spec_entries(dir.path());
        assert_eq!(
            entries,
            vec!["one".to_string(), "other".to_string(), "two".to_string()],
        );
    }

    /// A spec at `{home}/notes/{project}/{topic}/{task}/one.hmml` — the hobby
    /// three-level notes layout.
    fn notes_spec(home: &Path, project: &str, topic: &str, task: &str) -> PathBuf {
        let dir = home.join("notes").join(project).join(topic).join(task);
        fs::create_dir_all(&dir).unwrap();
        dir.join("one.hmml")
    }

    #[test]
    fn repo_dirs_finds_task_worktree() {
        // Spec at notes/{project}/{topic}/{task}; the checkout IS the worktree at
        // {home}/fun_worktrees/{project}/{topic}/{task} — no repo level below it.
        let home = tempfile::tempdir().unwrap();
        let current = notes_spec(home.path(), "bevy_survivor", "features", "enemy-waves");
        let wt = home
            .path()
            .join("fun_worktrees/bevy_survivor/features/enemy-waves");
        fs::create_dir_all(&wt).unwrap();
        let dirs = repo_dirs_with("bevy_survivor", &current, None, home.path().to_str());
        assert_eq!(dirs, vec![wt]);
    }

    #[test]
    fn repo_dirs_empty_when_nothing_exists() {
        // No worktree and no main checkout → nothing resolves.
        let home = tempfile::tempdir().unwrap();
        let current = notes_spec(home.path(), "ghost", "topic", "task");
        assert!(repo_dirs_with("ghost", &current, None, home.path().to_str()).is_empty());
    }

    #[test]
    fn repo_dirs_worktree_ignores_header_repo_mismatch() {
        // The worktree is found from the *path*, so a header `source.repo` that
        // disagrees with the project dir does not prevent resolution.
        let home = tempfile::tempdir().unwrap();
        let current = notes_spec(home.path(), "wusel", "perf", "chunk-loading");
        let wt = home.path().join("fun_worktrees/wusel/perf/chunk-loading");
        fs::create_dir_all(&wt).unwrap();
        let dirs = repo_dirs_with("some-other-name", &current, None, home.path().to_str());
        assert_eq!(dirs, vec![wt]);
    }

    #[test]
    fn repo_dirs_workspace_root_and_dedup() {
        let home = tempfile::tempdir().unwrap();
        let current = notes_spec(home.path(), "wusel", "perf", "chunk-loading");
        let wt = home.path().join("fun_worktrees/wusel/perf/chunk-loading");
        fs::create_dir_all(&wt).unwrap();
        // A distinct workspace root contributes its own checkout, after the worktree.
        let ws = tempfile::tempdir().unwrap();
        fs::create_dir_all(ws.path().join("wusel")).unwrap();
        let dirs = repo_dirs_with("wusel", &current, Some(ws.path()), home.path().to_str());
        assert_eq!(dirs, vec![wt, ws.path().join("wusel")]);
    }

    #[test]
    fn repo_dirs_dedups_workspace_equal_to_worktree() {
        let home = tempfile::tempdir().unwrap();
        let current = notes_spec(home.path(), "wusel", "perf", "chunk-loading");
        let wt = home.path().join("fun_worktrees/wusel/perf/chunk-loading");
        fs::create_dir_all(&wt).unwrap();
        // Workspace root whose join(repo) lands on the worktree itself is deduped.
        let dirs = repo_dirs_with(
            "chunk-loading",
            &current,
            Some(&home.path().join("fun_worktrees/wusel/perf")),
            home.path().to_str(),
        );
        assert_eq!(dirs, vec![wt]);
    }

    #[test]
    fn repo_dirs_falls_back_to_fun_repos() {
        let home = tempfile::tempdir().unwrap();
        let current = notes_spec(home.path(), "wusel", "perf", "chunk-loading");
        fs::create_dir_all(home.path().join("fun_repos/wusel")).unwrap();
        let dirs = repo_dirs_with("wusel", &current, None, home.path().to_str());
        assert_eq!(dirs, vec![home.path().join("fun_repos/wusel")]);
    }

    #[test]
    fn base_dir_absolute_and_relative() {
        // Absolute base is used as-is.
        assert_eq!(
            base_dir("/opt/repo", Some(Path::new("/spec")), None),
            Some(PathBuf::from("/opt/repo"))
        );
        // Relative base resolves against the current file's directory.
        assert_eq!(
            base_dir("../repo", Some(Path::new("/specs/sub")), None),
            Some(PathBuf::from("/specs/sub/../repo"))
        );
    }
}
