//! The dashboard view: the model, its UI state, the selection and every poller that keeps them
//! current, and the state changes that keys and clicks make. Drawing is `draw`, input is `input`.

mod draw;
mod input;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use hwt_core::{Interval, Reply, Roots, spawn_job};
use log::warn;
use ratatui::layout::Rect;

use crate::facets::Chip;
use crate::focus::{PresenceProbe, Presences};
use crate::hits::Hits;
use crate::model::item::{Item, Model, UiState, in_pr_scope, item_visible, merge, scan};
use crate::model::pull::{PullError, PullMsg, PullOutcome, pull};
use crate::model::task::{
    BlockPrMsg, CachedPr, GitMsg, PrMsg, PrStatus, Task, TaskId, adopt, clear_pr_cache, read_git,
    read_pr_cache, read_pr_status, record_block_pr_state, set_backlog, toggle_done, write_pr_cache,
};
use crate::refresh::{Landed, RefreshPoller};
use crate::session::{SessionReader, SessionState, Sessions};

const GIT_POLL_SECS: u64 = 10;
const BLOCK_POLL_SECS: u64 = 5;
const PR_POLL_MINS: u64 = 5;
const PULL_LINE_LINGER: Duration = Duration::from_secs(10);
const DRAIN_CAP: usize = 256;

/// The buttons a card can carry, in focus order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CardButton {
    Done,
    Backlog,
    Terminal,
    Notes,
    Hide,
    Main,
}

/// Everything in the view that reacts to a click.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Control {
    ProjectRow(Option<String>),
    PullButton(String),
    FilterField,
    ClearFilter,
    Chip(Chip),
    ClearChips,
    ShowDone,
    ShowBacklog,
    Refresh,
    ClearCache,
    CardBody(TaskId),
    Card(TaskId, CardButton),
    PrsToggle(TaskId),
    TodoToggle(TaskId),
    PrLine(TaskId, String),
    Aspect(TaskId, String),
    AspectToggle(TaskId, String),
}

impl Control {
    fn card(&self) -> Option<&TaskId> {
        match self {
            Control::CardBody(id)
            | Control::Card(id, _)
            | Control::PrsToggle(id)
            | Control::TodoToggle(id)
            | Control::PrLine(id, _)
            | Control::Aspect(id, _)
            | Control::AspectToggle(id, _) => Some(id),
            _ => None,
        }
    }
}

/// Where one project's pull has got to.
#[derive(Clone, Debug, PartialEq)]
pub enum PullState {
    Running,
    Done(PullOutcome),
    Failed(String),
}

/// One project's pull and, once it succeeded, when its line goes.
#[derive(Clone, Debug)]
pub struct Pull {
    pub state: PullState,
    pub expires: Option<Instant>,
}

type ScopeKey = (Option<String>, bool, bool, BTreeSet<TaskId>);

/// The dashboard.
pub struct DashView {
    model: Model,
    ui: UiState,
    sessions: Sessions,
    session_reader: SessionReader,
    presences: Presences,
    probe: PresenceProbe,
    poller: RefreshPoller,
    git_tx: Sender<Option<GitMsg>>,
    git_rx: Receiver<Option<GitMsg>>,
    git_in_flight: HashSet<(TaskId, PathBuf)>,
    git_every: Interval,
    pr_tx: Sender<PrMsg>,
    pr_rx: Receiver<PrMsg>,
    dispatched: HashMap<TaskId, HashSet<String>>,
    pr_every: Interval,
    pr_cache: HashMap<String, CachedPr>,
    pull_tx: Sender<PullMsg>,
    pull_rx: Receiver<PullMsg>,
    pulls: HashMap<String, Pull>,
    block_every: Interval,
    block_pr_every: Interval,
    block_pr_tx: Sender<BlockPrMsg>,
    block_pr_rx: Receiver<BlockPrMsg>,
    lifted: HashSet<TaskId>,
    scope_key: Option<ScopeKey>,
    model_dirty: bool,

