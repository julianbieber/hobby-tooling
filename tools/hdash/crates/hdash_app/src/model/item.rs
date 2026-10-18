//! The list the dashboard draws: one **item** per task, its lifecycle stage, the order the
//! items are shown in, and which of them the current filters let through.

use std::collections::HashSet;

use bevy::prelude::*;

use super::launch::LaunchSpec;
use super::project::{Project, scan_projects};
use super::task::{Task, TaskId, scan_tasks};

/// Where an item sits in the notes → worktree → PR → done lifecycle. Drives the card's
/// primary button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// A worktree with no `task.json` in the notes repo: **Adopt**.
    ///
    /// The notes repo is what records a task, so an orphan is a worktree nothing has recorded:
    /// a state to resolve before the work can be tracked, not work to pick up. **Adopt** writes
    /// the missing file; a scan never does.
    Orphan,
    /// A recorded task with no worktree and no branch yet: **Start task**.
    NotStarted,
    /// A task whose worktree was removed but whose branch and notes survive: **Restore**.
    Removed,
    /// A live worktree: **Open hcdev**.
    Active,
    /// A task marked done in its `task.json`.
    Done,
    /// A task under the reserved `coordinator` topic: **Open coordinator**.
    ///
    /// Outranks `Removed` because a coordinator never has a worktree, so without this
    /// it would sit at **Restore** forever. A done coordinator is `Done` like any other
    /// task, so the done toggle hides it; its card still opens the coordinator session.
    Coordinator,
}

/// One piece of work, as the list draws it.
#[derive(Clone, Copy, Debug)]
pub struct Item<'a> {
    /// The task behind the card.
    pub task: &'a Task,
}

impl<'a> Item<'a> {
    /// The project the task belongs to.
    pub fn project(&self) -> &'a str {
        &self.task.id.project
    }

    /// Lifecycle stage — what the primary button will be.
    pub fn stage(&self) -> Stage {
        let task = self.task;
        if !task.adopted {
            Stage::Orphan
        } else if task.done {
            Stage::Done
        } else if task.is_coordinator() {
            Stage::Coordinator
        } else if !task.present && !task.has_branch {
            Stage::NotStarted
        } else if !task.present {
            Stage::Removed
        } else {
            Stage::Active
        }
    }

    /// Whether the task's `task.json` records it backlogged. An orphan has no file, so never.
    pub fn backlogged(&self) -> bool {
        self.task.adopted && self.task.backlog == Some(true)
    }

    /// The task's topic: the middle level of its path.
    pub fn topic(&self) -> &'a str {
        &self.task.id.topic
    }

    /// The task/branch name.
    pub fn task_name(&self) -> &'a str {
        &self.task.id.name
    }

    /// The card's headline: the `issue.md` title when there is one, else the task name.
    pub fn title(&self) -> &'a str {
        self.task.title.as_deref().unwrap_or(&self.task.id.name)
    }

    /// What a launch needs, from the task's path.
    pub fn launch_spec(&self) -> LaunchSpec {
        LaunchSpec::from_task(&self.task.id)
    }

    fn rank(&self) -> u8 {
        let task = self.task;
        match self.stage() {
            Stage::Orphan => 0,
            Stage::Done => 8,
            Stage::NotStarted => 5,
            Stage::Removed | Stage::Coordinator => 6,
            _ if task.ready() => 1,
            _ if task.blocked() => 4,
            _ if task.awaiting_review() > 0 => 2,
            _ => 3,
        }
    }
}

/// UI selection / filter state. The default is "All", open work, nothing hidden.
#[derive(Resource, Default)]
pub struct UiState {
    /// Selected project, or `None` for "All". Topic rides along as a card label.
    pub selected_project: Option<String>,
    /// Whether done tasks are listed. **Off** by default: there are far more done tasks
    /// than open ones, and they would swamp the list.
    pub show_done: bool,
    /// Whether the list shows the backlog instead of the open work. **Off** by default: the
    /// backlog exists to keep the normal list readable.
    pub backlog_mode: bool,
    /// Tasks hidden for this session only (in-memory; cleared on Refresh).
    pub hidden: HashSet<TaskId>,
    /// Tasks whose PR list is expanded (in-memory; collapsed by default).
    pub pr_expanded: HashSet<TaskId>,
}

