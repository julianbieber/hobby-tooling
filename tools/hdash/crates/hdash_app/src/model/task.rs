//! The task half of the model: the worktree/notes filesystem scan, git status reads, PR
//! status via `gh`, and the `task.json` done flag. No Bevy UI here.
//!
//! A task is a directory under `~/fun_worktrees/PROJECT/TOPIC/TASK`, or a notes directory
//! carrying a `task.json` for one that was removed but remembered. Unlike the pre-merge
//! `hwt-overview`, tasks are **not** grouped into topics here: an unstarted issue has a
//! topic *label* and no topic *directory*, so the merged list groups by project only and
//! carries the topic as a card label. See [`super::join`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use hwt_core::{days_from_civil, git_capture, parse_ymd};
use serde::{Deserialize, Serialize};

pub use hwt_core::{notes_root, worktrees_root};

/// A git status result for one repo, delivered from the async scan over a channel.
pub struct GitMsg {
    /// The task the repo belongs to.
    pub id: TaskId,
    /// Index of the repo within the task's `repos`.
    pub repo: usize,
    /// The freshly-read git status.
    pub info: GitInfo,
}

/// Live PR review state, mapped from `gh pr view`. Mirrors how `GitInfo.loaded` distinguishes the
/// pre-fetch state: `Loading` is "not queried yet", `Unknown` is "queried and failed".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum PrStatus {
    /// The async query has not completed yet.
    #[default]
    Loading,
    /// `OPEN` + draft.
    Draft,
    /// `OPEN` + not draft — i.e. waiting for review/approval (the key signal).
    Open,
    /// `MERGED`.
    Merged,
    /// `CLOSED` (and not merged).
    Closed,
    /// `gh` failed / not authenticated / not found / unparseable.
    Unknown,
}

impl PrStatus {
    /// A merged or closed PR is in a terminal state that can never change again, so its status is
    /// safe to persist in the on-disk cache and reuse without re-querying `gh`. Open/Draft can still
    /// transition and Loading/Unknown aren't real answers, so none of those are cacheable.
    pub fn is_terminal(self) -> bool {
        matches!(self, PrStatus::Merged | PrStatus::Closed)
    }
}

/// A pull request associated with a task. `url` and the optional metadata come from the task's
/// `prs.json`; `status`/`number`/`live_title` are populated asynchronously by `gh`.
#[derive(Clone, Debug, Deserialize)]
pub struct Pr {
    /// PR URL (the only guaranteed field; used as the `gh pr view` argument and the display fallback).
    pub url: String,
    /// `owner/repo` from the file, if present.
    #[serde(default)]
    pub repo: Option<String>,
    /// Title from the file, if present (fallback until the live title loads).
    #[serde(default)]
    pub title: Option<String>,
    /// `YYYY-MM-DD` the PR was opened, if present.
    #[serde(default)]
    pub opened: Option<String>,

    // ---- live fields, not in the file (filled by the async query) ----
    /// Live review state. `Loading` until the async query lands.
    #[serde(skip, default)]
    pub status: PrStatus,
    /// PR number from `gh` (for the `#123` label).
    #[serde(skip, default)]
    pub number: Option<u64>,
    /// Live title from `gh` (overrides the file title for display once loaded).
    #[serde(skip, default)]
    pub live_title: Option<String>,
}

impl Pr {
    /// Best available title: the live `gh` title, then the file title, else empty.
    pub fn display_title(&self) -> &str {
        self.live_title
            .as_deref()
            .or(self.title.as_deref())
            .unwrap_or("")
    }
}

/// A subtask owned by a task: a sibling worktree the task spun off (via `/spawn`). Declared in the
/// owning task's `subtasks.json`. `task` is the only required field; the rest is display metadata.
#[derive(Clone, Debug, Deserialize)]
pub struct SubtaskRef {
    /// The child task's directory/branch name (third path level, under its topic).
    pub task: String,
    /// The child's topic, if it differs from the owner's. Defaults to the owner's topic when absent.
    #[serde(default)]
    pub topic: Option<String>,
    /// Project, if recorded. A subtask always shares its parent's project, so this is
    /// optional and defaults to the owner's — `/spawn` writes it to keep the entry
    /// self-describing.
    #[serde(default)]
    pub project: Option<String>,
    /// Short blurb (the new-name suffix), if present.
    #[serde(default)]
    pub blurb: Option<String>,
    /// One-line goal/brief, if present.
    #[serde(default)]
    pub goal: Option<String>,
    /// `YYYY-MM-DD` the subtask was created, if present.
    #[serde(default)]
    pub created: Option<String>,
}