    field_focused: bool,
    selected: Option<TaskId>,
    sel_pos: usize,
    focus: Option<Control>,
    reset_selection: bool,
    scroll_list: u16,
    scroll_sidebar: u16,
    scroll_to_selected: bool,
    hovered: Option<Control>,
    pressed: Option<Option<Control>>,
    hits: Hits<Control>,
    list_area: Rect,
    sidebar_area: Rect,
    cards_per_screen: usize,
    drawn: bool,
}

impl DashView {
    /// Scan the model, load the PR cache, read the session state, and start reading git and PR
    /// state in the background.
    pub fn new() -> Self {
        let mut view = Self::with_model(scan());
        view.pr_cache = read_pr_cache();
        let index = view.session_index();
        view.session_reader.refresh(&mut view.sessions, &index);
        let repos = present_repos(&view.model, |_| true);
        view.kick_git(repos);
        view.dispatch_prs();
        view.scope_key = Some(view.scope());
        view
    }

    /// A view over `model` that has started nothing in the background.
    pub fn with_model(model: Model) -> Self {
        let (git_tx, git_rx) = channel();
        let (pr_tx, pr_rx) = channel();
        let (block_pr_tx, block_pr_rx) = channel();
        let (pull_tx, pull_rx) = channel();
        Self {
            model,
            ui: UiState::default(),
            sessions: Sessions::default(),
            session_reader: SessionReader::default(),
            presences: Presences::default(),
            probe: PresenceProbe::default(),
            poller: RefreshPoller::default(),
            git_tx,
            git_rx,
            git_in_flight: HashSet::new(),
            git_every: Interval::new(Duration::from_secs(GIT_POLL_SECS), false),
            pr_tx,
            pr_rx,
            dispatched: HashMap::new(),
            pr_every: Interval::new(Duration::from_secs(PR_POLL_MINS * 60), false),
            pr_cache: HashMap::new(),
            pull_tx,
            pull_rx,
            pulls: HashMap::new(),
            block_every: Interval::new(Duration::from_secs(BLOCK_POLL_SECS), false),
            block_pr_every: Interval::new(Duration::from_secs(PR_POLL_MINS * 60), true),
            block_pr_tx,
            block_pr_rx,
            lifted: HashSet::new(),
            scope_key: None,
            model_dirty: false,
            field_focused: false,
            selected: None,
            sel_pos: 0,
            focus: None,
            reset_selection: false,
            scroll_list: 0,
            scroll_sidebar: 0,
            scroll_to_selected: false,
            hovered: None,
            pressed: None,
            hits: Hits::default(),
            list_area: Rect::default(),
            sidebar_area: Rect::default(),
            cards_per_screen: 1,
            drawn: false,
        }
    }

    /// The filter field's text.
    #[cfg(test)]
    pub fn query(&self) -> &str {
        &self.ui.query
    }

    /// The selected card and the control holding the keyboard focus.
    #[cfg(test)]
    pub fn selection(&self) -> (Option<TaskId>, Option<Control>) {
        (self.selected.clone(), self.focus.clone())
    }

    /// Whether keys go to the filter field rather than to the shell's bindings.
    pub fn captures_keys(&self) -> bool {
        self.field_focused
    }

    /// Fold finished background work into the model and start what is due. Answers whether
    /// anything shown changed.
    pub fn tick(&mut self, delta: Duration) -> bool {
        let mut changed = std::mem::take(&mut self.model_dirty);
        changed |= self.drain_git();
        changed |= self.drain_pr();
        changed |= self.drain_block_prs();
        changed |= self.drain_pulls();
        changed |= self.expire_pull_lines();
        changed |= self.poll_background(delta);
        let scope = self.scope();
        if changed || self.scope_key.as_ref() != Some(&scope) {
            self.dispatch_prs();
            self.scope_key = Some(scope);
        }
        changed
    }

