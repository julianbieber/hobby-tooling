//! The list the dashboard draws: one **item** per task, its lifecycle stage, the order the
//! items are shown in, and which of them the current filters let through.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use crate::facets::{Filters, facts, passes};
use crate::session::SessionState;

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

    fn rank(&self, session: Option<SessionState>) -> u8 {
        let task = self.task;
        let stage = self.stage();
        if stage != Stage::Done && session == Some(SessionState::Idle) {
            return 0;
        }
        let busy = session == Some(SessionState::Busy);
        match stage {
            Stage::Orphan => 1,
            Stage::Done => 9,
            _ if busy && !task.ready() && task.awaiting_review() == 0 => 4,
            Stage::NotStarted => 7,
            Stage::Removed | Stage::Coordinator => 8,
            _ if task.ready() => 2,
            _ if task.blocked() => 6,
            _ if task.awaiting_review() > 0 => 3,
            _ => 5,
        }
    }
}

/// UI selection / filter state. The default is "All", open work, nothing hidden, no chip on and
/// an empty project filter.
#[derive(Default)]
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
    /// The sidebar's project filter text (in-memory).
    pub query: String,
    /// The filter chips that are on (in-memory).
    pub chips: Filters,
}

/// The scanned model.
#[derive(Default)]
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

    /// Every task as an item, in display order: by rank, then most recent activity, then name, so
    /// the order is total and stable.
    ///
    /// Rank, first match wins: a task that is not done whose session is idle; an orphan; a lifted
    /// block; a PR awaiting review; a busy session; active; blocked; not started; removed or a
    /// coordinator; done. A coordinator's session counts like any other task's.
    pub fn items(&self, sessions: &HashMap<TaskId, SessionState>) -> Vec<Item<'_>> {
        let mut items: Vec<(u8, Item<'_>)> = self
            .tasks
            .iter()
            .map(|task| {
                let item = Item { task };
                (item.rank(sessions.get(&task.id).copied()), item)
            })
            .collect();
        items.sort_by(|(ra, a), (rb, b)| {
            ra.cmp(rb)
                .then(b.task.last_active.cmp(&a.task.last_active))
                .then(a.task_name().cmp(b.task_name()))
        });
        items.into_iter().map(|(_, item)| item).collect()
    }
}

/// Fold a fresh scan into `model`: its tasks and projects replace the old ones, while each repo
/// keeps the git status already loaded for its path and each PR the live status, number and title
/// already loaded for its URL. Answers whether anything that is drawn differs from before.
pub fn merge(model: &mut Model, mut fresh: Model) -> bool {
    let mut git = HashMap::new();
    let mut prs = HashMap::new();
    for task in &model.tasks {
        for repo in &task.repos {
            git.insert(repo.path.clone(), repo.git.clone());
        }
        for pr in &task.prs {
            prs.insert(
                pr.url.clone(),
                (pr.status, pr.number, pr.live_title.clone()),
            );
        }
    }
    for task in &mut fresh.tasks {
        for repo in &mut task.repos {
            if let Some(info) = git.get(&repo.path) {
                repo.git = info.clone();
            }
        }
        for pr in &mut task.prs {
            if let Some((status, number, title)) = prs.get(&pr.url) {
                pr.status = *status;
                pr.number = *number;
                pr.live_title = title.clone();
            }
        }
    }
    let changed = model.tasks != fresh.tasks || model.projects != fresh.projects;
    *model = fresh;
    changed
}

/// The projects matching `query`, best match first; every project in name order when the query is
/// empty. A project is scored on its own name and on the name and title of every task in it,
/// keeping the best. Smart-case: an uppercase letter in the query makes case count.
pub fn rank_projects<'a>(projects: &'a [Project], tasks: &[Task], query: &str) -> Vec<&'a Project> {
    hwt_core::rank_by(projects, query, |project| {
        std::iter::once(project.name.clone())
            .chain(
                tasks
                    .iter()
                    .filter(|t| t.id.project == project.name)
                    .flat_map(|t| std::iter::once(t.id.name.clone()).chain(t.title.clone())),
            )
            .collect::<Vec<_>>()
    })
}

/// Whether an item is listed: the project selection, the done toggle, the chips that are on, the
/// backlog mode and the session hides. `session` is the task's session state, for the Session
/// chips.
///
/// An orphan is never hidden by a toggle — it is the one item the app is asking the user to
/// resolve — though a chip that is on can narrow it away like anything else.
///
/// Backlog mode shows only backlogged items that are not done; outside it a backlogged item is
/// hidden while it is open, and a done one follows `show_done` like any other.
pub fn item_visible(item: &Item<'_>, ui: &UiState, session: Option<SessionState>) -> bool {
    in_pr_scope(item, ui) && chips_pass(item, ui, session)
}

