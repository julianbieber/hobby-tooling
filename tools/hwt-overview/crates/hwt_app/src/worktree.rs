//! Domain layer: the worktree/task model, the filesystem scan, git status reads, and the shell
//! actions (open cdev / terminal / notes, toggle done). No Bevy UI here.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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
    /// The child task's directory/branch name (second path level under its topic).
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
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
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
#[derive(Clone, Debug)]
pub struct Repo {
    /// Repo directory name.
    pub name: String,
    /// Absolute path to the repo.
    pub path: PathBuf,
    /// Git status (lazily populated).
    pub git: GitInfo,
}

/// A block: the task is waiting on an action and/or until a day. Deserialized from the `blocked`
/// object in `task.json` (`null`/absent means not blocked).
#[derive(Clone, Debug, Default, serde::Serialize, Deserialize)]
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
    /// task (its metadata still lives in the notes repo; use `wt restore` to re-create it).
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

    /// Whether a `.blocked` marker is present (regardless of its until-date).
    pub fn blocked(&self) -> bool {
        self.block.is_some()
    }

    /// Whether the block has lifted: a `.blocked` marker with an until-date that is today or
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

    /// State rank for sorting: ready (0) < active (1) < blocked (2) < done (3).
    fn state_rank(&self) -> u8 {
        if self.done {
            3
        } else if self.ready() {
            0
        } else if self.blocked() {
            2
        } else {
            1
        }
    }
}

/// Parse a `YYYY-MM-DD` string into `(year, month, day)`.
fn parse_ymd(s: &str) -> Option<(i64, i64, i64)> {
    let mut parts = s.trim().splitn(3, '-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    Some((y, m, d))
}

/// Days since the Unix epoch (1970-01-01) for a civil date. Howard Hinnant's algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// A topic grouping a set of tasks.
#[derive(Clone, Debug)]
pub struct Topic {
    /// Project this topic belongs to. Two projects may use the same topic name, so
    /// the (project, name) pair is what identifies a group.
    pub project: String,
    /// Topic name.
    pub name: String,
    /// Tasks within the topic.
    pub tasks: Vec<Task>,
}

impl Topic {
    /// Number of completed (`.done`) tasks.
    pub fn done_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.done).count()
    }

    /// Number of active (not done) tasks.
    pub fn active_count(&self) -> usize {
        self.tasks.len() - self.done_count()
    }
}

/// The scanned worktree model.
#[derive(Resource, Default)]
pub struct Worktrees {
    /// Topics, sorted by name.
    pub topics: Vec<Topic>,
}

impl Worktrees {
    /// Iterate every task across all topics.
    pub fn all_tasks(&self) -> impl Iterator<Item = &Task> {
        self.topics.iter().flat_map(|t| t.tasks.iter())
    }

    /// Find a task by id.
    pub fn task(&self, id: &TaskId) -> Option<&Task> {
        self.all_tasks().find(|t| t.id == *id)
    }

    /// Find a task by id (mutable).
    pub fn task_mut(&mut self, id: &TaskId) -> Option<&mut Task> {
        self.topics
            .iter_mut()
            .filter(|t| t.name == id.topic)
            .flat_map(|t| t.tasks.iter_mut())
            .find(|t| t.id == *id)
    }
}