/// The scanned model.
#[derive(Resource, Default)]
pub struct Model {
    /// Tasks, in `(project, topic, name)` order — see [`scan_tasks`].
    pub tasks: Vec<Task>,
    /// Projects with a notes directory — see [`scan_projects`].
    pub projects: Vec<Project>,
}

/// Scan the notes tree and the worktrees. Git and PR status arrive async.
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

    /// Re-resolve one item from a card button's id. Buttons carry the id rather than the
    /// data so a click always acts on the current model, not on whatever was true when the
    /// card was drawn.
    pub fn item(&self, id: &TaskId) -> Option<Item<'_>> {
        self.tasks
            .iter()
            .find(|t| t.id == *id)
            .map(|task| Item { task })
    }

    /// Every task as an item, in display order: by stage rank, then most recent activity,
    /// then name, so the order is total and stable.
    pub fn items(&self) -> Vec<Item<'_>> {
        let mut items: Vec<Item<'_>> = self.tasks.iter().map(|task| Item { task }).collect();
        items.sort_by(|a, b| {
            a.rank()
                .cmp(&b.rank())
                .then(b.task.last_active.cmp(&a.task.last_active))
                .then(a.task_name().cmp(b.task_name()))
        });
        items
    }
}

/// Whether an item is currently rendered — and therefore whether its `gh`/git calls are
/// worth making. Shared by the list rebuild and the async dispatchers so fetches track
/// exactly what the user can see.
///
/// An orphan is never hidden by a content toggle — it is the one item the app is asking the
/// user to resolve.
///
/// Backlog mode shows only backlogged items that are not done; outside it a backlogged item is
/// hidden while it is open, and a done one follows `show_done` like any other.
pub fn item_visible(item: &Item<'_>, ui: &UiState) -> bool {
    if let Some(sel) = &ui.selected_project
        && item.project() != sel
    {
        return false;
    }
    if item.stage() == Stage::Done && !ui.show_done {
        return false;
    }
    backlog_visible(item, ui) && !ui.hidden.contains(&item.task.id)
}

