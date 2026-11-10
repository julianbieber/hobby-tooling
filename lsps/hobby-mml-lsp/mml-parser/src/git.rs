//! Diff links against the repo checkout (grammar §8).
//!
//! The checkout is found from the delta's own path; every question about it
//! is answered by shelling out to `git`. Answers about a commit never change,
//! so [`GitCache`] keeps them for as long as it lives.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::ast::{Document, Side};
use crate::diagnostic::{Diagnostic, code};
use crate::parser::is_commit_id;

#[derive(Default)]
pub struct GitCache {
    has_commit: HashMap<(PathBuf, String), bool>,
    files: HashMap<(PathBuf, String), Option<Arc<Vec<String>>>>,
    diff: HashMap<(PathBuf, String, String), Arc<Vec<String>>>,
    lines: HashMap<(PathBuf, String, String), Option<usize>>,
}

fn git(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

fn git_text(dir: &Path, args: &[&str]) -> Option<String> {
    git(dir, args).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn count_lines(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let n = bytes.iter().filter(|&&b| b == b'\n').count();
    if bytes.ends_with(b"\n") { n } else { n + 1 }
}

impl GitCache {
    pub fn new() -> GitCache {
        GitCache::default()
    }

    pub fn has_commit(&mut self, dir: &Path, sha: &str) -> bool {
        let key = (dir.to_path_buf(), sha.to_owned());
        if let Some(&v) = self.has_commit.get(&key) {
            return v;
        }
        let found =
            dir.is_dir() && git(dir, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_some();
        if found {
            self.has_commit.insert(key, true);
        }
        found
    }

    pub fn files_at(&mut self, dir: &Path, sha: &str) -> Option<Arc<Vec<String>>> {
        let key = (dir.to_path_buf(), sha.to_owned());
        self.files
            .entry(key)
            .or_insert_with(|| {
                git_text(dir, &["ls-tree", "-r", "--name-only", sha])
                    .map(|s| Arc::new(s.lines().map(str::to_owned).collect()))
            })
            .clone()
    }

    pub fn diff_files(&mut self, dir: &Path, base: &str, head: &str) -> Arc<Vec<String>> {
        let key = (dir.to_path_buf(), base.to_owned(), head.to_owned());
        self.diff
            .entry(key)
            .or_insert_with(|| {
                let range = format!("{base}...{head}");
                Arc::new(
                    git_text(dir, &["diff", "--name-only", &range])
                        .map(|s| s.lines().map(str::to_owned).collect())
                        .unwrap_or_default(),
                )
            })
            .clone()
    }

    pub fn line_count(&mut self, dir: &Path, sha: &str, path: &str) -> Option<usize> {
        let key = (dir.to_path_buf(), sha.to_owned(), path.to_owned());
        *self.lines.entry(key).or_insert_with(|| {
            git(dir, &["show", &format!("{sha}:{path}")]).map(|b| count_lines(&b))
        })
    }
}

pub fn rev_parse(dir: &Path, rev: &str) -> Option<String> {
    git_text(dir, &["rev-parse", "--verify", "--quiet", rev]).map(|s| s.trim().to_owned())
}

pub fn is_ancestor(dir: &Path, a: &str, b: &str) -> bool {
    git(dir, &["merge-base", "--is-ancestor", a, b]).is_some()
}

/// `$HOME`, which the checkout candidates hang off.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Where a delta sits in the notes tree: `notes/{project}/{topic}/{task}/x.hmml`,
/// or `notes/{project}/{topic}/{task}/aspects/{aspect}/x.hmml` for an aspect's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesPlace {
    pub project: String,
    pub topic: String,
    pub task: String,
    pub aspect: Option<String>,
}

pub fn task_of(delta: &Path) -> Option<NotesPlace> {
    let name = |p: &Path| p.file_name().and_then(|n| n.to_str()).map(str::to_owned);
    let dir = delta.parent()?;
    let (task_dir, aspect) = match dir.parent() {
        Some(up) if name(up).as_deref() == Some("aspects") => (up.parent()?, Some(name(dir)?)),
        _ => (dir, None),
    };
    let topic_dir = task_dir.parent()?;
    let project_dir = topic_dir.parent()?;
    if name(project_dir.parent()?)? != "notes" {
        return None;
    }
    Some(NotesPlace {
        project: name(project_dir)?,
        topic: name(topic_dir)?,
        task: name(task_dir)?,
        aspect,
    })
}

/// The directories that may hold the checkout of `repo`, in lookup order.
pub fn checkout_candidates(delta: &Path, repo: &str, home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(place) = task_of(delta).filter(|p| p.project == repo) {
        let rel = Path::new(&place.project)
            .join(&place.topic)
            .join(&place.task);
        if let Some(aspect) = &place.aspect {
            out.push(home.join("fun_aspects").join(&rel).join(aspect));
        }
        out.push(home.join("fun_worktrees").join(&rel));
    }
    out.push(home.join("fun_repos").join(repo));
    out
}

pub fn find_checkout(
    cache: &mut GitCache,
    delta: &Path,
    repo: &str,
    head: &str,
    home: &Path,
) -> Option<PathBuf> {
    checkout_candidates(delta, repo, home)
        .into_iter()
        .find(|c| cache.has_commit(c, head))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Found(String),
    NotFound,
    Ambiguous(Vec<String>),
    MissingCommit,
}

pub fn suffix_matches(path: &str, file: &str) -> bool {
    let want: Vec<&str> = file.split('/').filter(|c| !c.is_empty()).collect();
    let have: Vec<&str> = path.split('/').collect();
    !want.is_empty() && have.len() >= want.len() && have[have.len() - want.len()..] == want[..]
}

/// Resolves `file` at the commit `sha`: among the files the diff touches
/// first, then among every file at that commit.
pub fn resolve_file(
    cache: &mut GitCache,
    checkout: &Path,
    base: &str,
    head: &str,
    sha: &str,
    file: &str,
) -> Resolution {
    let Some(all) = cache.files_at(checkout, sha) else {
        return Resolution::MissingCommit;
    };
    let diff = cache.diff_files(checkout, base, head);
    let pick = |pool: &mut dyn Iterator<Item = &String>| -> Vec<String> {
        pool.filter(|p| suffix_matches(p, file)).cloned().collect()
    };
    let in_diff: Vec<String> = pick(&mut diff.iter())
        .into_iter()
        .filter(|p| all.contains(p))
        .collect();
    let found = if in_diff.is_empty() {
        pick(&mut all.iter())
    } else {
        in_diff
    };
    match found.len() {
        0 => Resolution::NotFound,
        1 => Resolution::Found(found.into_iter().next().unwrap_or_default()),
        _ => Resolution::Ambiguous(found),
    }
}

/// The resolved checkout of a delta plus its two commits.
pub struct Target {
    pub checkout: PathBuf,
    pub base: Option<String>,
    pub head: String,
}

impl Target {
    pub fn sha(&self, side: Side) -> Option<&str> {
        match side {
            Side::Head => Some(&self.head),
            Side::Base => self.base.as_deref(),
        }
    }
}

pub fn target(cache: &mut GitCache, doc: &Document, delta: &Path, home: &Path) -> Option<Target> {
    let repo = doc.header.value("repo")?;
    let head = doc.header.value("head").filter(|h| is_commit_id(h))?;
    let base = doc
        .header
        .value("base")
        .filter(|b| is_commit_id(b))
        .map(str::to_owned);
    let checkout = find_checkout(cache, delta, repo, head, home)?;
    Some(Target {
        checkout,
        base,
        head: head.to_owned(),
    })
}

/// The grammar §8 checks: `checkout-not-found`, `stale-delta` and one
/// `unresolved-diff-link` per diff link that does not resolve.
pub fn check_links(
    cache: &mut GitCache,
    doc: &Document,
    delta: &Path,
    home: &Path,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let (Some(repo), Some(head)) = (doc.header.get("repo"), doc.header.value("head")) else {
        return out;
    };
    if !is_commit_id(head) {
        return out;
    }
    let Some(t) = target(cache, doc, delta, home) else {
        let tried: Vec<String> = checkout_candidates(delta, &repo.value, home)
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        out.push(Diagnostic::warning(
            repo.span,
            code::CHECKOUT_NOT_FOUND,
            format!(
                "no checkout of `{}` holds commit {head}; tried {}",
                repo.value,
                tried.join(", ")
            ),
        ));
        return out;
    };
    if let Some(branch) = doc.header.get("branch") {
        let tip = rev_parse(
            &t.checkout,
            &format!("refs/heads/{}^{{commit}}", branch.value),
        )
        .or_else(|| {
            rev_parse(
                &t.checkout,
                &format!("refs/remotes/origin/{}^{{commit}}", branch.value),
            )
        });
        let head_full = rev_parse(&t.checkout, &format!("{head}^{{commit}}"));
        if let (Some(tip), Some(head_full)) = (tip, head_full)
            && tip != head_full
            && is_ancestor(&t.checkout, &head_full, &tip)
        {
            out.push(Diagnostic::info(
                branch.span,
                code::STALE_DELTA,
                format!(
                    "`{}` is at {} in {}, past head {head}; the delta may be out of date",
                    branch.value,
                    &tip[..10.min(tip.len())],
                    t.checkout.display()
                ),
            ));
        }
    }
    for (link, side) in doc.links() {
        let Some(sha) = t.sha(side) else { continue };
        let base = t.base.as_deref().unwrap_or(sha);
        let side_name = match side {
            Side::Head => "head",
            Side::Base => "base",
        };
        let file = &link.file.value;
        let msg = match resolve_file(cache, &t.checkout, base, &t.head, sha, file) {
            Resolution::Found(path) => match &link.line {
                None => None,
                Some(n) => match cache.line_count(&t.checkout, sha, &path) {
                    Some(count) if n.value >= 1 && (n.value as usize) <= count => None,
                    Some(count) => Some(format!(
                        "line {} is outside {path}, which has {count} lines at {side_name} {sha}",
                        n.value
                    )),
                    None => Some(format!("cannot read {path} at {side_name} {sha}")),
                },
            },
            Resolution::NotFound => Some(format!("`{file}` matches no file at {side_name} {sha}")),
            Resolution::Ambiguous(c) => Some(format!(
                "`{file}` matches {} files at {side_name} {sha}: {}",
                c.len(),
                c.join(", ")
            )),
            Resolution::MissingCommit => Some(format!(
                "{side_name} commit {sha} is not in {}",
                t.checkout.display()
            )),
        };
        if let Some(m) = msg {
            out.push(Diagnostic::warning(
                link.span,
                code::UNRESOLVED_DIFF_LINK,
                m,
            ));
        }
    }
    out
}

/// A file on disk holding `path` as it is at `sha`: the working file when the
/// checkout's `HEAD` is that commit, else a copy materialised under
/// `<cache_dir>/<full sha>/<path>`.
pub fn source_file(checkout: &Path, sha: &str, path: &str, cache_dir: &Path) -> Option<PathBuf> {
    let full = rev_parse(checkout, &format!("{sha}^{{commit}}"))?;
    if rev_parse(checkout, "HEAD").as_deref() == Some(full.as_str()) {
        let working = checkout.join(path);
        if working.is_file() {
            return Some(working);
        }
    }
    let target = cache_dir.join(&full).join(path);
    if target.is_file() {
        return Some(target);
    }
    let bytes = git(checkout, &["show", &format!("{full}:{path}")])?;
    std::fs::create_dir_all(target.parent()?).ok()?;
    std::fs::write(&target, bytes).ok()?;
    Some(target)
}