impl SubtaskRef {
    /// The child's stable id, resolving project and topic against the owner's when the
    /// entry does not record them.
    pub fn resolved_id(&self, owner_project: &str, owner_topic: &str) -> TaskId {
        TaskId {
            project: self
                .project
                .clone()
                .unwrap_or_else(|| owner_project.to_owned()),
            topic: self.topic.clone().unwrap_or_else(|| owner_topic.to_owned()),
            name: self.task.clone(),
        }
    }

    /// Best one-line label: the blurb or goal, else the task name.
    pub fn display_label(&self) -> &str {
        self.blurb
            .as_deref()
            .or(self.goal.as_deref())
            .unwrap_or(&self.task)
    }
}

/// A live PR status result, delivered from the async query over a channel. Mirrors [`GitMsg`].
pub struct PrMsg {
    /// The task the PR belongs to.
    pub id: TaskId,
    /// Index of the PR within the task's `prs`.
    pub pr: usize,
    /// Mapped live state.
    pub status: PrStatus,
    /// PR number from `gh`, if read.
    pub number: Option<u64>,
    /// PR title from `gh`, if read.
    pub title: Option<String>,
}

/// Stable identity for a task: (project, topic, task name) — the three directory
/// levels under `~/fun_worktrees`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TaskId {
    /// Project (first level) — a repo in `~/fun_repos`.
    pub project: String,
    /// Topic (second level).
    pub topic: String,
    /// Task name (third level). This directory is the git worktree itself.
    pub name: String,
}

impl TaskId {
    /// `project/topic/task` — the path-shaped form, used for notes lookups and labels.
    pub fn rel(&self) -> String {
        format!("{}/{}/{}", self.project, self.topic, self.name)
    }
}

/// Per-repo git status. `loaded` is false until the async scan fills it in.
#[derive(Clone, Default, Debug)]
pub struct GitInfo {
    /// Whether the async git scan has populated this entry.
    pub loaded: bool,
    /// Current branch (`None` if detached or unreadable).
    pub branch: Option<String>,
    /// Short HEAD SHA.
    pub head: Option<String>,
    /// Number of changed (porcelain) entries.
    pub dirty: usize,
}

/// A single git worktree (repo checkout) inside a task directory.
///
/// A hobby task is exactly one repo — the task directory itself — so this never holds
/// more than one entry. It stays a struct in a vector rather than collapsing into `Task`
/// because the async git-status plumbing addresses repos by index. There is no `name`:
/// a hobby repo is its project, and the merged card already prints the project.
#[derive(Clone, Debug)]
pub struct Repo {
    /// Absolute path to the repo.
    pub path: PathBuf,
    /// Git status (lazily populated).
    pub git: GitInfo,
}

/// A block: the task is waiting on an action and/or until a day. Deserialized from the `blocked`
/// object in `task.json` (`null`/absent means not blocked).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Block {
    /// `YYYY-MM-DD` the block lifts, if set (`None` = indefinite, waiting on an action).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Human-readable reason the task is blocked, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A task: a directory under a topic, possibly holding several repos.
#[derive(Clone, Debug)]
pub struct Task {
    /// Stable id.
    pub id: TaskId,
    /// Absolute path to the (intended) worktree task directory. May not exist if the worktree was
    /// removed but the metadata was kept — see [`Task::present`].
    pub dir: PathBuf,
    /// Whether the worktree task directory exists on disk. `false` for a removed-but-remembered
    /// task (its metadata still lives in the notes repo; use `hwt restore` to re-create it).
    pub present: bool,
    /// Whether the task is marked done in its `task.json`.
    pub done: bool,
    /// Block from `task.json`, if present.
    pub block: Option<Block>,
    /// Repos checked out under this task (may be empty).
    pub repos: Vec<Repo>,
    /// Pull requests declared in the task's `prs.json` (live status loaded async).
    pub prs: Vec<Pr>,
    /// Subtasks this task owns, declared in its `subtasks.json` (sibling worktrees spun off via `/spawn`).
    pub subtasks: Vec<SubtaskRef>,
    /// Notes directory/file last-modified time, if any.
    pub last_active: Option<SystemTime>,
}