/// [`item_visible`] without the chips: the items whose PR status is worth fetching, since a chip
/// can be turned on or off without waiting for a query.
pub fn in_pr_scope(item: &Item<'_>, ui: &UiState) -> bool {
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

/// Whether an item passes the chips that are on, ignoring every other filter.
pub fn chips_pass(item: &Item<'_>, ui: &UiState, session: Option<SessionState>) -> bool {
    ui.chips.is_empty() || passes(&facts(item, session, SystemTime::now()), &ui.chips)
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
            phase: None,
            session_name: String::new(),
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
        let items = m.items(&HashMap::new());
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
            m.items(&HashMap::new())
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
        let order: Vec<&str> = m
            .items(&HashMap::new())
            .iter()
            .map(|i| i.task_name())
            .collect();
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
        let order: Vec<&str> = m
            .items(&HashMap::new())
            .iter()
            .map(|i| i.task_name())
            .collect();
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
        let items = m.items(&HashMap::new());
        let visible = |ui: &UiState| -> Vec<&str> {
            let mut v: Vec<&str> = items
                .iter()
                .filter(|i| item_visible(i, ui, None))
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
        let items = m.items(&HashMap::new());
        let visible = |ui: &UiState| -> Vec<&str> {
            let mut v: Vec<&str> = items
                .iter()
                .filter(|i| item_visible(i, ui, None))
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

    // Pins where a session moves a card: an idle one ahead of everything that is not done, even
    // an orphan; a busy one between a PR awaiting review and plain active work; done stays last.
    #[test]
    fn sessions_move_cards_by_attention() {
        let mut orphan = task("p", "t", "a-orphan");
        orphan.adopted = false;
        let mut review = task("p", "t", "b-review");
        review.prs = vec![Pr {
            url: "https://gh/pr/2".into(),
            repo: None,
            title: None,
            opened: None,
            status: PrStatus::Open,
            number: Some(2),
            live_title: None,
        }];
        let mut done_idle = task("p", "t", "c-done-idle");
        done_idle.done = true;
        let m = model(vec![
            orphan,
            review,
            task("p", "t", "d-active"),
            task("p", "t", "e-busy"),
            task("p", "t", "f-idle"),
            done_idle,
        ]);
        let id = |n: &str| TaskId {
            project: "p".into(),
            topic: "t".into(),
            name: n.into(),
        };
        let sessions = HashMap::from([
            (id("e-busy"), SessionState::Busy),
            (id("f-idle"), SessionState::Idle),
            (id("c-done-idle"), SessionState::Idle),
        ]);
        let order: Vec<&str> = m.items(&sessions).iter().map(|i| i.task_name()).collect();
        assert_eq!(
            order,
            vec![
                "f-idle",
                "a-orphan",
                "b-review",
                "e-busy",
                "d-active",
                "c-done-idle"
            ]
        );
    }

    // Pins that a rescan keeps what was already loaded — git by repo path, PR state by URL — and
    // reports a change only when the drawn model differs.
    #[test]
    fn merge_keeps_loaded_state_by_path_and_url() {
        let mut old = task("p", "t", "live");
        old.repos[0].git = GitInfo {
            loaded: true,
            branch: Some("live".into()),
            head: None,
            dirty: 2,
        };
        old.prs = vec![Pr {
            url: "https://gh/pr/1".into(),
            repo: None,
            title: None,
            opened: None,
            status: PrStatus::Merged,
            number: Some(1),
            live_title: Some("Done".into()),
        }];
        let mut m = model(vec![old.clone()]);

        let mut fresh = old.clone();
        fresh.repos[0].git = GitInfo::default();
        fresh.prs[0].status = PrStatus::Loading;
        fresh.prs[0].live_title = None;
        assert!(
            !merge(&mut m, model(vec![fresh.clone()])),
            "nothing drawn changed"
        );
        assert_eq!(m.tasks[0].repos[0].git.dirty, 2);
        assert_eq!(m.tasks[0].prs[0].status, PrStatus::Merged);

        fresh.title = Some("Renamed".into());
        assert!(merge(&mut m, model(vec![fresh])), "a new title is a change");
        assert_eq!(m.tasks[0].prs[0].live_title.as_deref(), Some("Done"));
    }

    // Pins the sidebar filter: a project surfaces by its own name or by a task name or title in
    // it, and an empty query keeps every project.
    #[test]
    fn projects_rank_by_name_task_and_title() {
        let project = |name: &str| Project {
            name: name.into(),
            path: PathBuf::from("/nowhere").join(name),
            has_checkout: true,
        };
        let projects = vec![project("bevy_survivor"), project("wusel")];
        let mut titled = task("wusel", "perf", "gh-7-chunks");
        titled.title = Some("Chunk streaming".into());
        let tasks = vec![titled, task("bevy_survivor", "features", "enemy-waves")];
        let names = |q: &str| -> Vec<String> {
            rank_projects(&projects, &tasks, q)
                .iter()
                .map(|p| p.name.clone())
                .collect()
        };
        assert_eq!(names(""), vec!["bevy_survivor", "wusel"]);
        assert_eq!(names("wsl"), vec!["wusel"]);
        assert_eq!(names("streaming"), vec!["wusel"]);
        assert_eq!(names("waves"), vec!["bevy_survivor"]);
        assert!(names("zzzz").is_empty());
    }
}
