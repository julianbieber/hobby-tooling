//! The tasks: the notes filesystem scan, git status reads, PR status via `gh`, and the
//! `task.json` marks. No UI here.
//!
//! A task is a notes directory carrying a `task.json`. The worktree under
//! `~/fun_worktrees/PROJECT/TOPIC/TASK` is not consulted for existence — only for
//! [`Task::present`]. A worktree with no `task.json` behind it is an **orphan**
//! ([`scan_orphan_worktrees`]), carried as a `Task` with `adopted: false`.
//!
//! The notes repo decides rather than the worktree tree because `~/fun_worktrees` answers one
//! question only — *is it checked out* — and is never enumerated to find out what work there
//! is. `hwt` reads the same file by the same rule, so its task count and the dashboard's agree.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use hwt_core::{Roots, clean, days_from_civil, git_capture, parse_ymd};
use log::warn;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub use hwt_core::{notes_root, worktrees_root};

/// A git status result for one repo, delivered from the async scan over a channel.
pub struct GitMsg {
    /// The task the repo belongs to.
    pub id: TaskId,
    /// Path of the repo the status was read from.
    pub path: PathBuf,
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
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
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
    /// A PR known only by its URL, its live state not yet queried.
    pub fn from_url(url: String) -> Self {
        Pr {
            url,
            ..Pr::default()
        }
    }

    /// Best available title: the live `gh` title, then the file title, else empty.
    pub fn display_title(&self) -> &str {
        self.live_title
            .as_deref()
            .or(self.title.as_deref())
            .unwrap_or("")
    }
}

/// An aspect of a task: a piece of the task's own work, worked in a tab of the task's session,
/// recorded in `notes/PROJECT/TOPIC/TASK/aspects/NAME/aspect.json`. Never a task of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AspectRef {
    /// The aspect's directory name, which is also its tab name.
    pub name: String,
    /// `research` or `code` as the file records it; `research` when it records none.
    pub kind: String,
    /// `open`, `active`, `reported`, `pr`, `done` or `closed` as the file records it;
    /// `unreadable` when the file could not be parsed.
    pub state: String,
    /// The question, if recorded.
    pub goal: Option<String>,
    /// The branch the aspect was cut from and its PR goes to; `None` for the default branch.
    pub base: Option<String>,
    /// `YYYY-MM-DD` the aspect was added.
    pub created: Option<String>,
    /// The aspect's PR, its live state loaded like a task's.
    pub pr: Option<Pr>,
    /// What the aspect concluded, written when it finished.
    pub outcome: Option<String>,
    /// What the aspect left unresolved, in the order it recorded them.
    pub open_items: Vec<OpenItem>,
    /// The `phase` in the aspect's own `build-task.json`, for an aspect worked through
    /// `/task:build`.
    pub phase: Option<String>,
    /// The live plan in the same file.
    pub todo: Vec<Todo>,
    /// What the aspect's session is waiting on me for.
    pub waiting_on: Option<String>,
}

impl AspectRef {
    /// The open items no task of their own covers yet.
    pub fn uncovered_items(&self) -> usize {
        self.open_items.iter().filter(|i| i.issue.is_none()).count()
    }

    /// Whether the aspect has anything beyond its one-line summary to expand.
    pub fn has_details(&self) -> bool {
        !self.todo.is_empty()
            || self.outcome.is_some()
            || !self.open_items.is_empty()
            || self.base.is_some()
            || self.created.is_some()
    }
}

/// Something an aspect left unresolved. `issue` names the task that now covers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenItem {
    pub text: String,
    pub issue: Option<String>,
}

#[derive(Deserialize)]
struct AspectFile {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    pr: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    open_items: Vec<OpenItemFile>,
}

#[derive(Deserialize)]
struct OpenItemFile {
    #[serde(default)]
    text: String,
    #[serde(default)]
    issue: Option<serde_json::Value>,
}

/// The live state of a PR a task's block waits on, delivered from the async query.
pub struct BlockPrMsg {
    pub id: TaskId,
    pub url: String,
    pub status: PrStatus,
}

/// A live PR status result, delivered from the async query over a channel. Mirrors [`GitMsg`].
pub struct PrMsg {
    /// The task the PR belongs to.
    pub id: TaskId,
    /// URL of the PR the status belongs to.
    pub url: String,
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
#[derive(Clone, Default, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
    /// Absolute path to the repo.
    pub path: PathBuf,
    /// Git status (lazily populated).
    pub git: GitInfo,
}

/// A block: the task is waiting on an action, a day and/or a pull request. Deserialized from the `blocked`
/// object in `task.json` (`null`/absent means not blocked).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// `YYYY-MM-DD` the block lifts, if set (`None` = indefinite, waiting on an action).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Human-readable reason the task is blocked, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    /// That PR's last-read `gh` state (`OPEN`, `MERGED`, `CLOSED`); `None` until first read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_state: Option<String>,
}

impl Block {
    pub fn pr_merged(&self) -> bool {
        self.pr_state.as_deref() == Some("MERGED")
    }

    pub fn pr_closed(&self) -> bool {
        self.pr_state.as_deref() == Some("CLOSED")
    }

