//! The union of the two halves: one **item** per piece of work, whatever stage it is at.
//!
//! The pre-merge tools drew the same object twice — `hgh-issues` walked
//! `~/fun_worktrees` to decide whether an issue had a worktree, and `hwt-overview` drew
//! that worktree as a task card with no idea an issue existed. Here the task half already
//! holds the tree, so the match is a pure in-memory join and the filesystem walk is gone.
//!
//! The join is recomputed per rebuild rather than folded into a merged list as fetches
//! land. That keeps the three async drains (git, PR, issues) independent: each folds into
//! its own half and nothing has to re-run the join. [`Model::items`] indexes tasks by
//! project first, so the recompute stays cheap even though `~/fun_repos` holds upstream
//! clones that report hundreds of issues and own no worktrees at all.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use bevy::prelude::*;

use super::issue::{Issue, ProjectIssues, matches_issue, scan_projects};
use super::launch::LaunchSpec;
use super::task::{Task, TaskId, scan_tasks};

/// Stable identity for one item, for the session-scoped `hidden` set. A joined item is
/// keyed by its **task** half: the task is the durable artifact, and the issue number is
/// recoverable from its name.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ItemKey {
    /// A task (joined or not).
    Task(TaskId),
    /// An issue with no worktree yet.
    Issue { project: String, number: u64 },
}

/// Where an item sits in the issue → worktree → PR → done lifecycle. Drives the card's
/// primary button, and nothing else needs to ask "is this an issue or a task?".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// A worktree with no `task.json` in the notes repo: **Adopt**.
    ///
    /// The notes repo is what records a task, so an orphan is a worktree nothing has recorded:
    /// a state to resolve before the work can be tracked, not work to pick up. **Adopt** writes
    /// the missing file; a scan never does.
    Orphan,
    /// An issue with no usable `topic:` label. Nothing to launch — the card says so.
    Untagged,
    /// A tagged issue with no worktree: **Start task**.
    Startable,
    /// A task whose worktree was removed but whose notes survive: **Restore**.
    Removed,
    /// A live worktree: **Open hcdev**.
    Active,
    /// A task marked done in its `task.json`.
    Done,
}

/// One piece of work: a task, an issue, or both when they are the same work.
///
/// Invariant: at least one of `task` / `issue` is `Some`, guaranteed by construction in
/// [`Model::items`].
#[derive(Clone, Copy, Debug)]
pub struct Item<'a> {
    /// The project both halves belong to (the join never crosses projects).
    pub project: &'a str,
    /// The worktree/notes half.
    pub task: Option<&'a Task>,
    /// The GitHub half.
    pub issue: Option<&'a Issue>,
}

impl<'a> Item<'a> {
    /// Session-hide identity.
    pub fn key(&self) -> ItemKey {
        match (self.task, self.issue) {
            (Some(task), _) => ItemKey::Task(task.id.clone()),
            (None, Some(issue)) => ItemKey::Issue {
                project: self.project.to_owned(),
                number: issue.number,
            },
            (None, None) => unreachable!("an item always has a task or an issue"),
        }
    }

    /// Lifecycle stage — what the primary button will be.
    pub fn stage(&self) -> Stage {
        match self.task {
            Some(task) if !task.adopted => Stage::Orphan,
            Some(task) if task.done => Stage::Done,
            Some(task) if !task.present => Stage::Removed,
            Some(_) => Stage::Active,
            // Issue-only: startable exactly when it names a topic.
            None => match self.issue.and_then(|i| i.topic.as_ref()) {
                Some(_) => Stage::Startable,
                None => Stage::Untagged,
            },
        }
    }

    /// The topic: a task's directory level, or an unstarted issue's `topic:` label.
    pub fn topic(&self) -> Option<&str> {
        match self.task {
            Some(task) => Some(&task.id.topic),
            None => self.issue.and_then(|i| i.topic.as_deref()),
        }
    }

