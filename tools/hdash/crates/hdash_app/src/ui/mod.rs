//! UI layer: static chrome built once, systems that rebuild the sidebar and item list when
//! the model or selection changes, the async fetches and pulls, and a global observer
//! that turns button clicks into actions.

mod card;
mod chrome;
mod fetch;

use std::collections::HashMap;

use bevy::prelude::*;

use feathers_kit::{Activate, KitFonts, kit_tokens as kt, text, tok};

use crate::model::item::{Model, UiState, item_visible, scan};
use crate::model::launch::{
    launch_coordinator, launch_hcdev, launch_restore, launch_terminal, open_notes, open_url,
};
use crate::model::pull::{PullMsg, PullOutcome, pull};
use crate::model::task::{
    CachedPr, GitMsg, PrMsg, TaskId, adopt, clear_pr_cache, read_git, read_pr_cache,
    read_pr_status, set_backlog, toggle_done, write_pr_cache,
};

use card::{
    CardAction, CardButton, PrButton, SubtaskButton, install_pr_hover_tokens, spawn_item_card,
};
use chrome::{
    BacklogModeToggle, ClearCacheButton, ItemListContainer, ProjectButton, PullButton,
    RefreshButton, ShowDoneToggle, build_chrome, ft_text_dim, rebuild_sidebar, update_header,
};
use fetch::Fetch;

#[derive(Resource, Default)]
struct GitFetch(Fetch<(TaskId, usize), GitMsg>);

#[derive(Resource, Default)]
struct PrFetch(Fetch<(TaskId, usize), PrMsg>);

#[derive(Resource, Default)]
struct PullFetch(Fetch<String, PullMsg>);

/// How long a *finished* pull's status line stays on screen, in seconds. A failure has no
/// expiry — its reason has to stay readable — and clears on the next pull or Refresh.
const PULL_LINE_LINGER: f32 = 10.0;

/// Where each project's pull has got to, keyed by project name.
///
/// UI state rather than model state: a pull is something the dashboard *does*, whereas
/// [`Model`] is what a scan *found*. Keeping it out of the model also means a Refresh
/// (which rebuilds the model wholesale) cannot lose track of a pull still in flight.
#[derive(Resource, Default)]
pub(crate) struct Pulls {
    map: HashMap<String, Pull>,
}

/// One project's pull.
pub(crate) struct Pull {
    state: PullState,
    expires: Option<f32>,
}

pub(crate) enum PullState {
    /// `git pull` is in flight.
    Running,
    /// It finished and did this.
    Done(PullOutcome),
    /// git refused, with this one-line reason.
    Failed(String),
}

impl Pulls {
    /// Whether a pull is in flight for this project — what makes the sidebar swap its button
    /// for a `…`, and therefore the only reason `chrome` needs to see this resource.
    pub(crate) fn running(&self, project: &str) -> bool {
        matches!(
            self.map.get(project).map(|p| &p.state),
            Some(PullState::Running)
        )
    }
}

#[derive(Resource, Default)]
struct PrCache {
    map: HashMap<String, CachedPr>,
}

/// Wires up the dashboard UI.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Model>()
            .init_resource::<UiState>()
            .init_resource::<GitFetch>()
            .init_resource::<PrFetch>()
            .init_resource::<PullFetch>()
            .init_resource::<Pulls>()
            // Registered up front, not inserted by `load_pr_cache`: `dispatch_all` takes it
            // as a plain `Res`, which panics on a missing resource, and relying on a
            // command flush landing between two `Startup` systems is a needless hazard.
            .init_resource::<PrCache>()
            .add_systems(PreStartup, install_pr_hover_tokens)
            .add_systems(
                Startup,
                (initial_scan, load_pr_cache, dispatch_all, build_chrome).chain(),
            )
            .add_systems(
                Update,
                (
                    drain_git,
                    drain_pr,
                    drain_pulls,
                    expire_pull_lines,
                    dispatch_all
                        .run_if(resource_changed::<Model>.or_else(resource_changed::<UiState>)),
                    rebuild_sidebar.run_if(
                        resource_changed::<Model>
                            .or_else(resource_changed::<UiState>)
                            .or_else(resource_changed::<Pulls>),
                    ),
                    rebuild_item_list.run_if(
                        resource_changed::<Model>
                            .or_else(resource_changed::<UiState>)
                            .or_else(resource_changed::<Pulls>),
                    ),
                    update_header
                        .run_if(resource_changed::<Model>.or_else(resource_changed::<UiState>)),
                )
                    .chain(),
            )
            .add_observer(on_chrome_activate)
            .add_observer(on_card_activate);
    }
}