    fn poll_background(&mut self, delta: Duration) -> bool {
        let mut changed = false;
        match self.poller.tick(delta) {
            Some(Landed::Merge(fresh)) => {
                if merge(&mut self.model, fresh) {
                    changed = true;
                    let repos = present_repos(&self.model, |repo| !repo.git.loaded);
                    self.kick_git(repos);
                    self.forget_vanished_dispatches();
                    self.fall_back_to_all();
                }
            }
            Some(Landed::Replace(fresh)) => {
                self.model = fresh;
                let repos = present_repos(&self.model, |_| true);
                self.kick_git(repos);
                self.dispatched.clear();
                self.fall_back_to_all();
                changed = true;
            }
            None => {}
        }
        if self.git_every.tick(delta) && self.git_in_flight.is_empty() {
            let open: HashSet<TaskId> = self
                .model
                .tasks
                .iter()
                .filter(|t| !t.done)
                .map(|t| t.id.clone())
                .collect();
            let repos = present_repos(&self.model, |_| true)
                .into_iter()
                .filter(|(id, _)| open.contains(id))
                .collect();
            self.kick_git(repos);
        }
        if self.pr_every.tick(delta) {
            self.forget_changeable_prs();
            changed = true;
        }
        if self.block_pr_every.tick(delta) {
            self.dispatch_block_prs();
        }
        if self.block_every.tick(delta) {
            let now: HashSet<TaskId> = self
                .model
                .tasks
                .iter()
                .filter(|t| t.ready())
                .map(|t| t.id.clone())
                .collect();
            if now != self.lifted {
                self.lifted = now;
                changed = true;
            }
        }
        let index = self.session_index();
        changed |= self.session_reader.tick(delta, &mut self.sessions, &index);
        changed |= self.probe.tick(delta, &mut self.presences);
        changed
    }

    fn session_index(&self) -> HashMap<String, TaskId> {
        self.model
            .tasks
            .iter()
            .filter(|t| !t.session_name.is_empty())
            .map(|t| (t.session_name.clone(), t.id.clone()))
            .collect()
    }

    fn scope(&self) -> ScopeKey {
        (
            self.ui.selected_project.clone(),
            self.ui.show_done,
            self.ui.backlog_mode,
            self.ui.hidden.iter().cloned().collect(),
        )
    }

    fn kick_git(&mut self, repos: Vec<(TaskId, PathBuf)>) {
        for (id, path) in repos {
            if !self.git_in_flight.insert((id.clone(), path.clone())) {
                continue;
            }
            let reply = Reply::new(self.git_tx.clone(), None);
            spawn_job(move || {
                let info = read_git(&path);
                reply.send(Some(GitMsg { id, path, info }));
            });
        }
    }

    fn drain_git(&mut self) -> bool {
        let msgs: Vec<Option<GitMsg>> = self.git_rx.try_iter().take(DRAIN_CAP).collect();
        if msgs.iter().any(Option::is_none) {
            self.git_in_flight.clear();
        }
        let mut changed = false;
        for msg in msgs.into_iter().flatten() {
            self.git_in_flight
                .remove(&(msg.id.clone(), msg.path.clone()));
            if let Some(task) = self.model.task_mut(&msg.id)
                && let Some(repo) = task.repos.iter_mut().find(|r| r.path == msg.path)
                && repo.git != msg.info
            {
                repo.git = msg.info;
                changed = true;
            }
        }
        changed
    }

    fn dispatch_block_prs(&self) {
        for task in self.model.tasks.iter().filter(|t| !t.done) {
            let Some(url) = task
                .block
                .as_ref()
                .and_then(|b| b.pr_to_check())
                .map(str::to_owned)
            else {
                continue;
            };
            let id = task.id.clone();
            let reply = Reply::new(
                self.block_pr_tx.clone(),
                BlockPrMsg {
                    id: id.clone(),
                    url: url.clone(),
                    status: PrStatus::Unknown,
                },
            );
            spawn_job(move || {
                let (status, _, _) = read_pr_status(&url);
                reply.send(BlockPrMsg { id, url, status });
            });
        }
    }

    fn drain_block_prs(&mut self) -> bool {
        let msgs: Vec<BlockPrMsg> = self.block_pr_rx.try_iter().take(DRAIN_CAP).collect();
        let mut changed = false;
        for msg in msgs {
            match record_block_pr_state(&Roots::of_user(), &msg.id, &msg.url, msg.status) {
                Ok(Some(block)) => {
                    if let Some(task) = self.model.task_mut(&msg.id) {
                        task.block = Some(block);
                        changed = true;
                    }
                }
                Ok(None) => {}
                Err(e) => warn!("recording {}'s block PR state: {e}", msg.id.name),
            }
        }
        changed
    }