    /// The task/branch name — an existing one, or the one a **Start task** would create.
    pub fn task_name(&self) -> &str {
        match self.task {
            Some(task) => &task.id.name,
            None => self.issue.map(|i| i.task.as_str()).unwrap_or_default(),
        }
    }

    /// The card's headline: the issue title when there is one (it says what the work *is*),
    /// else the task name.
    pub fn title(&self) -> &str {
        match self.issue {
            Some(issue) => &issue.title,
            None => self.task_name(),
        }
    }

    /// What a launch needs, or `None` for an untagged issue — which is exactly when there
    /// is no launch action to offer.
    pub fn launch_spec(&self) -> Option<LaunchSpec> {
        match self.task {
            // The worktree path is the authority, not the issue's label.
            Some(task) => Some(LaunchSpec::from_task(&task.id)),
            None => {
                let issue = self.issue?;
                Some(LaunchSpec {
                    project: self.project.to_owned(),
                    topic: issue.topic.clone()?,
                    task: issue.task.clone(),
                })
            }
        }
    }

    fn rank(&self) -> u8 {
        if let Some(task) = self.task {
            if !task.adopted {
                return 0;
            }
            if task.done {
                return 8;
            }
            if !task.present {
                return 6; // removed: restore it before it is actionable again
            }
            if task.ready() {
                return 1; // a block that has lifted is the loudest signal there is
            }
            if task.blocked() {
                return 4;
            }
            if task.awaiting_review() > 0 {
                return 2;
            }
            return 3;
        }
        match self.stage() {
            Stage::Startable => 5,
            _ => 7, // untagged
        }
    }

    fn activity(&self) -> Option<SystemTime> {
        let task = self.task.and_then(|t| t.last_active);
        let issue = self.issue.and_then(|i| i.updated);
        task.max(issue)
    }
}

/// UI selection / filter state.
#[derive(Resource)]
pub struct UiState {
    /// Selected project, or `None` for "All". Project is the coarsest useful axis for
    /// hobby work; topic rides along as a card label, since an unstarted issue has a
    /// topic *label* and no topic *directory* to group under.
    pub selected_project: Option<String>,
    /// Whether done tasks are listed. **Off** by default: there are far more done tasks
    /// than open ones, and they would swamp the merged list.
    pub show_done: bool,
    /// Whether issues without a usable `topic:` label are listed. **On** by default: an
    /// untagged issue is the one item this app cannot act on, so hiding it by default
    /// would hide the work that most needs attention.
    pub show_untagged: bool,
    /// Items hidden for this session only (in-memory; cleared on Refresh).
    pub hidden: HashSet<ItemKey>,
    /// Tasks whose PR list is expanded (in-memory; collapsed by default).
    pub pr_expanded: HashSet<TaskId>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_project: None,
            show_done: false,
            show_untagged: true,
            hidden: HashSet::new(),
            pr_expanded: HashSet::new(),
        }
    }
}

/// The scanned model: both halves, kept separate and joined on demand.
#[derive(Resource, Default)]
pub struct Model {
    /// Tasks, in `(project, topic, name)` order — see [`scan_tasks`].
    pub tasks: Vec<Task>,
    /// Projects under `~/fun_repos`, each with its issue fetch.
    pub projects: Vec<ProjectIssues>,
}

/// Scan both halves. Issues arrive async; git and PR status likewise.
pub fn scan() -> Model {
    Model {
        tasks: scan_tasks(),
        projects: scan_projects(),
    }
}