fn initial_scan(mut model: ResMut<Model>) {
    *model = scan();
}

fn load_pr_cache(mut cache: ResMut<PrCache>) {
    cache.map = read_pr_cache();
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

fn dispatch_all(
    model: Res<Model>,
    ui: Res<UiState>,
    mut git: ResMut<GitFetch>,
    mut prs: ResMut<PrFetch>,
    cache: Res<PrCache>,
) {
    // Git: every present task, visible or not.
    for task in &model.tasks {
        if !task.present {
            continue; // removed tasks have no checkout to read
        }
        for (i, repo) in task.repos.iter().enumerate() {
            let (id, path) = (task.id.clone(), repo.path.clone());
            git.0.dispatch((task.id.clone(), i), move || GitMsg {
                id,
                repo: i,
                info: read_git(&path),
            });
        }
    }

    dispatch_prs(&model, &ui, &mut prs, &cache);
}

fn dispatch_prs(model: &Model, ui: &UiState, prs: &mut PrFetch, cache: &PrCache) {
    for item in model.items() {
        if !item_visible(&item, ui) {
            continue;
        }
        let task = item.task;
        for (i, pr) in task.prs.iter().enumerate() {
            let key = (task.id.clone(), i);
            if let Some(hit) = cache.map.get(&pr.url) {
                prs.0.deliver(
                    key,
                    PrMsg {
                        id: task.id.clone(),
                        pr: i,
                        status: hit.status,
                        number: hit.number,
                        title: hit.title.clone(),
                    },
                );
                continue;
            }
            let (id, url) = (task.id.clone(), pr.url.clone());
            prs.0.dispatch(key, move || {
                let (status, number, title) = read_pr_status(&url);
                PrMsg {
                    id,
                    pr: i,
                    status,
                    number,
                    title,
                }
            });
        }
    }
}

fn start_pull(project: &str, model: &Model, fetch: &mut PullFetch, pulls: &mut Pulls) {
    let Some(entry) = model.projects.iter().find(|p| p.name == project) else {
        return;
    };
    let (name, path) = (entry.name.clone(), entry.path.clone());
    let for_msg = name.clone();
    let started = fetch.0.dispatch(name.clone(), move || PullMsg {
        project: for_msg,
        result: pull(&path),
    });
    if started {
        pulls.map.insert(
            name,
            Pull {
                state: PullState::Running,
                expires: None,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Drains
// ---------------------------------------------------------------------------

fn drain_git(git: Res<GitFetch>, mut model: ResMut<Model>) {
    let msgs = git.0.drain(256);
    if msgs.is_empty() {
        return;
    }
    for msg in msgs {
        if let Some(task) = model.task_mut(&msg.id)
            && let Some(repo) = task.repos.get_mut(msg.repo)
        {
            repo.git = msg.info;
        }
    }
}

fn drain_pr(prs: Res<PrFetch>, mut model: ResMut<Model>, mut cache: ResMut<PrCache>) {
    let msgs = prs.0.drain(256);
    if msgs.is_empty() {
        return;
    }
    let mut cache_dirty = false;
    for msg in msgs {
        if let Some(task) = model.task_mut(&msg.id)
            && let Some(pr) = task.prs.get_mut(msg.pr)
        {
            pr.status = msg.status;
            pr.number = msg.number;
            pr.live_title = msg.title.clone();
            if msg.status.is_terminal() && !cache.map.contains_key(&pr.url) {
                cache.map.insert(
                    pr.url.clone(),
                    CachedPr {
                        status: msg.status,
                        number: msg.number,
                        title: msg.title,
                    },
                );
                cache_dirty = true;
            }
        }
    }
    if cache_dirty {
        write_pr_cache(&cache.map);
    }
}

fn drain_pulls(mut fetch: ResMut<PullFetch>, mut pulls: ResMut<Pulls>, time: Res<Time>) {
    let msgs = fetch.0.drain(64);
    if msgs.is_empty() {
        return;
    }
    for msg in msgs {
        fetch.0.forget(&msg.project);
        let (state, expires) = match msg.result {
            Ok(outcome) => (
                PullState::Done(outcome),
                Some(time.elapsed_secs() + PULL_LINE_LINGER),
            ),
            Err(reason) => {
                warn!("git pull failed for {}: {reason}", msg.project);
                (PullState::Failed(reason), None)
            }
        };
        pulls.map.insert(msg.project, Pull { state, expires });
    }
}

fn expire_pull_lines(time: Res<Time>, mut pulls: ResMut<Pulls>) {
    let now = time.elapsed_secs();
    let expired = |p: &Pull| p.expires.is_some_and(|at| now >= at);
    if !pulls.map.values().any(expired) {
        return;
    }
    pulls.map.retain(|_, p| !expired(p));
}

// ---------------------------------------------------------------------------
// Item list
// ---------------------------------------------------------------------------

fn pull_strip(model: &Model, pulls: &Pulls) -> Vec<(String, &'static str)> {
    model
        .projects
        .iter()
        .filter_map(|entry| {
            let name = &entry.name;
            Some(match &pulls.map.get(name)?.state {
                PullState::Running => (format!("{name} · pulling…"), ft_text_dim()),
                PullState::Done(outcome) => {
                    (format!("{name} · {}", outcome.summary()), kt::DOT_CLEAN)
                }
                PullState::Failed(reason) => {
                    (format!("{name} · pull failed: {reason}"), kt::STATUS_CLOSED)
                }
            })
        })
        .collect()
}

fn rebuild_item_list(
    mut commands: Commands,
    container: Query<Entity, With<ItemListContainer>>,
    model: Res<Model>,
    ui: Res<UiState>,
    pulls: Res<Pulls>,
    fonts: Res<KitFonts>,
) {
    let Ok(c) = container.single() else {
        return;
    };
    commands.entity(c).despawn_children();

    for (msg, token) in pull_strip(&model, &pulls) {
        let id = commands.spawn(text(&fonts, msg, 12.0, tok(token))).id();
        commands.entity(c).add_child(id);
    }

    let items = model.items();
    let mut shown = 0;
    for item in items.iter().filter(|i| item_visible(i, &ui)) {
        let expanded = ui.pr_expanded.contains(&item.task.id);
        let card_id = spawn_item_card(&mut commands, &fonts, item, expanded);
        commands.entity(c).add_child(card_id);
        shown += 1;
    }

    if shown == 0 {
        let msg = if model.tasks.is_empty() {
            "No tasks in the notes repo."
        } else {
            "Nothing here. Toggle \"show done\"/\"show backlog\", or pick another project."
        };
        commands
            .entity(c)
            .with_child(text(&fonts, msg, 14.0, tok(ft_text_dim())));
    }
}

// ---------------------------------------------------------------------------
// Click handling
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn on_chrome_activate(
    ev: On<Activate>,
    projects: Query<&ProjectButton>,
    pull_buttons: Query<&PullButton>,
    show_done: Query<(), With<ShowDoneToggle>>,
    backlog_mode: Query<(), With<BacklogModeToggle>>,
    refreshes: Query<(), With<RefreshButton>>,
    clears: Query<(), With<ClearCacheButton>>,
    mut ui: ResMut<UiState>,
    mut model: ResMut<Model>,
    mut git: ResMut<GitFetch>,
    mut prs: ResMut<PrFetch>,
    mut pull_fetch: ResMut<PullFetch>,
    mut pulls: ResMut<Pulls>,
    mut cache: ResMut<PrCache>,
) {
    let e = ev.entity;

    // Checked before the row it sits inside, though the two can never both match: a click on
    // the button stops bubbling there, so the row's `ProjectButton` never sees it.
    if let Ok(pull_btn) = pull_buttons.get(e) {
        start_pull(&pull_btn.0, &model, &mut pull_fetch, &mut pulls);
        return;
    }
    if let Ok(pb) = projects.get(e) {
        ui.selected_project = pb.0.clone();
        return;
    }
    if show_done.contains(e) {
        ui.show_done = !ui.show_done;
        return;
    }
    if backlog_mode.contains(e) {
        ui.backlog_mode = !ui.backlog_mode;
        return;
    }
    if refreshes.contains(e) {
        // Re-scan and clear every dispatch guard so `dispatch_all` re-issues the
        // work — it runs because both resources just changed. The terminal-PR-status cache is
        // deliberately kept: a refresh only re-queries the PRs that can still change. Use
        // Clear cache to force merged/closed PRs to be re-checked too.
        ui.hidden.clear();
        *model = scan();
        git.0.reset();
        prs.0.reset();
        // Finished pull lines are session chatter and go with the hides. A pull still in
        // flight keeps both its line and its guard — its result is still coming.
        pulls
            .map
            .retain(|_, p| matches!(p.state, PullState::Running));
        return;
    }
    if clears.contains(e) {
        clear_pr_cache();
        cache.map.clear();
        prs.0.reset();
        dispatch_prs(&model, &ui, &mut prs, &cache);
    }
}

fn on_card_activate(
    ev: On<Activate>,
    cards: Query<&CardButton>,
    pr_buttons: Query<&PrButton>,
    subtask_buttons: Query<&SubtaskButton>,
    mut ui: ResMut<UiState>,
    mut model: ResMut<Model>,
) {
    let e = ev.entity;

    if let Ok(pr) = pr_buttons.get(e) {
        open_url(&pr.url);
        return;
    }
    if let Ok(sub) = subtask_buttons.get(e) {
        ui.selected_project = Some(sub.id.project.clone());
        return;
    }
    let Ok(btn) = cards.get(e) else {
        return;
    };

    match btn.action {
        CardAction::Hide => {
            ui.hidden.insert(btn.key.clone());
            return;
        }
        CardAction::TogglePrs => {
            let id = &btn.key;
            if !ui.pr_expanded.remove(id) {
                ui.pr_expanded.insert(id.clone());
            }
            return;
        }
        CardAction::ToggleDone => {
            let id = &btn.key;
            match toggle_done(id) {
                Ok(done) => {
                    if let Some(task) = model.task_mut(id) {
                        task.done = done;
                    }
                }
                Err(err) => warn!("toggle done failed for {id:?}: {err}"),
            }
            return;
        }
        CardAction::ToggleBacklog => {
            let Some(item) = model.item(&btn.key) else {
                return;
            };
            if !item.task.adopted {
                return;
            }
            let backlog = !item.backlogged();
            let id = item.task.id.clone();
            if let Err(err) = set_backlog(&id, backlog) {
                warn!("recording backlog failed for {id:?}: {err}");
                return;
            }
            if let Some(task) = model.task_mut(&id) {
                task.backlog = Some(backlog);
            }
            return;
        }
        CardAction::Adopt => {
            let id = &btn.key;
            match adopt(id) {
                // The file is now on disk, so flipping the flag in place is the whole
                // update: everything else about the task was already read from the worktree.
                Ok(()) => {
                    if let Some(task) = model.task_mut(id) {
                        task.adopted = true;
                    }
                }
                Err(err) => warn!("adopt failed for {id:?}: {err}"),
            }
            return;
        }
        _ => {}
    }

    // Everything else reads the current model and shells out.
    let Some(item) = model.item(&btn.key) else {
        return;
    };
    let (task, spec) = (item.task, item.launch_spec());
    match btn.action {
        CardAction::Hcdev => launch_hcdev(&task.dir, &spec),
        CardAction::Restore => launch_restore(&spec),
        CardAction::Coordinator => launch_coordinator(&spec),
        CardAction::Terminal => launch_terminal(&task.dir, &format!("term-{}", task.id.name)),
        CardAction::Notes => open_notes(task),
        CardAction::Hide
        | CardAction::TogglePrs
        | CardAction::ToggleDone
        | CardAction::ToggleBacklog
        | CardAction::Adopt => {
            unreachable!()
        }
    }
}