impl Task {
    /// Aggregate dirty count across repos.
    pub fn dirty_total(&self) -> usize {
        self.repos.iter().map(|r| r.git.dirty).sum()
    }

    /// Number of PRs waiting for review/approval (open, non-draft).
    pub fn awaiting_review(&self) -> usize {
        self.prs
            .iter()
            .filter(|p| p.status == PrStatus::Open)
            .count()
    }

    /// Whether a block is recorded (regardless of its until-date).
    pub fn blocked(&self) -> bool {
        self.block.is_some()
    }

    /// Whether the block has lifted: a block with an until-date that is today or
    /// earlier. Indefinite blocks (no until-date) are never "ready".
    pub fn ready(&self) -> bool {
        let Some(block) = &self.block else {
            return false;
        };
        let Some(until) = &block.until else {
            return false;
        };
        let Some((y, m, d)) = parse_ymd(until) else {
            return false;
        };
        let today = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|s| (s.as_secs() / 86_400) as i64)
            .unwrap_or(0);
        today >= days_from_civil(y, m, d)
    }
}

// ---------------------------------------------------------------------------
// PR status cache
// ---------------------------------------------------------------------------

/// `~/.cache/hdash/pr-status.json` (honouring `XDG_CACHE_HOME`): the persisted terminal
/// PR-status cache.
fn pr_cache_path() -> PathBuf {
    hwt_core::xdg_cache_path("hdash", "pr-status.json")
}

/// The pre-merge `hwt-overview` cache location. Read once as a fallback so the rename does
/// not cost a `gh pr view` for every merged PR on first launch; never written, never deleted.
fn legacy_pr_cache_path() -> PathBuf {
    hwt_core::xdg_cache_path("hwt-overview", "pr-status.json")
}

/// A cached terminal PR status (merged/closed), keyed by PR URL in the cache file. Stores just
/// enough to render the PR line without calling `gh`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedPr {
    pub status: PrStatus,
    #[serde(default)]
    pub number: Option<u64>,
    #[serde(default)]
    pub title: Option<String>,
}

/// Load the terminal-PR-status cache (URL → status), falling back to `hwt-overview`'s old
/// path when this app has not written one yet. A missing or unparseable file is treated as
/// an empty cache, so a corrupt cache degrades to re-querying rather than breaking the app.
pub fn read_pr_cache() -> std::collections::HashMap<String, CachedPr> {
    let cache = read_pr_cache_at(&pr_cache_path());
    if cache.is_empty() {
        return read_pr_cache_at(&legacy_pr_cache_path());
    }
    cache
}

/// Persist the terminal-PR-status cache, creating `~/.cache/hdash/` if needed. Best-effort:
/// a write failure is logged and otherwise ignored (the cache is an optimisation, not a source of
/// truth).
pub fn write_pr_cache(cache: &std::collections::HashMap<String, CachedPr>) {
    write_pr_cache_at(&pr_cache_path(), cache);
}

/// Delete the on-disk terminal-PR-status cache so the next scan re-queries every PR. Best-effort;
/// a missing file is success. Only this app's file — the legacy one is left alone, but it is not
/// read once this app has written its own.
pub fn clear_pr_cache() {
    let path = pr_cache_path();
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!("failed to clear PR cache {}: {e}", path.display()),
    }
}