/// UI selection / filter state.
#[derive(Resource, Default)]
pub struct UiState {
    /// Selected project, or `None` for "All". The sidebar filters by project here,
    /// where the work dashboard filtered by topic — a hobby task's project is the
    /// coarsest and most useful axis, and topics are shown as a label on each card.
    pub selected_project: Option<String>,
    /// My git identity in the notes repo, for the header line.
    pub me: Option<String>,
    /// Whether completed tasks are shown.
    pub show_done: bool,
    /// Tasks hidden for this session only (in-memory; cleared on Refresh).
    pub hidden: HashSet<TaskId>,
    /// Tasks whose PR list is expanded (in-memory; collapsed by default).
    pub pr_expanded: HashSet<TaskId>,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub use hwt_core::{notes_root, open_url, rel_time, worktrees_root};

/// `~/.cache/wt-overview/pr-status.json` (honouring `XDG_CACHE_HOME`): the persisted terminal
/// PR-status cache.
fn pr_cache_path() -> PathBuf {
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

/// Load the terminal-PR-status cache (URL → status). A missing or unparseable file is treated as an
/// empty cache, so a corrupt cache degrades to re-querying rather than breaking the app.
pub fn read_pr_cache() -> std::collections::HashMap<String, CachedPr> {
    read_pr_cache_at(&pr_cache_path())
}

/// Persist the terminal-PR-status cache, creating `~/.cache/wt-overview/` if needed. Best-effort:
/// a write failure is logged and otherwise ignored (the cache is an optimisation, not a source of
/// truth).
pub fn write_pr_cache(cache: &std::collections::HashMap<String, CachedPr>) {
    write_pr_cache_at(&pr_cache_path(), cache);
}

/// Delete the on-disk terminal-PR-status cache so the next scan re-queries every PR. Best-effort;
/// a missing file is success.
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

/// Resolve a task's notes location: either `notes/TOPIC/TASK/` (dir) or `notes/TOPIC/TASK.md`.
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
// Ownership (the notes repo is shared across the team)
// ---------------------------------------------------------------------------

/// The git identity configured in the hobby notes repo (`user.email`).
///
/// The work dashboard used this to tell the user's tasks from teammates' on a shared
/// notes repo. The hobby notes repo has exactly one author, so there is nothing to
/// disambiguate and no owner filter in the UI; this is kept only so the window title
/// can show who the checkout belongs to.
pub fn notes_repo_email() -> Option<String> {
    git_capture(&notes_root(), &["config", "user.email"])
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
/// have a `features` topic and their tasks must not be merged into one group.
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

/// Build the task model from the union of two sources: live worktree dirs under `~/worktrees` and
/// notes dirs carrying a `task.json`. The union keeps live tasks visible (even before migration)
/// while surfacing removed-but-remembered tasks from their retained metadata. Git status is left
/// unloaded (filled in async).
///
/// A hobby task is a single repo, so `repos` holds at most one entry: the task
/// directory itself, named after its project. Keeping it a vector rather than a
/// scalar lets the async git-status plumbing and the card rendering stay as they are.
pub fn scan() -> Worktrees {
    let mut by_group: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    collect_tasks(&worktrees_root(), |_| true, &mut by_group);
    collect_tasks(
        &notes_root(),
        |p| p.join("task.json").exists(),
        &mut by_group,
    );

    let mut topics = Vec::new();
    for ((project_name, topic_name), task_names) in by_group {
        let mut tasks = Vec::new();
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
                name: id.project.clone(),
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

        // Ready (block lifted) first, then active, then blocked, then done; ties broken by
        // most-recent notes activity, then name.
        tasks.sort_by(|a, b| {
            a.state_rank()
                .cmp(&b.state_rank())
                .then(b.last_active.cmp(&a.last_active))
                .then(a.id.name.cmp(&b.id.name))
        });

        if !tasks.is_empty() {
            topics.push(Topic {
                project: project_name,
                name: topic_name,
                tasks,
            });
        }
    }

    // Most open (active) tasks first; topics with all tasks done sink to the bottom. Ties broken
    // alphabetically.
    topics.sort_by(|a, b| {
        b.active_count()
            .cmp(&a.active_count())
            .then(a.name.cmp(&b.name))
    });
    Worktrees { topics }
}


/// A task's `task.json` in the notes repo: done/blocked state and the recorded repo list. All
/// fields default so a missing or partial file is tolerated. Written by the `wt`/`cdev` commands;
/// this app reads it (and edits `done` via [`toggle_done`]).
#[derive(Debug, Default, serde::Serialize, Deserialize)]
struct TaskMeta {
    #[serde(default)]
    done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blocked: Option<Block>,
    #[serde(default)]
    repos: Vec<String>,
}

/// Path to a task's `task.json` in the notes tree: `notes/TOPIC/TASK/task.json`.
pub fn meta_path(id: &TaskId) -> PathBuf {
    notes_root()
        .join(&id.topic)
        .join(&id.name)
        .join("task.json")
}

/// Read a task's `task.json`. Returns the all-default meta if the file is absent, empty, or
/// malformed — a bad file must never abort the scan. Mirrors [`read_prs`].
fn read_meta(id: &TaskId) -> TaskMeta {
    let path = meta_path(id);
    let Ok(content) = std::fs::read_to_string(&path) else {
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

/// Write a task's `task.json` (creating the notes dir if needed). Never touches git.
fn write_meta(id: &TaskId, meta: &TaskMeta) -> std::io::Result<()> {
    let path = meta_path(id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)
}

/// Path to a task's `prs.json` in the notes tree: `notes/TOPIC/TASK/prs.json`.
pub fn prs_path(id: &TaskId) -> PathBuf {
    notes_root().join(&id.topic).join(&id.name).join("prs.json")
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

/// Path to a task's `subtasks.json` in the notes tree: `notes/TOPIC/TASK/subtasks.json`.
pub fn subtasks_path(id: &TaskId) -> PathBuf {
    notes_root()
        .join(&id.topic)
        .join(&id.name)
        .join("subtasks.json")
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
// Git status (sync helper; called from async tasks)
// ---------------------------------------------------------------------------

use hwt_core::git_capture;

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
    let mut meta = read_meta(id);
    meta.done = !meta.done;
    write_meta(id, &meta)?;
    Ok(meta.done)
}

use hwt_core::spawn_detached;

/// Open a full `hcdev` session (Zellij hobby-dev layout, Claude on the hobby account)
/// in a new terminal for this task.
pub fn launch_cdev(task: &Task) {
    let dir = task.dir.to_string_lossy().into_owned();
    let title = format!("hcdev-{}-{}-{}", task.id.project, task.id.topic, task.id.name);
    // Pass project and topic explicitly: the same task name can exist under two
    // projects, and `hcdev wt` errors on an ambiguous name rather than guessing.
    let cdev_cmd = format!(
        "hcdev wt '{}' --project '{}' --topic '{}'",
        task.id.name, task.id.project, task.id.topic
    );
    spawn_detached(&[
        "-f",
        "alacritty",
        "--working-directory",
        &dir,
        "--title",
        &title,
        "-e",
        "nu",
        "--login",
        "-c",
        &cdev_cmd,
    ]);
}

/// Re-create a removed task's worktree by running `hwt restore` in a new terminal. The
/// task dir does not exist yet, so the terminal opens in the worktrees root.
pub fn launch_restore(task: &Task) {
    let dir = worktrees_root().to_string_lossy().into_owned();
    let title = format!("restore-{}", task.id.name);
    let cmd = format!("hwt restore '{}'", task.id.name);
    spawn_detached(&[
        "-f",
        "alacritty",
        "--working-directory",
        &dir,
        "--title",
        &title,
        "-e",
        "nu",
        "--login",
        "-c",
        &cmd,
    ]);
}

/// Open a plain terminal in the task directory.
pub fn launch_terminal(task: &Task) {
    let dir = task.dir.to_string_lossy().into_owned();
    let title = format!("term-{}", task.id.name);
    spawn_detached(&[
        "-f",
        "alacritty",
        "--working-directory",
        &dir,
        "--title",
        &title,
    ]);
}

/// Open a terminal in the task's notes directory (falls back to the task directory).
pub fn open_notes(task: &Task) {
    let dir = notes_path(&task.id)
        .map(|p| {
            if p.is_dir() {
                p
            } else {
                p.parent().map(Path::to_path_buf).unwrap_or(p)
            }
        })
        .unwrap_or_else(|| task.dir.clone());
    let dir = dir.to_string_lossy().into_owned();
    spawn_detached(&[
        "-f",
        "alacritty",
        "--working-directory",
        &dir,
        "--title",
        "notes",
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .expect("run git")
            .success();
        assert!(ok, "git {args:?} failed");
    }

    /// `scan()` enumerates the union of live worktrees and notes tasks, and tags each with an owner
    /// (the notes repo is shared, so filtering happens later in the UI):
    /// - a present task reads its repos from disk and is owned by me;
    /// - a removed task (task.json, no worktree) is owned by its notes dir's last committer;
    /// - a removed task with no commits yet (uncommitted → local) is owned by me.
    #[test]
    fn scan_unions_and_tags_owner() {
        // Isolate HOME to a fixture. Tests in this module share the process, so keep it to one.
        let home = std::env::temp_dir().join("wt_overview_scan_fixture");
        let _ = std::fs::remove_dir_all(&home);
        // SAFETY: single-threaded test setup; no other code reads HOME concurrently here.
        unsafe {
            std::env::set_var("HOME", &home);
        }

        let notes_root = home.join("projects/team-constellation-mimisbrunnr/notes");
        let notes = notes_root.join("topicA");
        let wts = home.join("worktrees/topicA");

        // Present task: a live worktree with one repo + matching task.json.
        std::fs::create_dir_all(wts.join("live-task/RepoX/.git")).unwrap();
        std::fs::create_dir_all(notes.join("live-task")).unwrap();
        std::fs::write(
            notes.join("live-task/task.json"),
            r#"{"done": false, "blocked": null, "repos": ["RepoX"]}"#,
        )
        .unwrap();

        // Removed task, mine: task.json only, no worktree. Blocked with a past date.
        std::fs::create_dir_all(notes.join("gone-task")).unwrap();
        std::fs::write(
            notes.join("gone-task/task.json"),
            r#"{"done": true, "blocked": {"until": "2020-01-01"}, "repos": ["RepoY", "RepoZ"]}"#,
        )
        .unwrap();

        // Removed task, a teammate's: task.json only, committed under their name below.
        std::fs::create_dir_all(notes.join("mate-task")).unwrap();
        std::fs::write(
            notes.join("mate-task/task.json"),
            r#"{"done": false, "blocked": null, "repos": ["RepoT"]}"#,
        )
        .unwrap();

        // Removed task, uncommitted: task.json only, never committed (stays local → mine).
        std::fs::create_dir_all(notes.join("orphan-task")).unwrap();
        std::fs::write(
            notes.join("orphan-task/task.json"),
            r#"{"done": false, "blocked": null, "repos": ["RepoQ"]}"#,
        )
        .unwrap();

        // Make the notes tree a git repo. My identity commits live/gone; a teammate authors mate;
        // orphan is left unstaged (uncommitted).
        git(&notes_root, &["init", "-q"]);
        git(
            &notes_root,
            &["config", "user.email", "j.bieber@omniaretail.com"],
        );
        git(&notes_root, &["config", "user.name", "Julian"]);
        git(
            &notes_root,
            &["add", "topicA/live-task", "topicA/gone-task"],
        );
        git(&notes_root, &["commit", "-q", "-m", "mine"]);
        git(&notes_root, &["add", "topicA/mate-task"]);
        git(
            &notes_root,
            &[
                "commit",
                "-q",
                "--author=Mate <mate@omniaretail.com>",
                "-m",
                "theirs",
            ],
        );

        let model = scan();
        let topic = model
            .topics
            .iter()
            .find(|t| t.name == "topicA")
            .expect("topicA present");
        let task = |name: &str| topic.tasks.iter().find(|t| t.id.name == name);
        let me = Some("j.bieber@omniaretail.com");

        // All four tasks are enumerated (filtering is a UI concern now).
        assert!(task("live-task").is_some());
        assert!(task("gone-task").is_some());
        assert!(task("orphan-task").is_some());
        assert!(task("mate-task").is_some());

        // Owners are tagged from notes authorship (or me for live/uncommitted).
        assert_eq!(task("live-task").unwrap().owner.as_deref(), me, "live = me");
        assert_eq!(
            task("gone-task").unwrap().owner.as_deref(),
            me,
            "committed by me"
        );
        assert_eq!(
            task("orphan-task").unwrap().owner.as_deref(),
            me,
            "uncommitted = me"
        );
        assert_eq!(
            task("mate-task").unwrap().owner.as_deref(),
            Some("mate@omniaretail.com"),
            "teammate's removed task tagged with their email"
        );

        let live = task("live-task").unwrap();
        assert!(live.present);
        assert_eq!(
            live.repos.iter().map(|r| &r.name).collect::<Vec<_>>(),
            ["RepoX"]
        );

        let gone = task("gone-task").unwrap();
        assert!(!gone.present, "removed task has no worktree");
        assert!(gone.done);
        assert!(gone.block.is_some(), "blocked parsed from task.json");
        assert_eq!(
            gone.repos
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>(),
            ["RepoY", "RepoZ"],
            "removed task's repos come from metadata"
        );

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

    /// The PR cache round-trips through disk, creates its parent dir on write, and degrades to an
    /// empty map for a missing or corrupt file (so a bad cache means "re-query", never a crash).
    #[test]
    fn pr_cache_round_trips_and_tolerates_bad_files() {
        let dir = std::env::temp_dir().join("wt_overview_pr_cache_fixture");
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