    /// The URL to ask GitHub about: the block's PR, unless its state can no longer change.
    pub fn pr_to_check(&self) -> Option<&str> {
        if self.pr_merged() || self.pr_closed() {
            return None;
        }
        self.pr.as_deref()
    }
}

/// A task: a directory under a topic, possibly holding several repos.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    /// Stable id.
    pub id: TaskId,
    /// Absolute path to the (intended) worktree task directory. May not exist if the worktree was
    /// removed but the metadata was kept — see [`Task::present`].
    pub dir: PathBuf,
    /// Whether the worktree task directory exists on disk. `false` for a removed-but-remembered
    /// task (its metadata still lives in the notes repo; use `hwt restore` to re-create it).
    pub present: bool,
    /// Whether the notes repo records this task at all. `false` marks an **orphan**: a worktree
    /// on disk with no `task.json` behind it. It stays a `Task` rather than its own type so the
    /// sort, the git-status plumbing and the card handle it unchanged; the stage and
    /// the **Adopt** button are the only things that read this flag.
    pub adopted: bool,
    /// Whether the task is marked done in its `task.json`.
    pub done: bool,
    /// Block from `task.json`, if present.
    pub block: Option<Block>,
    /// Backlog mark from `task.json`: `None` until one has been set.
    pub backlog: Option<bool>,
    /// The `title:` from the task's `issue.md` frontmatter, if it has one.
    pub title: Option<String>,
    /// Whether a branch named after the task exists in the project's main checkout, locally
    /// or on a remote. Always `true` for a present worktree, and for a removed task whose
    /// checkout could not be read; `false` only when the checkout answered and had none.
    pub has_branch: bool,
    /// Repos checked out under this task (may be empty).
    pub repos: Vec<Repo>,
    /// Pull requests declared in the task's `prs.json` (live status loaded async).
    pub prs: Vec<Pr>,
    /// Aspects recorded under the task's `aspects/` notes dir, by name, closed ones
    /// included.
    pub aspects: Vec<AspectRef>,
    /// Notes directory/file last-modified time, if any.
    pub last_active: Option<SystemTime>,
    /// The `phase` recorded in the task's `build-task.json` by `/task:build`, if there is one.
    pub phase: Option<String>,
    /// The session's live plan, `todo` in the same `build-task.json`, in the order it was written.
    /// Empty when the file has none.
    pub todo: Vec<Todo>,
    /// What the session is waiting on me for, `waiting_on` in the same file; `None` while it is
    /// working or has never stopped.
    pub waiting_on: Option<String>,
    /// The zellij session name `hcdev` gives this task, as `hcdev.nu` spells it.
    pub session_name: String,
}

impl Task {
    /// The task's own PRs and its aspects' PRs, which all load their live state the same way.
    pub fn all_prs(&self) -> impl Iterator<Item = &Pr> {
        self.prs
            .iter()
            .chain(self.aspects.iter().filter_map(|a| a.pr.as_ref()))
    }

    /// [`Task::all_prs`], mutably.
    pub fn all_prs_mut(&mut self) -> impl Iterator<Item = &mut Pr> {
        self.prs
            .iter_mut()
            .chain(self.aspects.iter_mut().filter_map(|a| a.pr.as_mut()))
    }

    /// The task's window title as `hdash` launches it: the unclamped `hcdev-PROJECT-TOPIC-TASK`.
    pub fn window_title(&self) -> String {
        unclamped_session_name(&self.id)
    }

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

    /// Whether the block has lifted: the PR it waits on merged, or its until-date is today
    /// or earlier. Any other block is not "ready".
    pub fn ready(&self) -> bool {
        let Some(block) = &self.block else {
            return false;
        };
        if block.pr_merged() {
            return true;
        }
        let Some(until) = &block.until else {
            return false;
        };
        let Some((y, m, d)) = parse_ymd(until) else {
            return false;
        };
        let today = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|s| i64::try_from(s.as_secs() / 86_400).ok())
            .unwrap_or(0);
        today >= days_from_civil(y, m, d)
    }
}

fn pr_cache_path() -> PathBuf {
    hwt_core::xdg_cache_path("hdash", "pr-status.json")
}

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

/// Resolve a task's notes location: either `notes/PROJECT/TOPIC/TASK/` (dir) or
/// `notes/PROJECT/TOPIC/TASK.md`.
pub fn notes_path(roots: &Roots, id: &TaskId) -> Option<PathBuf> {
    let base = roots.notes.join(&id.project).join(&id.topic);
    let dir = roots.notes.join(id.rel());
    if dir.is_dir() {
        return Some(dir);
    }
    let file = base.join(format!("{}.md", id.name));
    if file.exists() {
        return Some(file);
    }
    None
}

fn is_visible_dir(entry: &std::fs::DirEntry) -> bool {
    entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.')
}

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