/// Whether the backlog mode lets an item through, ignoring every other filter — the part of
/// [`item_visible`] the sidebar counts share.
pub fn backlog_visible(item: &Item<'_>, ui: &UiState) -> bool {
    let stage = item.stage();
    if stage == Stage::Orphan {
        return true;
    }
    let backlogged = item.backlogged();
    if ui.backlog_mode {
        backlogged && stage != Stage::Done
    } else {
        !backlogged || stage == Stage::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            backlog: None,
            title: None,
            has_branch: true,
            prs: Vec::new(),
            subtasks: Vec::new(),
            last_active: None,
            coordinator: None,
        }
    }

    fn model(tasks: Vec<Task>) -> Model {
        Model {
            tasks,
            projects: Vec::new(),
        }
    }

    // Pins the stage of each task state, including the split this list exists to make
    // between a task never started and one whose worktree was removed.
    #[test]
    fn stages_follow_the_lifecycle() {
        let mut removed = task("p", "t", "gone");
        removed.present = false;
        let mut unstarted = task("p", "t", "fresh");
        unstarted.present = false;
        unstarted.has_branch = false;
        let mut done = task("p", "t", "finished");
        done.done = true;
        let mut done_and_gone = task("p", "t", "finished-and-gone");
        done_and_gone.done = true;
        done_and_gone.present = false;
        let mut coordinator = task("p", "coordinator", "list");
        coordinator.present = false;
        coordinator.has_branch = false;

        let m = model(vec![
            task("p", "t", "live"),
            removed,
            unstarted,
            done,
            done_and_gone,
            coordinator,
        ]);
        let items = m.items();
        let stage = |n: &str| items.iter().find(|i| i.task_name() == n).unwrap().stage();

        assert_eq!(stage("live"), Stage::Active);
        assert_eq!(stage("gone"), Stage::Removed);
        assert_eq!(stage("fresh"), Stage::NotStarted);
        assert_eq!(stage("finished"), Stage::Done);
        assert_eq!(stage("finished-and-gone"), Stage::Done, "done wins");
        assert_eq!(
            stage("list"),
            Stage::Coordinator,
            "a coordinator never starts"
        );
    }

    // Pins that the headline is the issue.md title when the task has one, the name otherwise.
    #[test]
    fn the_title_prefers_the_issue_file() {
        let mut titled = task("p", "t", "gh-27-seasons");
        titled.title = Some("Seasons".into());
        let m = model(vec![titled, task("p", "t", "unify_uis")]);
        let title = |n: &str| {
            m.items()
                .iter()
                .find(|i| i.task_name() == n)
                .unwrap()
                .title()
        };
        assert_eq!(title("gh-27-seasons"), "Seasons");
        assert_eq!(title("unify_uis"), "unify_uis");
    }

    // Pins the display order: what wants a click first, then live work, then what has to be
    // started or restored, done last; ties broken by recent activity, then name.
    #[test]
    fn sort_order() {
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
        let mut unstarted = task("p", "t", "z-unstarted");
        unstarted.present = false;
        unstarted.has_branch = false;
        let mut removed = task("p", "t", "z-removed");
        removed.present = false;
        let mut done = task("p", "t", "z-done");
        done.done = true;
        let mut orphan = task("p", "t", "z-orphan");
        orphan.adopted = false;

        let m = model(vec![
            ready, in_review, active, blocked, unstarted, removed, done, orphan,
        ]);
        let order: Vec<&str> = m.items().iter().map(|i| i.task_name()).collect();
        assert_eq!(
            order,
            vec![
                "z-orphan",
                "z-ready",
                "z-in-review",
                "z-active",
                "z-blocked",
                "z-unstarted",
                "z-removed",
                "z-done",
            ]
        );

        let mut old = task("p", "t", "a-old");
        old.last_active = Some(day(100));
        let mut fresh = task("p", "t", "z-fresh");
        fresh.last_active = Some(day(200));
        let m = model(vec![old, fresh]);
        let order: Vec<&str> = m.items().iter().map(|i| i.task_name()).collect();
        assert_eq!(
            order,
            vec!["z-fresh", "a-old"],
            "recent first, despite the name"
        );
    }

    // Pins the filters: project selection, the done toggle, the session hide, and that no
    // toggle hides an orphan.
    #[test]
    fn visibility_honours_filters_and_defaults() {
        let mut done = task("p", "t", "finished");
        done.done = true;
        let mut orphan = task("p", "t", "stray");
        orphan.adopted = false;
        let m = model(vec![
            task("p", "t", "live"),
            done,
            task("other", "t", "elsewhere"),
            orphan,
        ]);
        let items = m.items();
        let visible = |ui: &UiState| -> Vec<&str> {
            let mut v: Vec<&str> = items
                .iter()
                .filter(|i| item_visible(i, ui))
                .map(|i| i.task_name())
                .collect();
            v.sort();
            v
        };

        let mut ui = UiState::default();
        assert_eq!(visible(&ui), vec!["elsewhere", "live", "stray"]);

        ui.show_done = true;
        assert!(visible(&ui).contains(&"finished"));
        ui.show_done = false;

        ui.selected_project = Some("p".into());
        assert_eq!(visible(&ui), vec!["live", "stray"]);

        ui.hidden.insert(TaskId {
            project: "p".into(),
            topic: "t".into(),
            name: "live".into(),
        });
        assert_eq!(visible(&ui), vec!["stray"]);
    }

    // Pins the backlog-mode grid: backlogged open work only in backlog mode, done work only
    // outside it, and orphans in both.
    #[test]
    fn backlog_mode_splits_the_list() {
        let mut parked = task("p", "t", "parked");
        parked.backlog = Some(true);
        let mut parked_done = task("p", "t", "parked-done");
        parked_done.backlog = Some(true);
        parked_done.done = true;
        let mut unmarked = task("p", "t", "unmarked");
        unmarked.backlog = Some(false);
        let mut orphan = task("p", "t", "stray");
        orphan.adopted = false;
        orphan.backlog = Some(true);
        let m = model(vec![
            task("p", "t", "live"),
            parked,
            parked_done,
            unmarked,
            orphan,
        ]);
        let items = m.items();
        let visible = |ui: &UiState| -> Vec<&str> {
            let mut v: Vec<&str> = items
                .iter()
                .filter(|i| item_visible(i, ui))
                .map(|i| i.task_name())
                .collect();
            v.sort();
            v
        };

        let mut ui = UiState::default();
        assert_eq!(visible(&ui), vec!["live", "stray", "unmarked"]);

        ui.show_done = true;
        assert!(
            visible(&ui).contains(&"parked-done"),
            "show done reveals a done item whether or not it is backlogged"
        );

        ui.backlog_mode = true;
        assert_eq!(visible(&ui), vec!["parked", "stray"]);
    }
}