impl Model {
    /// Find a task by id (mutable), for folding a git/PR result in.
    pub fn task_mut(&mut self, id: &TaskId) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == *id)
    }

    /// Find a project's entry by name (mutable), for folding a fetch result in.
    pub fn project_mut(&mut self, name: &str) -> Option<&mut ProjectIssues> {
        self.projects.iter_mut().find(|p| p.project.name == name)
    }

    /// Re-resolve one item from a card button's key. Buttons carry the key rather than the
    /// data so a click always acts on the current model, not on whatever was true when the
    /// card was drawn.
    pub fn item(&self, key: &ItemKey) -> Option<Item<'_>> {
        self.items().into_iter().find(|i| i.key() == *key)
    }

    /// The merged, display-ordered item list: every task, every issue, and one item where
    /// the two are the same work.
    ///
    /// An issue joins the first task in its project whose name [`matches_issue`] its
    /// number. `tasks` is in `(project, topic, name)` order, so "first" is deterministic;
    /// a second matching task keeps its own unjoined card rather than silently vanishing.
    pub fn items(&self) -> Vec<Item<'_>> {
        // Task indices per project. Without this the join is issues × tasks, and
        // `~/fun_repos` holds upstream clones (a `bevy` checkout reports 200 open issues
        // and has no worktrees at all) — indexing first makes those cost nothing, which
        // matters because `items()` runs several times per rebuild.
        let mut by_project: HashMap<&str, Vec<usize>> = HashMap::new();
        for (idx, task) in self.tasks.iter().enumerate() {
            by_project
                .entry(task.id.project.as_str())
                .or_default()
                .push(idx);
        }

        // task index -> the issue that claimed it, and the issues that found a task.
        let mut joined: HashMap<usize, &Issue> = HashMap::new();
        let mut consumed: HashSet<(&str, u64)> = HashSet::new();

        for entry in &self.projects {
            let project = entry.project.name.as_str();
            let Some(candidates) = by_project.get(project) else {
                continue; // no worktrees in this project, so nothing can join
            };
            for issue in &entry.issues {
                let hit = candidates.iter().copied().find(|idx| {
                    !joined.contains_key(idx)
                        && matches_issue(&self.tasks[*idx].id.name, issue.number)
                });
                if let Some(idx) = hit {
                    joined.insert(idx, issue);
                    consumed.insert((project, issue.number));
                }
            }
        }

        let mut items: Vec<Item<'_>> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| Item {
                project: &task.id.project,
                task: Some(task),
                issue: joined.get(&idx).copied(),
            })
            .collect();

        for entry in &self.projects {
            let project = entry.project.name.as_str();
            items.extend(
                entry
                    .issues
                    .iter()
                    .filter(|i| !consumed.contains(&(project, i.number)))
                    .map(|issue| Item {
                        project,
                        task: None,
                        issue: Some(issue),
                    }),
            );
        }

        // Rank, then most-recent activity, then name — so the order is total and stable.
        items.sort_by(|a, b| {
            a.rank()
                .cmp(&b.rank())
                .then(b.activity().cmp(&a.activity()))
                .then(a.task_name().cmp(b.task_name()))
        });
        items
    }
}