/// Build the task list from the notes tree: every `notes/PROJECT/TOPIC/TASK/` carrying a
/// `task.json`. The worktree tree is not enumerated — it only answers [`Task::present`] —
/// so a directory under `~/fun_worktrees` with no `task.json` behind it is an orphan, not a
/// task, and is appended by [`scan_orphan_worktrees`]. Git status is left unloaded (filled
/// in async).
///
/// A hobby task is a single repo, so `repos` holds exactly one entry: the task
/// directory itself, named after its project. Keeping it a vector rather than a
/// scalar lets the async git-status plumbing and the card rendering stay as they are.
///
/// The result is in `(project, topic, name)` order with the orphans after the tasks —
/// deterministic, but not display order; [`super::item::Model::items`] applies the display
/// sort.
///
/// Orphans ride in this vector rather than a second field on the model so that every consumer —
/// the sort, the async git-status fetches, the card — treats them as tasks with
/// nothing special done for them. `adopted` is the only thing that tells them apart.
pub fn scan_tasks(roots: &Roots) -> Vec<Task> {
    let mut by_group: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    collect_tasks(
        &roots.notes,
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
            let meta = read_meta(roots, &id);
            let mut task = build_task(roots, id, true, meta.done, meta.blocked);
            task.backlog = meta.backlog;
            tasks.push(task);
        }
    }
    mark_unstarted(&roots.projects, &mut tasks);
    tasks.extend(scan_orphan_worktrees(roots));
    name_sessions(&mut tasks);
    tasks
}

fn unclamped_session_name(id: &TaskId) -> String {
    format!("hcdev-{}-{}-{}", id.project, id.topic, id.name)
}

fn name_cache() -> &'static Mutex<HashMap<TaskId, String>> {
    static CACHE: OnceLock<Mutex<HashMap<TaskId, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn name_sessions(tasks: &mut [Task]) {
    let mut cache = name_cache().lock();
    let unknown: Vec<TaskId> = tasks
        .iter()
        .map(|t| t.id.clone())
        .filter(|id| !cache.contains_key(id))
        .collect();
    if !unknown.is_empty() {
        match session_names_from_nu(&unknown) {
            Some(names) => cache.extend(unknown.into_iter().zip(names)),
            None => {
                static WARNED: OnceLock<()> = OnceLock::new();
                if WARNED.set(()).is_ok() {
                    warn!(
                        "hcdev.nu did not name the sessions; using unclamped hcdev-P-T-TASK names"
                    );
                }
            }
        }
    }
    for task in tasks {
        task.session_name = cache
            .get(&task.id)
            .cloned()
            .unwrap_or_else(|| unclamped_session_name(&task.id));
    }
}

fn session_names_from_nu(ids: &[TaskId]) -> Option<Vec<String>> {
    let module = hwt_core::xdg_config_path("nushell", "hcdev.nu");
    let module = module.to_str().filter(|m| !m.contains('\''))?;
    let input: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| serde_json::json!({ "project": id.project, "topic": id.topic, "task": id.name }))
        .collect();
    let script = format!(
        "use '{module}' *; $env.HDASH_IDS | from json | hcdev-session-names | to json --raw"
    );
    let out = Command::new("nu")
        .args(["-c", &script])
        .env("HDASH_IDS", serde_json::to_string(&input).ok()?)
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let names: Vec<String> = serde_json::from_slice(&out.stdout).ok()?;
    (names.len() == ids.len()).then_some(names)
}

fn mark_unstarted(projects_root: &Path, tasks: &mut [Task]) {
    let projects: BTreeSet<String> = tasks
        .iter()
        .filter(|t| !t.present)
        .map(|t| t.id.project.clone())
        .collect();
    for project in projects {
        let Some(branches) = branch_names(&projects_root.join(&project)) else {
            continue;
        };
        for task in tasks
            .iter_mut()
            .filter(|t| t.id.project == project && !t.present)
        {
            task.has_branch = branches.contains(&task.id.name);
        }
    }
}