fn read_pr_cache_at(path: &Path) -> std::collections::HashMap<String, CachedPr> {
    let Ok(bytes) = std::fs::read(path) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_pr_cache_at(path: &Path, cache: &std::collections::HashMap<String, CachedPr>) {
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        warn!("failed to create PR cache dir {}: {e}", dir.display());
        return;
    }
    match serde_json::to_vec_pretty(cache) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                warn!("failed to write PR cache {}: {e}", path.display());
            }
        }
        Err(e) => warn!("failed to serialize PR cache: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Resolve a task's notes location: either `notes/PROJECT/TOPIC/TASK/` (dir) or
/// `notes/PROJECT/TOPIC/TASK.md`.
pub fn notes_path(id: &TaskId) -> Option<PathBuf> {
    let base = notes_root().join(&id.project).join(&id.topic);
    let dir = notes_root().join(id.rel());
    if dir.is_dir() {
        return Some(dir);
    }
    let file = base.join(format!("{}.md", id.name));
    if file.exists() {
        return Some(file);
    }
    None
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

fn is_visible_dir(entry: &std::fs::DirEntry) -> bool {
    entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.')
}

/// Collect `(project, topic) -> {task names}` from a three-level
/// `root/PROJECT/TOPIC/TASK` tree, keeping only task dirs that satisfy `keep`.
/// Merges into `out` so several roots can be unioned.
///
/// The key is the (project, topic) pair, not the topic alone: two projects can both
/// have a `features` topic and their tasks must not be merged.
fn collect_tasks(
    root: &Path,
    keep: impl Fn(&Path) -> bool,
    out: &mut BTreeMap<(String, String), BTreeSet<String>>,
) {
    let Ok(project_entries) = std::fs::read_dir(root) else {
        return;
    };
    for project_entry in project_entries.flatten() {
        if !is_visible_dir(&project_entry) {
            continue;
        }
        let project = project_entry.file_name().to_string_lossy().into_owned();
        let Ok(topic_entries) = std::fs::read_dir(project_entry.path()) else {
            continue;
        };
        for topic_entry in topic_entries.flatten() {
            if !is_visible_dir(&topic_entry) {
                continue;
            }
            let topic = topic_entry.file_name().to_string_lossy().into_owned();
            let Ok(task_entries) = std::fs::read_dir(topic_entry.path()) else {
                continue;
            };
            for task_entry in task_entries.flatten() {
                if !is_visible_dir(&task_entry) || !keep(&task_entry.path()) {
                    continue;
                }
                let task = task_entry.file_name().to_string_lossy().into_owned();
                out.entry((project.clone(), topic.clone()))
                    .or_default()
                    .insert(task);
            }
        }
    }
}

/// Build the task list from the union of two sources: live worktree dirs under `~/fun_worktrees`
/// and notes dirs carrying a `task.json`. The union keeps live tasks visible (even before
/// migration) while surfacing removed-but-remembered tasks from their retained metadata. Git
/// status is left unloaded (filled in async).
///
/// A hobby task is a single repo, so `repos` holds exactly one entry: the task
/// directory itself, named after its project. Keeping it a vector rather than a
/// scalar lets the async git-status plumbing and the card rendering stay as they are.
///
/// The result is in `(project, topic, name)` order — deterministic, but not display order;
/// [`super::join::Model::items`] applies the merged sort.
pub fn scan_tasks() -> Vec<Task> {
    let mut by_group: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    collect_tasks(&worktrees_root(), |_| true, &mut by_group);
    collect_tasks(
        &notes_root(),
        |p| p.join("task.json").exists(),
        &mut by_group,
    );

    let mut tasks = Vec::new();
    for ((project_name, topic_name), task_names) in by_group {
        for task_name in task_names {
            let id = TaskId {
                project: project_name.clone(),
                topic: topic_name.clone(),
                name: task_name,
            };
            let dir = worktrees_root()
                .join(&id.project)
                .join(&id.topic)
                .join(&id.name);
            let present = dir.is_dir();
            let meta = read_meta(&id);
            // The task dir IS the worktree, so there is nothing to enumerate: present
            // tasks get one Repo pointing at themselves, removed ones get the same
            // entry with the path it would be restored to (git status stays unloaded).
            let repos = vec![Repo {
                path: dir.clone(),
                git: GitInfo::default(),
            }];
            let prs = read_prs(&id);
            let subtasks = read_subtasks(&id);
            let last_active = notes_path(&id)
                .and_then(|p| std::fs::metadata(&p).ok().and_then(|m| m.modified().ok()));

            tasks.push(Task {
                id,
                dir,
                present,
                done: meta.done,
                block: meta.blocked,
                repos,
                prs,
                subtasks,
                last_active,
            });
        }
    }
    tasks
}

/// A task's `task.json` in the notes repo: `{ done, blocked }` and nothing else. Both fields
/// default so a missing or partial file is tolerated, and both are always written back — `hwt`
/// emits `"blocked": null` when a task is unblocked, and these files are tracked in the notes
/// repo, so omitting the key would make every dashboard toggle a spurious diff. There is no
/// `repos` field: the project is in the path and a hobby task is exactly one repo.
///
/// Written by the `hwt`/`hcdev` commands; this app reads it (and edits `done` via
/// [`toggle_done`]).
#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskMeta {
    #[serde(default)]
    done: bool,
    #[serde(default)]
    blocked: Option<Block>,
}

/// Path to a task's `task.json` in the notes tree: `notes/PROJECT/TOPIC/TASK/task.json`.
pub fn meta_path(id: &TaskId) -> PathBuf {
    notes_root().join(id.rel()).join("task.json")
}

/// Read a task's `task.json`. Returns the all-default meta if the file is absent, empty, or
/// malformed — a bad file must never abort the scan. Mirrors [`read_prs`].
fn read_meta(id: &TaskId) -> TaskMeta {
    read_meta_at(&meta_path(id))
}

fn read_meta_at(path: &Path) -> TaskMeta {
    let Ok(content) = std::fs::read_to_string(path) else {
        return TaskMeta::default();
    };
    if content.trim().is_empty() {
        return TaskMeta::default();
    }
    match serde_json::from_str::<TaskMeta>(&content) {
        Ok(m) => m,
        Err(e) => {
            warn!("ignoring malformed {}: {e}", path.display());
            TaskMeta::default()
        }
    }
}

/// Write a task's `task.json`, creating the notes dir if needed. Never touches git.
///
/// Serializes with `to_string_pretty` and no trailing newline — byte-for-byte what nushell's
/// `to json --indent 2 | save` produces in `hwt`, so the two writers never fight over a file.
fn write_meta_at(path: &Path, meta: &TaskMeta) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Path to a task's `prs.json` in the notes tree: `notes/PROJECT/TOPIC/TASK/prs.json`.
pub fn prs_path(id: &TaskId) -> PathBuf {
    notes_root().join(id.rel()).join("prs.json")
}

/// Read a task's `prs.json`: a JSON array of PR objects. Returns an empty vec if the file is
/// absent, empty, or malformed — a bad file must never abort the scan.
fn read_prs(id: &TaskId) -> Vec<Pr> {
    let path = prs_path(id);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new(); // missing or unreadable → no PRs
    };
    if content.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<Pr>>(&content) {
        Ok(prs) => prs,
        Err(e) => {
            warn!("ignoring malformed {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Path to a task's `subtasks.json` in the notes tree: `notes/PROJECT/TOPIC/TASK/subtasks.json`.
pub fn subtasks_path(id: &TaskId) -> PathBuf {
    notes_root().join(id.rel()).join("subtasks.json")
}

/// Read a task's `subtasks.json`: a JSON array of subtask refs. Returns an empty vec if the file is
/// absent, empty, or malformed — a bad file must never abort the scan. Mirrors [`read_prs`].
fn read_subtasks(id: &TaskId) -> Vec<SubtaskRef> {
    let path = subtasks_path(id);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new(); // missing or unreadable → no subtasks
    };
    if content.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<SubtaskRef>>(&content) {
        Ok(subs) => subs,
        Err(e) => {
            warn!("ignoring malformed {}: {e}", path.display());
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Git status / PR status (sync helpers; called from async tasks)
// ---------------------------------------------------------------------------

/// Read branch, short SHA, and dirty count for a single repo. Blocking; run off the main thread.
pub fn read_git(path: &Path) -> GitInfo {
    let branch = git_capture(path, &["symbolic-ref", "--short", "HEAD"]);
    let head = git_capture(path, &["rev-parse", "--short", "HEAD"]);
    let dirty = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count()
        })
        .unwrap_or(0);
    GitInfo {
        loaded: true,
        branch,
        head,
        dirty,
    }
}

/// Read live status for one PR via `gh`. Blocking; run off the main thread (like [`read_git`]).
/// Any failure (gh missing, not authenticated, PR not found, unparseable) maps to
/// [`PrStatus::Unknown`] so the UI degrades gracefully instead of blocking.
pub fn read_pr_status(url: &str) -> (PrStatus, Option<u64>, Option<String>) {
    let out = Command::new("gh")
        .args(["pr", "view", url, "--json", "state,isDraft,number,title"])
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return (PrStatus::Unknown, None, None),
    };

    // gh emits one JSON object: {"state":"OPEN","isDraft":false,"number":123,"title":"..."}
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return (PrStatus::Unknown, None, None);
    };

    let number = v.get("number").and_then(serde_json::Value::as_u64);
    let title = v
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let is_draft = v
        .get("isDraft")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let status = match v.get("state").and_then(serde_json::Value::as_str) {
        Some("MERGED") => PrStatus::Merged,
        Some("CLOSED") => PrStatus::Closed,
        Some("OPEN") if is_draft => PrStatus::Draft,
        Some("OPEN") => PrStatus::Open,
        _ => PrStatus::Unknown,
    };
    (status, number, title)
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Toggle the `done` flag in a task's `task.json`, preserving its other fields. Returns the new
/// done state. Creates the file (and notes dir) if absent.
pub fn toggle_done(id: &TaskId) -> std::io::Result<bool> {
    toggle_done_at(&meta_path(id))
}

fn toggle_done_at(path: &Path) -> std::io::Result<bool> {
    let mut meta = read_meta_at(path);
    meta.done = !meta.done;
    write_meta_at(path, &meta)?;
    Ok(meta.done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `scan_tasks()` enumerates the union of live worktrees and notes tasks across the
    /// three-level `PROJECT/TOPIC/TASK` layout:
    /// - a present task is one whose worktree dir exists;
    /// - a removed task (task.json, no worktree) still appears, so it can be restored;
    /// - each task carries exactly one repo — itself — named after its project;
    /// - the same topic name under two projects stays distinct.
    ///
    /// **This is the only test in the crate that sets `HOME`.** `HOME` is process-global
    /// and cargo runs tests in threads, so a second one would race this. Anything else
    /// needing a tree either shares this fixture or takes a root path parameter.
    #[test]
    fn scan_unions_across_projects() {
        let home = std::env::temp_dir().join("hdash_scan_fixture");
        let _ = std::fs::remove_dir_all(&home);
        // SAFETY: single-threaded test setup; no other test in this crate reads HOME.
        unsafe {
            std::env::set_var("HOME", &home);
        }

        let notes_root = home.join("fun_repos/hobby-mimisbrunnr/notes");
        let wt_root = home.join("fun_worktrees");

        // Present task: the task dir itself is the worktree (a .git entry, no repo level).
        std::fs::create_dir_all(wt_root.join("projA/features/live-task/.git")).unwrap();
        std::fs::create_dir_all(notes_root.join("projA/features/live-task")).unwrap();
        std::fs::write(
            notes_root.join("projA/features/live-task/task.json"),
            r#"{"done": false, "blocked": null}"#,
        )
        .unwrap();

        // Removed task: task.json only, no worktree. Done, and blocked with a past date.
        std::fs::create_dir_all(notes_root.join("projA/bugs/gone-task")).unwrap();
        std::fs::write(
            notes_root.join("projA/bugs/gone-task/task.json"),
            r#"{"done": true, "blocked": {"until": "2020-01-01"}}"#,
        )
        .unwrap();

        // A second project reusing the SAME topic name — must stay a separate task.
        std::fs::create_dir_all(wt_root.join("projB/features/other-task/.git")).unwrap();
        std::fs::create_dir_all(notes_root.join("projB/features/other-task")).unwrap();
        std::fs::write(
            notes_root.join("projB/features/other-task/task.json"),
            r#"{"done": false, "blocked": null}"#,
        )
        .unwrap();

        let tasks = scan_tasks();
        let find = |project: &str, name: &str| {
            tasks
                .iter()
                .find(|t| t.id.project == project && t.id.name == name)
                .unwrap_or_else(|| panic!("{project}/{name} scanned"))
        };

        assert_eq!(tasks.len(), 3);

        // A live worktree is present and carries one repo: itself, named for its project.
        let live = find("projA", "live-task");
        assert!(live.present);
        assert_eq!(live.id.topic, "features");
        assert_eq!(live.repos.len(), 1, "a hobby task is exactly one repo");
        assert_eq!(live.repos[0].path, live.dir, "the repo IS the task dir");
        assert_eq!(live.id.rel(), "projA/features/live-task");

        // The same topic name under another project is its own task, not a merge.
        let other = find("projB", "other-task");
        assert_eq!(other.id.topic, "features");

        // A removed task still shows up, from its retained metadata.
        let gone = find("projA", "gone-task");
        assert!(!gone.present, "removed task has no worktree");
        assert!(gone.done);
        assert!(gone.block.is_some(), "blocked parsed from task.json");
        assert!(gone.ready(), "a past until-date means the block has lifted");
        // Its single repo entry points at where a restore would put it.
        assert_eq!(gone.repos.len(), 1);
        assert_eq!(gone.repos[0].path, gone.dir);

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Only merged/closed are terminal (safe to cache); open/draft can still change and
    /// loading/unknown aren't real answers.
    #[test]
    fn only_merged_and_closed_are_terminal() {
        assert!(PrStatus::Merged.is_terminal());
        assert!(PrStatus::Closed.is_terminal());
        assert!(!PrStatus::Open.is_terminal());
        assert!(!PrStatus::Draft.is_terminal());
        assert!(!PrStatus::Loading.is_terminal());
        assert!(!PrStatus::Unknown.is_terminal());
    }

    /// `task.json` is tracked in the notes repo and has two writers, so a toggle must produce
    /// the exact bytes `hwt` would have written — otherwise every dashboard click shows up as a
    /// schema diff. Asserted at the byte level so a re-added `repos` field, a dropped
    /// `"blocked": null`, or a changed indent all fail here.
    ///
    /// Goes through `toggle_done_at` rather than `toggle_done` to stay off the process-global
    /// `HOME` that the scan test owns.
    #[test]
    fn toggle_done_round_trips_hwt_nu_bytes() {
        let dir = std::env::temp_dir().join("hdash_meta_fixture");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested/task.json");

        // Byte-for-byte what `{ done: false, blocked: null } | to json --indent 2 | save` writes:
        // two-space indent, `blocked` present and null, no trailing newline.
        let undone = "{\n  \"done\": false,\n  \"blocked\": null\n}";
        let done = "{\n  \"done\": true,\n  \"blocked\": null\n}";

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, undone).unwrap();

        assert!(toggle_done_at(&path).unwrap(), "toggled to done");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            done,
            "only the done flag may change"
        );

        assert!(!toggle_done_at(&path).unwrap(), "toggled back to not-done");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            undone,
            "a round trip leaves the file exactly as hwt wrote it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Toggling done must not disturb a block. `hwt block` with no flags writes `"blocked": {}`,
    /// which has to survive the round trip as `{}` and not become `null` or vanish.
    #[test]
    fn toggle_done_preserves_block() {
        let dir = std::env::temp_dir().join("hdash_meta_block_fixture");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("task.json");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            &path,
            r#"{"done": false, "blocked": {"until": "2030-01-01", "reason": "waiting"}}"#,
        )
        .unwrap();

        assert!(toggle_done_at(&path).unwrap());
        let back = read_meta_at(&path);
        assert!(back.done);
        let block = back.blocked.expect("block preserved");
        assert_eq!(block.until.as_deref(), Some("2030-01-01"));
        assert_eq!(block.reason.as_deref(), Some("waiting"));
        // Nested indent matches nushell's `to json --indent 2` (four spaces one level in).
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"done\": true,\n  \"blocked\": {\n    \"until\": \"2030-01-01\",\n    \"reason\": \"waiting\"\n  }\n}"
        );

        // An empty block (bare `hwt block`) stays an empty object, not null.
        std::fs::write(&path, r#"{"done": false, "blocked": {}}"#).unwrap();
        toggle_done_at(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"done\": true,\n  \"blocked\": {}\n}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The PR cache round-trips through disk, creates its parent dir on write, and degrades to an
    /// empty map for a missing or corrupt file (so a bad cache means "re-query", never a crash).
    #[test]
    fn pr_cache_round_trips_and_tolerates_bad_files() {
        let dir = std::env::temp_dir().join("hdash_pr_cache_fixture");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested/pr-status.json");

        // Missing file → empty cache.
        assert!(
            read_pr_cache_at(&path).is_empty(),
            "missing file reads as empty"
        );

        // Write (creating the nested dir) then read back the same entry.
        let mut cache = std::collections::HashMap::new();
        cache.insert(
            "https://github.com/o/r/pull/1".to_owned(),
            CachedPr {
                status: PrStatus::Merged,
                number: Some(1),
                title: Some("done".to_owned()),
            },
        );
        write_pr_cache_at(&path, &cache);
        let back = read_pr_cache_at(&path);
        assert_eq!(back.len(), 1);
        let hit = back
            .get("https://github.com/o/r/pull/1")
            .expect("entry present");
        assert_eq!(hit.status, PrStatus::Merged);
        assert_eq!(hit.number, Some(1));
        assert_eq!(hit.title.as_deref(), Some("done"));

        // Corrupt file → empty cache (not a panic).
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(
            read_pr_cache_at(&path).is_empty(),
            "corrupt file reads as empty"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