/// Whether an item is currently rendered — and therefore whether its `gh`/git calls are
/// worth making. Shared by the list rebuild and the async dispatchers so fetches track
/// exactly what the user can see.
///
/// With **no project selected** the list is tasks only: issues are fetched per selected
/// project, so an issue-only item under "All" would show whichever projects happened to be
/// visited this session and nothing else.
///
/// An orphan is never hidden by a content toggle — it is the one item the app is asking the
/// user to resolve.
///
/// Under "All" the list is tasks only: `Startable` and `Untagged` are the issue-only stages,
/// and `gh` is queried per selected project, so showing them here would report whichever
/// projects happened to be visited this session rather than a whole list.
pub fn item_visible(item: &Item<'_>, ui: &UiState) -> bool {
    if let Some(sel) = &ui.selected_project
        && item.project != sel
    {
        return false;
    }
    let stage = item.stage();
    if ui.selected_project.is_none() && matches!(stage, Stage::Startable | Stage::Untagged) {
        return false;
    }
    match stage {
        Stage::Done if !ui.show_done => return false,
        Stage::Untagged if !ui.show_untagged => return false,
        _ => {}
    }
    !ui.hidden.contains(&item.key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::issue::{FetchState, Project};
    use crate::model::task::{Block, GitInfo, Pr, PrStatus, Repo};
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    fn task(project: &str, topic: &str, name: &str) -> Task {
        let id = TaskId {
            project: project.into(),
            topic: topic.into(),
            name: name.into(),
        };
        let dir = PathBuf::from("/nowhere").join(id.rel());
        Task {
            repos: vec![Repo {
                path: dir.clone(),
                git: GitInfo::default(),
            }],
            id,
            dir,
            adopted: true,
            present: true,
            done: false,
            block: None,
            prs: Vec::new(),
            subtasks: Vec::new(),
            last_active: None,
        }
    }

    fn issue(number: u64, title: &str, topic: Option<&str>) -> Issue {
        Issue {
            number,
            title: title.into(),
            url: format!("https://gh/{number}"),
            labels: topic
                .map(|t| vec![format!("topic:{t}")])
                .unwrap_or_default(),
            updated: None,
            author: None,
            topic: topic.map(str::to_owned),
            task: crate::model::issue::task_name(number, title),
        }
    }

    fn model(tasks: Vec<Task>, projects: Vec<(&str, Vec<Issue>)>) -> Model {
        Model {
            tasks,
            projects: projects
                .into_iter()
                .map(|(name, issues)| ProjectIssues {
                    project: Project {
                        name: name.into(),
                        path: PathBuf::from("/nowhere").join(name),
                        has_notes: true,
                    },
                    state: FetchState::Ready,
                    issues,
                })
                .collect(),
        }
    }

    #[test]
    fn join_pairs_issues_with_their_worktrees() {
        let m = model(
            vec![
                // Pre-convention name — matched by number, not by spelling.
                task("wusel", "perf", "issue-7_chunk_loading"),
                // No number at all: never joins, always its own card.
                task("hobby-tooling", "task_management", "unify_uis"),
                // Same number as wusel's issue 7, but a different project.
                task("hobby-tooling", "bugs", "gh-7-unrelated"),
            ],
            vec![
                (
                    "wusel",
                    vec![
                        issue(7, "Chunk loading", Some("perf")),
                        issue(12, "Not started yet", Some("features")),
                    ],
                ),
                ("hobby-tooling", vec![]),
            ],
        );

        let items = m.items();
        assert_eq!(items.len(), 4, "3 tasks + 1 unjoined issue");

        let by_name = |n: &str| {
            items
                .iter()
                .find(|i| i.task_name() == n)
                .unwrap_or_else(|| panic!("{n} present"))
        };

        // Joined: one item carrying both halves.
        let joined = by_name("issue-7_chunk_loading");
        assert!(joined.task.is_some() && joined.issue.is_some());
        assert_eq!(joined.issue.unwrap().number, 7);
        assert_eq!(joined.stage(), Stage::Active);
        assert_eq!(joined.title(), "Chunk loading", "the issue names the work");
        assert_eq!(
            joined.key(),
            ItemKey::Task(joined.task.unwrap().id.clone()),
            "a joined item is keyed by its task half"
        );

        // The task with no issue number stays a bare task.
        let bare = by_name("unify_uis");
        assert!(bare.issue.is_none());
        assert_eq!(
            bare.title(),
            "unify_uis",
            "no issue, so the name is the title"
        );

        // Same number, other project: not joined.
        let other = by_name("gh-7-unrelated");
        assert!(other.issue.is_none(), "the join never crosses projects");

        // The issue with no worktree is its own item.
        let unstarted = by_name("gh-12-not-started-yet");
        assert!(unstarted.task.is_none());
        assert_eq!(unstarted.stage(), Stage::Startable);
    }

    #[test]
    fn a_second_matching_task_keeps_its_own_card() {
        let m = model(
            // scan_tasks yields (project, topic, name) order; mimic it.
            vec![
                task("wusel", "bugs", "gh-42-a"),
                task("wusel", "perf", "gh-42-b"),
            ],
            vec![("wusel", vec![issue(42, "Two worktrees", Some("bugs"))])],
        );
        let items = m.items();
        assert_eq!(items.len(), 2);
        let a = items.iter().find(|i| i.task_name() == "gh-42-a").unwrap();
        let b = items.iter().find(|i| i.task_name() == "gh-42-b").unwrap();
        assert!(a.issue.is_some(), "first in order claims the issue");
        assert!(b.issue.is_none(), "the second is not silently dropped");
    }

    #[test]
    fn stages_follow_the_lifecycle() {
        let mut removed = task("p", "t", "gone");
        removed.present = false;
        let mut done = task("p", "t", "finished");
        done.done = true;
        // A done task whose worktree is also gone is still Done — done wins.
        let mut done_and_gone = task("p", "t", "finished-and-gone");
        done_and_gone.done = true;
        done_and_gone.present = false;

        let tasks = vec![task("p", "t", "live"), removed, done, done_and_gone];
        let m = model(
            tasks,
            vec![(
                "p",
                vec![
                    issue(1, "Tagged", Some("features")),
                    issue(2, "No label", None),
                ],
            )],
        );
        let items = m.items();
        let stage = |n: &str| items.iter().find(|i| i.task_name() == n).unwrap().stage();

        assert_eq!(stage("live"), Stage::Active);
        assert_eq!(stage("gone"), Stage::Removed);
        assert_eq!(stage("finished"), Stage::Done);
        assert_eq!(stage("finished-and-gone"), Stage::Done);
        assert_eq!(stage("gh-1-tagged"), Stage::Startable);
        assert_eq!(stage("gh-2-no-label"), Stage::Untagged);

        // Only the untagged issue has nothing to launch.
        for item in &items {
            let expected = item.stage() != Stage::Untagged;
            assert_eq!(
                item.launch_spec().is_some(),
                expected,
                "{} launch spec",
                item.task_name()
            );
        }
        // A task's spec comes from its path, even when an issue label disagrees.
        let live = items.iter().find(|i| i.task_name() == "live").unwrap();
        assert_eq!(live.launch_spec().unwrap().topic, "t");
    }

    #[test]
    fn merged_sort_order() {
        let day = |n: u64| UNIX_EPOCH + Duration::from_secs(n * 86_400);

        let mut ready = task("p", "t", "z-ready");
        ready.block = Some(Block {
            until: Some("2020-01-01".into()),
            reason: None,
        });
        let mut in_review = task("p", "t", "z-in-review");
        in_review.prs = vec![Pr {
            url: "https://gh/pr/1".into(),
            repo: None,
            title: None,
            opened: None,
            status: PrStatus::Open,
            number: Some(1),
            live_title: None,
        }];
        let active = task("p", "t", "z-active");
        let mut blocked = task("p", "t", "z-blocked");
        blocked.block = Some(Block {
            until: Some("2999-01-01".into()),
            reason: None,
        });
        let mut removed = task("p", "t", "z-removed");
        removed.present = false;
        let mut done = task("p", "t", "z-done");
        done.done = true;

        let m = model(
            vec![ready, in_review, active, blocked, removed, done],
            vec![(
                "p",
                vec![
                    issue(1, "Startable", Some("features")),
                    issue(2, "Untagged", None),
                ],
            )],
        );
        let items = m.items();
        let order: Vec<&str> = items.iter().map(|i| i.task_name()).collect();
        assert_eq!(
            order,
            vec![
                "z-ready",
                "z-in-review",
                "z-active",
                "z-blocked",
                "gh-1-startable",
                "z-removed",
                "gh-2-untagged",
                "z-done",
            ]
        );

        // Ties inside a rank break by most-recent activity, then name.
        let mut old = task("p", "t", "a-old");
        old.last_active = Some(day(100));
        let mut fresh = task("p", "t", "z-fresh");
        fresh.last_active = Some(day(200));
        let m = model(vec![old, fresh], vec![]);
        let items = m.items();
        let order: Vec<&str> = items.iter().map(|i| i.task_name()).collect();
        assert_eq!(
            order,
            vec!["z-fresh", "a-old"],
            "recent first, despite the name"
        );
    }

    #[test]
    fn visibility_honours_filters_and_defaults() {
        let mut done = task("p", "t", "finished");
        done.done = true;
        let m = model(
            vec![
                task("p", "t", "live"),
                done,
                task("other", "t", "elsewhere"),
            ],
            vec![(
                "p",
                vec![
                    issue(1, "Tagged", Some("features")),
                    issue(2, "Untagged", None),
                ],
            )],
        );
        let items = m.items();
        let visible = |ui: &UiState| -> Vec<&str> {
            items
                .iter()
                .filter(|i| item_visible(i, ui))
                .map(|i| i.task_name())
                .collect()
        };

        // Defaults: no project selected, so the list is tasks only — done hidden, and no
        // issue at all. The sort is global, not per project, so two active tasks from
        // different projects interleave and break the tie by name.
        let mut ui = UiState::default();
        assert_eq!(
            visible(&ui),
            vec!["elsewhere", "live"],
            "\"All\" is a task list: issues are fetched per selected project"
        );

        ui.show_done = true;
        assert!(visible(&ui).contains(&"finished"));
        ui.show_done = false;

        // Selecting the project is what brings its issues into the list.
        ui.selected_project = Some("p".into());
        assert_eq!(visible(&ui), vec!["live", "gh-1-tagged", "gh-2-untagged"]);
        assert!(!visible(&ui).contains(&"elsewhere"));

        ui.show_untagged = false;
        assert!(!visible(&ui).contains(&"gh-2-untagged"));
        assert!(
            visible(&ui).contains(&"gh-1-tagged"),
            "the toggle only hides untagged ones"
        );
        ui.show_untagged = true;

        // Session hide, by key — an issue-only item is keyed by (project, number).
        ui.hidden.insert(ItemKey::Issue {
            project: "p".into(),
            number: 1,
        });
        assert!(!visible(&ui).contains(&"gh-1-tagged"));
        ui.hidden.insert(ItemKey::Task(TaskId {
            project: "p".into(),
            topic: "t".into(),
            name: "live".into(),
        }));
        assert!(!visible(&ui).contains(&"live"));
    }

    #[test]
    fn an_orphan_worktree_is_its_own_stage() {
        let mut orphan = task("p", "t", "gh-3-stray");
        orphan.adopted = false;
        let mut ready = task("p", "t", "a-ready");
        ready.block = Some(Block {
            until: Some("2020-01-01".into()),
            reason: None,
        });

        let m = model(
            vec![ready, orphan],
            vec![("p", vec![issue(3, "Stray", Some("features"))])],
        );
        let items = m.items();
        let order: Vec<&str> = items.iter().map(|i| i.task_name()).collect();
        assert_eq!(
            order,
            vec!["gh-3-stray", "a-ready"],
            "an orphan outranks even a lifted block"
        );

        let stray = items
            .iter()
            .find(|i| i.task_name() == "gh-3-stray")
            .unwrap();
        assert_eq!(stray.stage(), Stage::Orphan);
        assert!(stray.issue.is_some(), "an orphan still joins its issue");
        assert!(
            stray.launch_spec().is_some(),
            "the worktree path is a launch spec whether or not it is recorded"
        );

        // Neither content toggle can hide it, and it shows under "All" — where the
        // issue-only items do not.
        let mut ui = UiState::default();
        for (done, untagged) in [(false, false), (false, true), (true, false), (true, true)] {
            ui.show_done = done;
            ui.show_untagged = untagged;
            assert!(
                items
                    .iter()
                    .filter(|i| item_visible(i, &ui))
                    .any(|i| i.stage() == Stage::Orphan),
                "orphan visible with show_done={done} show_untagged={untagged}"
            );
        }
    }
}