fn branch_names(checkout: &Path) -> Option<HashSet<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args([
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(parse_branch_refs(&String::from_utf8_lossy(&out.stdout)))
}

fn parse_branch_refs(refs: &str) -> HashSet<String> {
    refs.lines()
        .filter_map(|r| {
            r.strip_prefix("refs/heads/").or_else(|| {
                r.strip_prefix("refs/remotes/")
                    .and_then(|rest| rest.split_once('/'))
                    .map(|(_, branch)| branch)
            })
        })
        .filter(|b| *b != "HEAD")
        .map(str::to_owned)
        .collect()
}

/// Every worktree under `~/fun_worktrees/PROJECT/TOPIC/TASK` with no `task.json` in the
/// notes repo, as a `Task` with `adopted: false`. In `(project, topic, name)` order.
///
/// The scan surfaces orphans rather than adopting them, because a scan never writes: adopting is
/// a click on the card, so nothing enters the notes repo without having been asked for.
pub fn scan_orphan_worktrees(roots: &Roots) -> Vec<Task> {
    let mut by_group: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    collect_tasks(&roots.worktrees, |_| true, &mut by_group);

    let mut orphans = Vec::new();
    for ((project_name, topic_name), task_names) in by_group {
        for task_name in task_names {
            let id = TaskId {
                project: project_name.clone(),
                topic: topic_name.clone(),
                name: task_name,
            };
            if meta_path(roots, &id).exists() {
                continue;
            }
            orphans.push(build_task(roots, id, false, false, None));
        }
    }
    orphans
}

fn build_task(roots: &Roots, id: TaskId, adopted: bool, done: bool, block: Option<Block>) -> Task {
    let dir = roots
        .worktrees
        .join(&id.project)
        .join(&id.topic)
        .join(&id.name);
    let present = dir.is_dir();
    let repos = vec![Repo {
        path: dir.clone(),
        git: GitInfo::default(),
    }];
    let prs = read_prs(roots, &id);
    let BuildState {
        phase,
        todo,
        waiting_on,
    } = read_build_state(roots, &id);
    let aspects = read_aspects(&roots.notes.join(id.rel()).join("aspects"));
    let last_active = notes_path(roots, &id)
        .and_then(|p| std::fs::metadata(&p).ok().and_then(|m| m.modified().ok()));
    let title = std::fs::read_to_string(issue_path(roots, &id))
        .ok()
        .and_then(|text| frontmatter_title(&text))
        .map(|t| clean(&t));

    Task {
        id,
        dir,
        adopted,
        present,
        done,
        block,
        backlog: None,
        title,
        has_branch: true,
        repos,
        prs,
        aspects,
        last_active,
        phase,
        todo,
        waiting_on,
        session_name: String::new(),
    }
}

/// Path to a task's `build-task.json` in the notes tree, the `/task:build` state.
pub fn build_state_path(roots: &Roots, id: &TaskId) -> PathBuf {
    roots.notes.join(id.rel()).join("build-task.json")
}

/// One step of a session's live plan, as `/task:build` records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub text: String,
    pub status: TodoStatus,
}

/// Where a [`Todo`] stands. An unknown or missing status reads as `Pending`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    Active,
    Done,
    Dropped,
}

impl TodoStatus {
    fn parse(status: Option<&str>) -> Self {
        match status {
            Some("active") => TodoStatus::Active,
            Some("done") => TodoStatus::Done,
            Some("dropped") => TodoStatus::Dropped,
            _ => TodoStatus::Pending,
        }
    }
}

/// How far a todo list has got. A dropped item counts as done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TodoCount {
    pub open: usize,
    pub done: usize,
}

impl TodoCount {
    pub fn of(todos: &[Todo]) -> Self {
        todos.iter().fold(TodoCount::default(), |mut c, t| {
            match t.status {
                TodoStatus::Pending | TodoStatus::Active => c.open += 1,
                TodoStatus::Done | TodoStatus::Dropped => c.done += 1,
            }
            c
        })
    }

    pub fn total(self) -> usize {
        self.open + self.done
    }
}

#[derive(Default)]
struct BuildState {
    phase: Option<String>,
    todo: Vec<Todo>,
    waiting_on: Option<String>,
}

fn read_build_state(roots: &Roots, id: &TaskId) -> BuildState {
    read_build_state_at(&build_state_path(roots, id))
}

fn read_build_state_at(path: &Path) -> BuildState {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| parse_build_state(&text))
        .unwrap_or_default()
}

fn parse_build_state(text: &str) -> BuildState {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return BuildState::default();
    };
    let line = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(clean)
            .filter(|s| !s.is_empty())
    };
    let todo = value
        .get("todo")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let text = clean(item.get("text")?.as_str()?);
            let status = TodoStatus::parse(item.get("status").and_then(serde_json::Value::as_str));
            (!text.is_empty()).then_some(Todo { text, status })
        })
        .collect();
    BuildState {
        phase: line("phase"),
        todo,
        waiting_on: line("waiting_on"),
    }
}

/// Path to a task's `issue.md` in the notes tree: `notes/PROJECT/TOPIC/TASK/issue.md`.
pub fn issue_path(roots: &Roots, id: &TaskId) -> PathBuf {
    roots.notes.join(id.rel()).join("issue.md")
}

fn frontmatter_title(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let value = lines
        .take_while(|l| l.trim_end() != "---")
        .find_map(|l| l.strip_prefix("title:"))?
        .trim();
    let unquoted = match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => inner.replace("\\\"", "\""),
        None => value.to_owned(),
    };
    (!unquoted.is_empty()).then_some(unquoted)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskMeta {
    #[serde(default)]
    done: bool,
    #[serde(default)]
    blocked: Option<Block>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backlog: Option<bool>,
}

/// Path to a task's `task.json` in the notes tree: `notes/PROJECT/TOPIC/TASK/task.json`.
pub fn meta_path(roots: &Roots, id: &TaskId) -> PathBuf {
    roots.notes.join(id.rel()).join("task.json")
}

fn read_meta(roots: &Roots, id: &TaskId) -> TaskMeta {
    read_meta_at(&meta_path(roots, id))
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

fn write_meta_at(path: &Path, meta: &TaskMeta) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Path to a task's `prs.json` in the notes tree: `notes/PROJECT/TOPIC/TASK/prs.json`.
pub fn prs_path(roots: &Roots, id: &TaskId) -> PathBuf {
    roots.notes.join(id.rel()).join("prs.json")
}

fn read_prs(roots: &Roots, id: &TaskId) -> Vec<Pr> {
    let path = prs_path(roots, id);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new(); // missing or unreadable → no PRs
    };
    if content.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<Pr>>(&content) {
        Ok(prs) => prs
            .into_iter()
            .map(|mut pr| {
                pr.repo = pr.repo.map(|r| clean(&r));
                pr.title = pr.title.map(|t| clean(&t));
                pr.opened = pr.opened.map(|o| clean(&o));
                pr
            })
            .collect(),
        Err(e) => {
            warn!("ignoring malformed {}: {e}", path.display());
            Vec::new()
        }
    }
}