    fn dispatch_prs(&mut self) {
        let sessions = self.sessions.states();
        let scoped: Vec<(TaskId, Vec<String>)> = self
            .model
            .items(&sessions)
            .into_iter()
            .filter(|item| in_pr_scope(item, &self.ui))
            .map(|item| {
                (
                    item.task.id.clone(),
                    item.task.all_prs().map(|p| p.url.clone()).collect(),
                )
            })
            .collect();
        for (id, urls) in scoped {
            for url in urls {
                if !self
                    .dispatched
                    .entry(id.clone())
                    .or_default()
                    .insert(url.clone())
                {
                    continue;
                }
                if let Some(hit) = self.pr_cache.get(&url) {
                    let _ = self.pr_tx.send(PrMsg {
                        id: id.clone(),
                        url,
                        status: hit.status,
                        number: hit.number,
                        title: hit.title.clone(),
                    });
                    continue;
                }
                let reply = Reply::new(
                    self.pr_tx.clone(),
                    PrMsg {
                        id: id.clone(),
                        url: url.clone(),
                        status: PrStatus::Unknown,
                        number: None,
                        title: None,
                    },
                );
                let id = id.clone();
                spawn_job(move || {
                    let (status, number, title) = read_pr_status(&url);
                    reply.send(PrMsg {
                        id,
                        url,
                        status,
                        number,
                        title,
                    });
                });
            }
        }
    }

    fn drain_pr(&mut self) -> bool {
        let msgs: Vec<PrMsg> = self.pr_rx.try_iter().take(DRAIN_CAP).collect();
        let mut changed = false;
        let mut cache_dirty = false;
        for msg in msgs {
            let Some(pr) = self
                .model
                .task_mut(&msg.id)
                .and_then(|t| t.all_prs_mut().find(|p| p.url == msg.url))
            else {
                continue;
            };
            if (pr.status, pr.number, &pr.live_title) != (msg.status, msg.number, &msg.title) {
                pr.status = msg.status;
                pr.number = msg.number;
                pr.live_title = msg.title.clone();
                changed = true;
            }
            if msg.status.is_terminal() && !self.pr_cache.contains_key(&msg.url) {
                self.pr_cache.insert(
                    msg.url,
                    CachedPr {
                        status: msg.status,
                        number: msg.number,
                        title: msg.title,
                    },
                );
                cache_dirty = true;
            }
        }
        if cache_dirty {
            write_pr_cache(&self.pr_cache);
        }
        changed
    }

    fn forget_changeable_prs(&mut self) {
        for task in &self.model.tasks {
            let Some(urls) = self.dispatched.get_mut(&task.id) else {
                continue;
            };
            for pr in task.all_prs() {
                if matches!(
                    pr.status,
                    PrStatus::Open | PrStatus::Draft | PrStatus::Unknown
                ) {
                    urls.remove(&pr.url);
                }
            }
        }
    }

    fn forget_vanished_dispatches(&mut self) {
        let model = &self.model;
        self.dispatched.retain(|id, urls| {
            let Some(task) = model.tasks.iter().find(|t| &t.id == id) else {
                return false;
            };
            urls.retain(|url| task.all_prs().any(|p| &p.url == url));
            true
        });
    }

    fn fall_back_to_all(&mut self) {
        if let Some(sel) = &self.ui.selected_project
            && !self.model.tasks.iter().any(|t| &t.id.project == sel)
            && !self.model.projects.iter().any(|p| &p.name == sel)
        {
            self.ui.selected_project = None;
        }
    }

