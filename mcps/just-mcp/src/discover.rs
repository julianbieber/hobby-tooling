//! Finding justfiles: walking a project tree for them and resolving a
//! user-supplied path or root to a concrete one.

use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A path the caller supplied does not lead to a justfile or a usable root.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("no justfile found in directory {}", .dir.display())]
    NoJustfileInDir { dir: PathBuf },
    #[error("path does not exist: {}", .path.display())]
    NotFound { path: PathBuf },
    #[error("cannot resolve {}", .path.display())]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read current dir")]
    CurrentDir(#[source] std::io::Error),
}

/// True if a filename is one just recognises as a justfile.
fn is_justfile_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower == "justfile" || lower == ".justfile"
}

/// Walk `root` collecting every justfile, respecting `.gitignore` and skipping
/// the configured `ignore_dirs`. Hidden entries are included (so `.justfile` is
/// found), but the ignore list still prunes dot-directories like `.git`.
pub fn discover(root: &Path, ignore_dirs: &[String], max_depth: usize) -> Vec<PathBuf> {
    let skip: HashSet<String> = ignore_dirs.iter().cloned().collect();

    let mut builder = WalkBuilder::new(root);
    builder
        .max_depth(Some(max_depth))
        .hidden(false)
        // Honour `.gitignore` files found within the tree (so already-ignored
        // build output is skipped) but ignore parent/global/exclude sources — a
        // broad rule like `*` in `~/.gitignore` would otherwise hide everything.
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .require_git(false)
        .filter_entry(move |entry| {
            // Prune ignored directories; always keep files so they can match.
            if entry.file_type().is_some_and(|t| t.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                return !skip.contains(name.as_ref());
            }
            true
        });

    let mut found = Vec::new();
    for entry in builder.build().flatten() {
        if entry.file_type().is_some_and(|t| t.is_file()) {
            let name = entry.file_name().to_string_lossy();
            if is_justfile_name(&name) {
                found.push(entry.into_path());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Resolve a user-supplied path to a concrete justfile. Accepts either a path
/// to a justfile directly, or a directory containing one.
pub fn resolve_justfile(path: &Path) -> Result<PathBuf, ResolveError> {
    if path.is_file() {
        return canonicalize(path);
    }
    if path.is_dir() {
        for candidate in ["justfile", ".justfile", "Justfile", "JUSTFILE"] {
            let p = path.join(candidate);
            if p.is_file() {
                return canonicalize(&p);
            }
        }
        return Err(ResolveError::NoJustfileInDir {
            dir: path.to_path_buf(),
        });
    }
    Err(ResolveError::NotFound {
        path: path.to_path_buf(),
    })
}

fn canonicalize(path: &Path) -> Result<PathBuf, ResolveError> {
    path.canonicalize()
        .map_err(|source| ResolveError::Canonicalize {
            path: path.to_path_buf(),
            source,
        })
}

/// Determine the discovery root: an explicit override, else the first
/// configured root, else the current working directory.
pub fn resolve_root(
    override_root: Option<&str>,
    configured: &[PathBuf],
) -> Result<PathBuf, ResolveError> {
    let base = match override_root {
        Some(r) => PathBuf::from(r),
        None => match configured.first() {
            Some(r) => r.clone(),
            None => std::env::current_dir().map_err(ResolveError::CurrentDir)?,
        },
    };
    canonicalize(&base)
}