fn read_aspects(dir: &Path) -> Vec<AspectRef> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut aspects: Vec<AspectRef> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| read_aspect(&e.path(), &e.file_name().to_string_lossy()))
        .collect();
    aspects.sort_by(|a, b| a.name.cmp(&b.name));
    aspects
}

fn read_aspect(dir: &Path, name: &str) -> AspectRef {
    let BuildState {
        phase,
        todo,
        waiting_on,
    } = read_build_state_at(&dir.join("build-task.json"));
    let parsed = std::fs::read_to_string(dir.join("aspect.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<AspectFile>(&text).ok());
    let line = |s: Option<String>| s.map(|s| clean(s.trim())).filter(|s| !s.is_empty());
    let base = AspectRef {
        name: clean(name),
        phase,
        todo,
        waiting_on,
        ..AspectRef::default()
    };
    let Some(file) = parsed else {
        return AspectRef {
            kind: "research".into(),
            state: "unreadable".into(),
            ..base
        };
    };
    AspectRef {
        kind: clean(file.kind.as_deref().unwrap_or("research")),
        state: clean(file.state.as_deref().unwrap_or("open")),
        goal: line(file.goal),
        base: line(file.base),
        created: line(file.created),
        pr: line(file.pr).map(Pr::from_url),
        outcome: line(file.outcome),
        open_items: file
            .open_items
            .into_iter()
            .map(|i| OpenItem {
                text: clean(i.text.trim()),
                issue: match i.issue {
                    Some(serde_json::Value::String(s)) => line(Some(s)),
                    Some(serde_json::Value::Number(n)) => Some(n.to_string()),
                    _ => None,
                },
            })
            .filter(|i| !i.text.is_empty())
            .collect(),
        ..base
    }
}

/// Read branch, short SHA, and dirty count for a single repo. Blocking; run off the main thread.
pub fn read_git(path: &Path) -> GitInfo {
    let branch = git_capture(path, &["symbolic-ref", "--short", "HEAD"]).map(|b| clean(&b));
    let head = git_capture(path, &["rev-parse", "--short", "HEAD"]);
    let dirty = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain"])
        .stdin(Stdio::null())
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
        .stdin(Stdio::null())
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return (PrStatus::Unknown, None, None),
    };

    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return (PrStatus::Unknown, None, None);
    };

    let number = v.get("number").and_then(serde_json::Value::as_u64);
    let title = v
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(clean);
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

/// Toggle the `done` flag in a task's `task.json`, preserving its other fields. Returns the new
/// done state. Creates the file (and notes dir) if absent.
pub fn toggle_done(roots: &Roots, id: &TaskId) -> std::io::Result<bool> {
    toggle_done_at(&meta_path(roots, id))
}

/// Record `backlog` in a task's `task.json`, leaving `done` and `blocked` as they are. Creates
/// the file (and notes dir) if absent.
pub fn set_backlog(roots: &Roots, id: &TaskId, backlog: bool) -> std::io::Result<()> {
    set_backlog_at(&meta_path(roots, id), backlog)
}

/// Record an orphan worktree as a task by writing the `task.json` it is missing:
/// `{ done: false, blocked: null }`, the same bytes `hwt new` writes.
///
/// Writing the file here rather than shelling out to `hwt` keeps the dashboard's one write to the
/// same fields `hwt new` produces, with no dependency on `hwt` being on `PATH` and no
/// subprocess to fail behind a button.
pub fn adopt(roots: &Roots, id: &TaskId) -> std::io::Result<()> {
    write_meta_at(&meta_path(roots, id), &TaskMeta::default())
}

/// Record what a PR's state has become on a task's block, leaving the rest of `task.json` as it
/// is. Returns the block as now recorded, or `None` when `status` says nothing about the state,
/// or the task no longer waits on `url`, or the recorded state already matches.
pub fn record_block_pr_state(
    roots: &Roots,
    id: &TaskId,
    url: &str,
    status: PrStatus,
) -> std::io::Result<Option<Block>> {
    record_block_pr_state_at(&meta_path(roots, id), url, status)
}

fn record_block_pr_state_at(
    path: &Path,
    url: &str,
    status: PrStatus,
) -> std::io::Result<Option<Block>> {
    let state = match status {
        PrStatus::Merged => "MERGED",
        PrStatus::Closed => "CLOSED",
        PrStatus::Open | PrStatus::Draft => "OPEN",
        PrStatus::Unknown | PrStatus::Loading => return Ok(None),
    };
    let mut meta = read_meta_at(path);
    let Some(block) = meta.blocked.as_mut() else {
        return Ok(None);
    };
    if block.pr.as_deref() != Some(url) || block.pr_state.as_deref() == Some(state) {
        return Ok(None);
    }
    block.pr_state = Some(state.to_owned());
    let recorded = block.clone();
    write_meta_at(path, &meta)?;
    Ok(Some(recorded))
}

fn toggle_done_at(path: &Path) -> std::io::Result<bool> {
    let mut meta = read_meta_at(path);
    meta.done = !meta.done;
    write_meta_at(path, &meta)?;
    Ok(meta.done)
}

fn set_backlog_at(path: &Path, backlog: bool) -> std::io::Result<()> {
    let mut meta = read_meta_at(path);
    meta.backlog = Some(backlog);
    write_meta_at(path, &meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merged_pr_lifts_the_block_and_a_closed_one_holds_it() {
        let mut t = build_task(
            &Roots::under(&std::env::temp_dir().join("hdash_pr_block_fixture")),
            TaskId {
                project: "p".into(),
                topic: "t".into(),
                name: "pr-block".into(),
            },
            true,
            false,
            Some(Block {
                pr: Some("u".into()),
                ..Block::default()
            }),
        );
        assert!(t.blocked() && !t.ready());
        t.block.as_mut().unwrap().pr_state = Some("CLOSED".into());
        assert!(t.blocked() && !t.ready());
        t.block.as_mut().unwrap().pr_state = Some("MERGED".into());
        assert!(t.ready());
    }

    #[test]
    fn only_a_pr_that_can_still_change_is_checked() {
        let block = |state: Option<&str>| Block {
            pr: Some("u".into()),
            pr_state: state.map(str::to_owned),
            ..Block::default()
        };
        assert_eq!(block(None).pr_to_check(), Some("u"));
        assert_eq!(block(Some("OPEN")).pr_to_check(), Some("u"));
        assert_eq!(block(Some("MERGED")).pr_to_check(), None);
        assert_eq!(block(Some("CLOSED")).pr_to_check(), None);
        assert_eq!(Block::default().pr_to_check(), None);
    }

    #[test]
    fn recording_a_block_pr_state_keeps_the_rest_of_task_json() {
        let dir = std::env::temp_dir().join(format!("hdash_block_pr_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("task.json");
        std::fs::write(
            &path,
            r#"{"done":false,"blocked":{"reason":"r","pr":"u","pr_state":"OPEN"},"backlog":true}"#,
        )
        .unwrap();

        assert_eq!(
            record_block_pr_state_at(&path, "u", PrStatus::Open).unwrap(),
            None
        );
        assert_eq!(
            record_block_pr_state_at(&path, "other", PrStatus::Merged).unwrap(),
            None
        );
        assert_eq!(
            record_block_pr_state_at(&path, "u", PrStatus::Unknown).unwrap(),
            None
        );
        let recorded = record_block_pr_state_at(&path, "u", PrStatus::Merged)
            .unwrap()
            .unwrap();
        assert!(recorded.pr_merged());

        let meta = read_meta_at(&path);
        assert_eq!(meta.blocked.unwrap().reason.as_deref(), Some("r"));
        assert_eq!(meta.backlog, Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn aspects_are_read_from_the_aspects_dir() {
        let dir = std::env::temp_dir().join(format!("hdash_aspects_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (name, body) in [
            (
                "b-spike",
                r#"{"kind":"research","state":"reported","goal":"why"}"#,
            ),
            ("a-fresh", r#"{"goal":""}"#),
            ("d-code", r#"{"kind":"code","state":"pr"}"#),
            ("c-broken", "{not json"),
        ] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
            std::fs::write(dir.join(name).join("aspect.json"), body).unwrap();
        }
        let aspects = read_aspects(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let summary: Vec<_> = aspects
            .iter()
            .map(|a| {
                (
                    a.name.as_str(),
                    a.kind.as_str(),
                    a.state.as_str(),
                    a.goal.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("a-fresh", "research", "open", None),
                ("b-spike", "research", "reported", Some("why")),
                ("c-broken", "research", "unreadable", None),
                ("d-code", "code", "pr", None),
            ]
        );
        assert!(
            read_aspects(&dir).is_empty(),
            "a task without aspects has none"
        );
    }

    #[test]
    fn an_aspect_carries_its_pr_outcome_open_items_and_own_build_state() {
        let dir = std::env::temp_dir().join(format!("hdash_aspect_full_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let aspect = dir.join("export");
        std::fs::create_dir_all(&aspect).unwrap();
        std::fs::write(
            aspect.join("aspect.json"),
            r#"{"kind":"code","state":"pr","goal":"g","base":null,"created":"2026-11-08",
                "pr":"https://github.com/o/r/pull/11","outcome":"merged it",
                "open_items":[{"text":"web loading","issue":null},
                              {"text":"tiff","issue":"tiff-export"},
                              {"text":"  ","issue":null}]}"#,
        )
        .unwrap();
        std::fs::write(
            aspect.join("build-task.json"),
            r#"{"phase":"await-fix","waiting_on":"red check",
                "todo":[{"text":"plan","status":"done"},{"text":"build","status":"active"}]}"#,
        )
        .unwrap();
        let read = read_aspects(&dir).remove(0);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read.base, None);
        assert_eq!(read.created.as_deref(), Some("2026-11-08"));
        assert_eq!(
            read.pr.as_ref().map(|p| (p.url.as_str(), p.status)),
            Some(("https://github.com/o/r/pull/11", PrStatus::Loading))
        );
        assert_eq!(read.outcome.as_deref(), Some("merged it"));
        assert_eq!(
            read.open_items,
            vec![
                OpenItem {
                    text: "web loading".into(),
                    issue: None
                },
                OpenItem {
                    text: "tiff".into(),
                    issue: Some("tiff-export".into())
                },
            ]
        );
        assert_eq!(read.uncovered_items(), 1);
        assert_eq!(read.phase.as_deref(), Some("await-fix"));
        assert_eq!(read.waiting_on.as_deref(), Some("red check"));
        assert_eq!(TodoCount::of(&read.todo), TodoCount { open: 1, done: 1 });
    }

    #[test]
    fn build_state_reads_todo_and_waiting_on() {
        let state = parse_build_state(
            r#"{"phase":"implement","waiting_on":"answer GATE C",
                "todo":[{"text":"write the test","status":"done"},
                        {"text":"fix it","status":"active"},
                        {"text":"","status":"pending"},
                        {"text":"ship it","status":"later"},
                        {"text":"old idea","status":"dropped"}]}"#,
        );
        assert_eq!(state.phase.as_deref(), Some("implement"));
        assert_eq!(state.waiting_on.as_deref(), Some("answer GATE C"));
        let statuses: Vec<_> = state.todo.iter().map(|t| t.status).collect();
        assert_eq!(
            statuses,
            [
                TodoStatus::Done,
                TodoStatus::Active,
                TodoStatus::Pending,
                TodoStatus::Dropped
            ]
        );
        assert_eq!(TodoCount::of(&state.todo), TodoCount { open: 2, done: 2 });

        let bare = parse_build_state(r#"{"phase":"plan","waiting_on":null}"#);
        assert!(bare.todo.is_empty());
        assert_eq!(bare.waiting_on, None);
    }

    #[test]
    fn scan_unions_across_projects() {
        let home = std::env::temp_dir().join("hdash_scan_fixture");
        let _ = std::fs::remove_dir_all(&home);
        let roots = Roots::under(&home);

        let notes_root = home.join("fun_repos/hobby-mimisbrunnr/notes");
        let wt_root = home.join("fun_worktrees");

        std::fs::create_dir_all(wt_root.join("projA/features/live-task/.git")).unwrap();
        std::fs::create_dir_all(notes_root.join("projA/features/live-task")).unwrap();
        std::fs::write(
            notes_root.join("projA/features/live-task/task.json"),
            r#"{"done": false, "blocked": null}"#,
        )
        .unwrap();

        std::fs::create_dir_all(notes_root.join("projA/bugs/gone-task")).unwrap();
        std::fs::write(
            notes_root.join("projA/bugs/gone-task/task.json"),
            r#"{"done": true, "blocked": {"until": "2020-01-01"}}"#,
        )
        .unwrap();

        std::fs::create_dir_all(wt_root.join("projB/features/other-task/.git")).unwrap();
        std::fs::create_dir_all(notes_root.join("projB/features/other-task")).unwrap();
        std::fs::write(
            notes_root.join("projB/features/other-task/task.json"),
            r#"{"done": false, "blocked": null}"#,
        )
        .unwrap();

        std::fs::create_dir_all(wt_root.join("projA/features/orphan-task/.git")).unwrap();

        std::fs::create_dir_all(notes_root.join("projA/bugs/just-notes")).unwrap();
        std::fs::write(notes_root.join("projA/bugs/just-notes/report.md"), "x").unwrap();

        let tasks = scan_tasks(&roots);
        let find = |project: &str, name: &str| {
            tasks
                .iter()
                .find(|t| t.id.project == project && t.id.name == name)
                .unwrap_or_else(|| panic!("{project}/{name} scanned"))
        };

        assert_eq!(tasks.len(), 4, "three tasks and one orphan");
        assert!(
            !tasks.iter().any(|t| t.id.name == "just-notes"),
            "a notes dir without task.json is not a task"
        );

        let orphan = find("projA", "orphan-task");
        assert!(!orphan.adopted);
        assert!(orphan.present);
        assert!(!orphan.done);
        assert_eq!(
            tasks.last().map(|t| t.id.name.as_str()),
            Some("orphan-task"),
            "orphans are appended after the notes tasks"
        );
        assert!(
            tasks.iter().take(3).all(|t| t.adopted),
            "every notes-sourced task is adopted"
        );

        adopt(&roots, &orphan.id).unwrap();
        let rescanned = scan_tasks(&roots);
        assert_eq!(rescanned.len(), 4);
        assert!(
            rescanned
                .iter()
                .find(|t| t.id.name == "orphan-task")
                .expect("still there")
                .adopted,
            "an adopted worktree is a task"
        );
        assert_eq!(
            std::fs::read_to_string(notes_root.join("projA/features/orphan-task/task.json"))
                .unwrap(),
            "{\n  \"done\": false,\n  \"blocked\": null\n}",
            "adopt writes exactly what hwt writes"
        );

        let live = find("projA", "live-task");
        assert!(live.present);
        assert_eq!(live.id.topic, "features");
        assert_eq!(live.repos.len(), 1, "a hobby task is exactly one repo");
        assert_eq!(live.repos[0].path, live.dir, "the repo IS the task dir");
        assert_eq!(live.id.rel(), "projA/features/live-task");

        let other = find("projB", "other-task");
        assert_eq!(other.id.topic, "features");

        let gone = find("projA", "gone-task");
        assert!(!gone.present, "removed task has no worktree");
        assert!(gone.done);
        assert!(gone.block.is_some(), "blocked parsed from task.json");
        assert!(gone.ready(), "a past until-date means the block has lifted");
        assert_eq!(gone.repos.len(), 1);
        assert_eq!(gone.repos[0].path, gone.dir);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn only_merged_and_closed_are_terminal() {
        assert!(PrStatus::Merged.is_terminal());
        assert!(PrStatus::Closed.is_terminal());
        assert!(!PrStatus::Open.is_terminal());
        assert!(!PrStatus::Draft.is_terminal());
        assert!(!PrStatus::Loading.is_terminal());
        assert!(!PrStatus::Unknown.is_terminal());
    }

    #[test]
    fn toggle_done_round_trips_hwt_nu_bytes() {
        let dir = std::env::temp_dir().join("hdash_meta_fixture");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested/task.json");

        let undone = "{\n  \"done\": false,\n  \"blocked\": null\n}";
        let done = "{\n  \"done\": true,\n  \"blocked\": null\n}";

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, undone).unwrap();

        assert!(toggle_done_at(&path).unwrap(), "toggled to done");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            done,
            "a done toggle changes the done flag and nothing else"
        );

        assert!(!toggle_done_at(&path).unwrap(), "toggled back to not-done");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            undone,
            "a round trip leaves the file exactly as hwt wrote it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

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
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"done\": true,\n  \"blocked\": {\n    \"until\": \"2030-01-01\",\n    \"reason\": \"waiting\"\n  }\n}"
        );

        std::fs::write(&path, r#"{"done": false, "blocked": {}}"#).unwrap();
        toggle_done_at(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"done\": true,\n  \"blocked\": {}\n}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_backlog_appends_after_blocked_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join("hdash_meta_backlog_fixture");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("task.json");
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(&path, "{\n  \"done\": false,\n  \"blocked\": null\n}").unwrap();
        assert_eq!(
            read_meta_at(&path).backlog,
            None,
            "an unmarked file records none"
        );

        set_backlog_at(&path, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"done\": false,\n  \"blocked\": null,\n  \"backlog\": true\n}"
        );
        toggle_done_at(&path).unwrap();
        assert_eq!(
            read_meta_at(&path).backlog,
            Some(true),
            "a done toggle keeps the mark"
        );

        set_backlog_at(&path, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"done\": true,\n  \"blocked\": null,\n  \"backlog\": false\n}",
            "unmarking records false rather than dropping the key"
        );

        std::fs::write(
            &path,
            r#"{"done": false, "blocked": {"reason": "waiting"}}"#,
        )
        .unwrap();
        set_backlog_at(&path, true).unwrap();
        let back = read_meta_at(&path);
        assert!(!back.done);
        assert_eq!(
            back.blocked.and_then(|b| b.reason).as_deref(),
            Some("waiting")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pr_cache_round_trips_and_tolerates_bad_files() {
        let dir = std::env::temp_dir().join("hdash_pr_cache_fixture");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested/pr-status.json");

        assert!(
            read_pr_cache_at(&path).is_empty(),
            "missing file reads as empty"
        );

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

        std::fs::write(&path, b"{ not json").unwrap();
        assert!(
            read_pr_cache_at(&path).is_empty(),
            "corrupt file reads as empty"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn issue_title_comes_from_the_frontmatter_only() {
        let doc = "---\ntitle: \"Seasons \\\"and\\\" weather\"\ncreated: 2026-10-18\n---\n\ntitle: body\n";
        assert_eq!(
            frontmatter_title(doc).as_deref(),
            Some("Seasons \"and\" weather")
        );
        assert_eq!(
            frontmatter_title("---\ntitle: Bare\n---\n").as_deref(),
            Some("Bare")
        );
        assert_eq!(frontmatter_title("# title: no frontmatter\n"), None);
        assert_eq!(
            frontmatter_title("---\ncreated: x\n---\ntitle: late\n"),
            None
        );
        assert_eq!(frontmatter_title("---\ntitle: \"\"\n---\n"), None);
    }

    #[test]
    fn branch_refs_are_named_without_their_remote() {
        let set = parse_branch_refs(
            "refs/heads/main\nrefs/heads/feature/x\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/gh-9-y\nrefs/remotes/up/a/b\n",
        );
        let mut names: Vec<&str> = set.iter().map(String::as_str).collect();
        names.sort();
        assert_eq!(names, vec!["a/b", "feature/x", "gh-9-y", "main"]);
    }
}