    fn start_pull(&mut self, project: &str) {
        let Some(entry) = self.model.projects.iter().find(|p| p.name == project) else {
            return;
        };
        if matches!(
            self.pulls.get(project).map(|p| &p.state),
            Some(PullState::Running)
        ) {
            return;
        }
        let (name, path) = (entry.name.clone(), entry.path.clone());
        self.pulls.insert(
            name.clone(),
            Pull {
                state: PullState::Running,
                expires: None,
            },
        );
        let reply = Reply::new(
            self.pull_tx.clone(),
            PullMsg {
                project: name.clone(),
                result: Err(PullError::JobStopped),
            },
        );
        spawn_job(move || {
            let result = pull(&path);
            reply.send(PullMsg {
                project: name,
                result,
            });
        });
    }

    fn drain_pulls(&mut self) -> bool {
        let msgs: Vec<PullMsg> = self.pull_rx.try_iter().collect();
        let changed = !msgs.is_empty();
        for msg in msgs {
            let pull = match msg.result {
                Ok(outcome) => Pull {
                    state: PullState::Done(outcome),
                    expires: Some(Instant::now() + PULL_LINE_LINGER),
                },
                Err(reason) => {
                    warn!("git pull failed for {}: {reason}", msg.project);
                    Pull {
                        state: PullState::Failed(reason.to_string()),
                        expires: None,
                    }
                }
            };
            self.pulls.insert(msg.project, pull);
        }
        changed
    }

    fn expire_pull_lines(&mut self) -> bool {
        let now = Instant::now();
        let before = self.pulls.len();
        self.pulls
            .retain(|_, p| !p.expires.is_some_and(|at| now >= at));
        self.pulls.len() != before
    }

    fn refresh(&mut self) {
        self.ui.hidden.clear();
        self.pulls
            .retain(|_, p| matches!(p.state, PullState::Running));
        self.poller.refresh();
    }

    fn clear_cache(&mut self) {
        clear_pr_cache();
        self.pr_cache.clear();
        self.dispatched.clear();
        self.dispatch_prs();
    }

    fn toggle_done(&mut self, id: &TaskId) {
        match toggle_done(&Roots::of_user(), id) {
            Ok(done) => {
                if let Some(task) = self.model.task_mut(id) {
                    task.done = done;
                }
                self.model_dirty = true;
            }
            Err(err) => warn!("toggle done failed for {id:?}: {err}"),
        }
    }

    fn toggle_backlog(&mut self, id: &TaskId) {
        let Some(item) = self.model.item(id) else {
            return;
        };
        if !item.task.adopted {
            return;
        }
        let backlog = !item.backlogged();
        if let Err(err) = set_backlog(&Roots::of_user(), id, backlog) {
            warn!("recording backlog failed for {id:?}: {err}");
            return;
        }
        if let Some(task) = self.model.task_mut(id) {
            task.backlog = Some(backlog);
        }
        self.model_dirty = true;
    }

    fn adopt(&mut self, id: &TaskId) {
        match adopt(&Roots::of_user(), id) {
            Ok(()) => {
                if let Some(task) = self.model.task_mut(id) {
                    task.adopted = true;
                }
                self.model_dirty = true;
            }
            Err(err) => warn!("adopt failed for {id:?}: {err}"),
        }
    }

    fn listed(&self) -> Vec<(Item<'_>, Option<SessionState>)> {
        let sessions = self.sessions.states();
        self.model
            .items(&sessions)
            .into_iter()
            .map(|item| {
                let state = sessions.get(&item.task.id).copied();
                (item, state)
            })
            .filter(|(item, state)| item_visible(item, &self.ui, *state))
            .collect()
    }

    fn listed_ids(&self) -> Vec<TaskId> {
        self.listed()
            .into_iter()
            .map(|(item, _)| item.task.id.clone())
            .collect()
    }

    fn task(&self, id: &TaskId) -> Option<&Task> {
        self.model.tasks.iter().find(|t| &t.id == id)
    }
}

fn present_repos(
    model: &Model,
    keep: impl Fn(&crate::model::task::Repo) -> bool,
) -> Vec<(TaskId, PathBuf)> {
    model
        .tasks
        .iter()
        .filter(|task| task.present)
        .flat_map(|task| {
            task.repos
                .iter()
                .filter(|repo| keep(repo))
                .map(|repo| (task.id.clone(), repo.path.clone()))
                .collect::<Vec<_>>()
        })
        .collect()
}
