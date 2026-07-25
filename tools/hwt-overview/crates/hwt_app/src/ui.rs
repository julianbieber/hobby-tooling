//! UI layer: static chrome built once, plus systems that rebuild the sidebar and task list when
//! the model or selection changes, and a global observer that turns button clicks into actions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_feathers::theme::ThemeBackgroundColor;

use feathers_kit::{
    Activate, Button, DotState, KitFonts, KitHoverable, KitSelected, Theme, Variant, badge, card,
    divider, feathers_tokens as ft, icon_button, kit_tokens as kt, row, scroll_list, sidebar_item,
    spacer_grow, status_dot, text, text_bold, text_mono, tok,
};

use crate::worktree::{
    CachedPr, GitMsg, Pr, PrMsg, PrStatus, Repo, SubtaskRef, Task, TaskId, UiState, Worktrees,
    clear_pr_cache, launch_cdev, launch_restore, launch_terminal, notes_repo_email, open_notes,
    open_url, read_git, read_pr_cache, read_pr_status, rel_time, scan, toggle_done, write_pr_cache,
};

/// Holds the receiving end of the async git-status scan.
#[derive(Resource)]
struct GitChannel {
    rx: Mutex<Receiver<GitMsg>>,
}

/// Holds both ends of the async PR-status query. `tx` is retained (not just `rx`) so PR fetches can
/// be dispatched incrementally as tasks become visible, all feeding the same receiver. `dispatched`
/// records which PRs have already been sent to `gh` so a filter/topic change never re-queries one.
#[derive(Resource)]
struct PrChannel {
    tx: Sender<PrMsg>,
    rx: Mutex<Receiver<PrMsg>>,
    dispatched: HashSet<(TaskId, usize)>,
}

/// In-memory mirror of the on-disk terminal-PR-status cache (`~/.cache/wt-overview/pr-status.json`),
/// keyed by PR URL. Merged/closed statuses land here so they're served without calling `gh` on the
/// next launch. Refresh keeps it (only re-querying PRs that can still change); the Clear cache
/// button wipes it (memory + disk) to force merged/closed PRs to be re-checked too.
#[derive(Resource, Default)]
struct PrCache {
    map: HashMap<String, CachedPr>,
}

/// Sidebar width in logical pixels.
const SIDEBAR_W: f32 = 250.0;

// ---------------------------------------------------------------------------
// Markers
// ---------------------------------------------------------------------------

#[derive(Component)]
struct SidebarContainer;

#[derive(Component)]
struct TaskListContainer;

/// A sidebar project button. `None` = the "All" entry. (Named `ProjectButton` in the
/// work dashboard, where the sidebar filtered by topic.)
#[derive(Component, Clone)]
struct ProjectButton(Option<String>);

#[derive(Component)]
struct ShowDoneToggle;

#[derive(Component)]
struct RefreshButton;

/// Header button that wipes the terminal-PR-status cache (memory + disk) and re-queries the visible
/// tasks, so even merged/closed PRs are checked against `gh` again.
#[derive(Component)]
struct ClearCacheButton;

/// The dim count summary text in the header (e.g. "12 active · 116 done").
#[derive(Component)]
struct HeaderSummary;

#[derive(Clone, Copy)]
enum CardAction {
    ToggleDone,
    Cdev,
    Restore,
    Terminal,
    Notes,
    Hide,
    TogglePrs,
}

#[derive(Component, Clone)]
struct TaskActionButton {
    action: CardAction,
    id: TaskId,
}

/// A clickable PR button carrying the PR's GitHub URL; clicking opens it in the browser.
#[derive(Component, Clone)]
struct PrButton {
    url: String,
}

/// A clickable subtask line carrying the child task's id; clicking filters the dashboard to the
/// child's topic so its card comes into view.
#[derive(Component, Clone)]
struct SubtaskButton {
    id: TaskId,
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Wires up the dashboard UI.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Worktrees>()
            .init_resource::<UiState>()
            .add_systems(PreStartup, install_pr_hover_tokens)
            .add_systems(
                Startup,
                (
                    initial_scan,
                    init_identity,
                    kick_git_startup,
                    kick_pr_startup,
                    build_chrome,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    drain_git,
                    drain_pr,
                    kick_pr_on_change
                        .run_if(resource_changed::<Worktrees>.or_else(resource_changed::<UiState>)),
                    rebuild_sidebar
                        .run_if(resource_changed::<Worktrees>.or_else(resource_changed::<UiState>)),
                    rebuild_task_list
                        .run_if(resource_changed::<Worktrees>.or_else(resource_changed::<UiState>)),
                    update_header
                        .run_if(resource_changed::<Worktrees>.or_else(resource_changed::<UiState>)),
                )
                    .chain(),
            )
            .add_observer(on_activate);
    }
}

fn initial_scan(mut wt: ResMut<Worktrees>) {
    *wt = scan();
}

/// Record the notes repo's git identity for the header line.
///
/// The work dashboard also seeded an owner *filter* here, because its notes repo is
/// shared and a scan turns up teammates' tasks. The hobby notes repo has one author,
/// so there is nothing to filter and no owner bar in this build.
fn init_identity(mut ui: ResMut<UiState>) {
    ui.me = notes_repo_email();
}

fn kick_git_startup(wt: Res<Worktrees>, mut commands: Commands) {
    kick_git(&wt, &mut commands);
}

fn kick_pr_startup(wt: Res<Worktrees>, ui: Res<UiState>, mut commands: Commands) {
    let (tx, rx) = std::sync::mpsc::channel::<PrMsg>();
    let mut channel = PrChannel {
        tx,
        rx: Mutex::new(rx),
        dispatched: HashSet::new(),
    };
    let cache = read_pr_cache();
    dispatch_visible_prs(&wt, &ui, &mut channel, &cache);
    commands.insert_resource(channel);
    commands.insert_resource(PrCache { map: cache });
}

/// Fetch PR status for any newly-visible task when the topic/owner/done filters change (or the model
/// is rescanned). The `dispatched` guard keeps this to one query per PR ever, and terminal statuses
/// come straight from the cache, so revealing a task loads its PRs on demand instead of querying
/// every task on disk up front.
fn kick_pr_on_change(
    wt: Res<Worktrees>,
    ui: Res<UiState>,
    channel: Option<ResMut<PrChannel>>,
    cache: Option<Res<PrCache>>,
) {
    let (Some(mut channel), Some(cache)) = (channel, cache) else {
        return;
    };
    dispatch_visible_prs(&wt, &ui, &mut channel, &cache.map);
}

/// Spawn one IoTaskPool task per repo to read git status, streaming results back over a channel.
fn kick_git(wt: &Worktrees, commands: &mut Commands) {
    let (tx, rx) = std::sync::mpsc::channel::<GitMsg>();
    let pool = IoTaskPool::get();
    for task in wt.all_tasks() {
        // Removed tasks have no checkout on disk; their repos are placeholders (no git to read).
        if !task.present {
            continue;
        }
        for (i, repo) in task.repos.iter().enumerate() {
            let tx = tx.clone();
            let id = task.id.clone();
            let path = repo.path.clone();
            pool.spawn(async move {
                let _ = tx.send(GitMsg {
                    id,
                    repo: i,
                    info: read_git(&path),
                });
            })
            .detach();
        }
    }
    commands.insert_resource(GitChannel { rx: Mutex::new(rx) });
}

/// Drain any git results that have arrived and fold them into the model. Touches `Worktrees`
/// mutably only when there is at least one message, so idle frames don't trigger a rebuild.
fn drain_git(channel: Option<Res<GitChannel>>, mut wt: ResMut<Worktrees>) {
    let Some(channel) = channel else {
        return;
    };
    let msgs: Vec<GitMsg> = {
        let Ok(rx) = channel.rx.lock() else {
            return;
        };
        let mut v = Vec::new();
        while let Ok(m) = rx.try_recv() {
            v.push(m);
            if v.len() >= 256 {
                break;
            }
        }
        v
    };
    if msgs.is_empty() {
        return;
    }
    for msg in msgs {
        if let Some(task) = wt.task_mut(&msg.id)
            && let Some(repo) = task.repos.get_mut(msg.repo)
        {
            repo.git = msg.info;
        }
    }
}

/// Resolve the status of every not-yet-queried PR of a currently-visible task, streaming results
/// back over the shared channel. Restricting to [`task_visible`] tasks keeps `gh pr view` calls
/// scoped to the tasks the user can actually see — done, hidden, and filtered-out tasks never hit
/// GitHub. A PR whose terminal status is already cached is served from `cache` with no `gh` call at
/// all; everything else is queried on the IoTaskPool. The `dispatched` set makes each PR resolved at
/// most once per scan.
fn dispatch_visible_prs(
    wt: &Worktrees,
    ui: &UiState,
    channel: &mut PrChannel,
    cache: &HashMap<String, CachedPr>,
) {
    let pool = IoTaskPool::get();
    for task in wt.all_tasks() {
        if !task_visible(task, ui) {
            continue;
        }
        for (i, pr) in task.prs.iter().enumerate() {
            if !channel.dispatched.insert((task.id.clone(), i)) {
                continue;
            }
            // Cached merged/closed PRs can't change — serve them straight from the cache, no `gh`.
            if let Some(hit) = cache.get(&pr.url) {
                let _ = channel.tx.send(PrMsg {
                    id: task.id.clone(),
                    pr: i,
                    status: hit.status,
                    number: hit.number,
                    title: hit.title.clone(),
                });
                continue;
            }
            let tx = channel.tx.clone();
            let id = task.id.clone();
            let url = pr.url.clone();
            pool.spawn(async move {
                let (status, number, title) = read_pr_status(&url);
                let _ = tx.send(PrMsg {
                    id,
                    pr: i,
                    status,
                    number,
                    title,
                });
            })
            .detach();
        }
    }
}

/// Drain any PR-status results that have arrived and fold them into the model. Mirrors
/// [`drain_git`]; touches `Worktrees` mutably only when there is at least one message. Newly-learned
/// terminal statuses (merged/closed) are recorded in the cache and persisted so future launches skip
/// their `gh` query.
fn drain_pr(
    channel: Option<Res<PrChannel>>,
    mut wt: ResMut<Worktrees>,
    mut cache: Option<ResMut<PrCache>>,
) {
    let Some(channel) = channel else {
        return;
    };
    let msgs: Vec<PrMsg> = {
        let Ok(rx) = channel.rx.lock() else {
            return;
        };
        let mut v = Vec::new();
        while let Ok(m) = rx.try_recv() {
            v.push(m);
            if v.len() >= 256 {
                break;
            }
        }
        v
    };
    if msgs.is_empty() {
        return;
    }
    let mut cache_dirty = false;
    for msg in msgs {
        if let Some(task) = wt.task_mut(&msg.id)
            && let Some(pr) = task.prs.get_mut(msg.pr)
        {
            pr.status = msg.status;
            pr.number = msg.number;
            pr.live_title = msg.title.clone();
            if msg.status.is_terminal()
                && let Some(cache) = cache.as_mut()
                && !cache.map.contains_key(&pr.url)
            {
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
    if cache_dirty && let Some(cache) = cache.as_ref() {
        write_pr_cache(&cache.map);
    }
}

// ---------------------------------------------------------------------------
// Static chrome
// ---------------------------------------------------------------------------

fn build_chrome(mut commands: Commands, fonts: Res<KitFonts>) {
    commands.spawn(Camera2d);

    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            ..default()
        },
        ThemeBackgroundColor(ft::WINDOW_BG),
        children![
            // ---- Sidebar ----
            (
                Node {
                    width: Val::Px(SIDEBAR_W),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(feathers_kit::sp::S),
                    padding: UiRect::all(Val::Px(feathers_kit::sp::M)),
                    ..default()
                },
                ThemeBackgroundColor(tok(kt::SIDEBAR_BG)),
                children![
                    text_bold(&fonts, "Projects", 16.0, tok(ft_text_main())),
                    scroll_list(SidebarContainer, ()),
                ],
            ),
            // ---- Main column ----
            (
                Node {
                    flex_grow: 1.0,
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ThemeBackgroundColor(tok(kt::PANEL_BG)),
                children![
                    header_bar(&fonts),
                    divider(),
                    scroll_list(TaskListContainer, ())
                ],
            ),
        ],
    ));
}

/// Token key for main text (the feathers `TEXT_MAIN` token, seeded by the kit).
fn ft_text_main() -> &'static str {
    "feathers.text.main"
}
/// Token key for dim text.
fn ft_text_dim() -> &'static str {
    "feathers.text.dim"
}

fn header_bar(fonts: &KitFonts) -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(feathers_kit::sp::S),
            padding: UiRect::all(Val::Px(feathers_kit::sp::M)),
            ..default()
        },
        children![
            text_bold(fonts, "Tasks", 18.0, tok(ft_text_main())),
            (text(fonts, "", 13.0, tok(ft_text_dim())), HeaderSummary,),
            spacer_grow(),
            (
                Node {
                    width: Val::Px(110.0),
                    ..default()
                },
                children![icon_button(Variant::Normal, ShowDoneToggle, "Toggle done")],
            ),
            (
                Node {
                    width: Val::Px(90.0),
                    ..default()
                },
                children![icon_button(Variant::Normal, RefreshButton, "Refresh")],
            ),
            (
                Node {
                    width: Val::Px(110.0),
                    ..default()
                },
                children![icon_button(
                    Variant::Normal,
                    ClearCacheButton,
                    "Clear cache"
                )],
            ),
        ],
    )
}

// ---------------------------------------------------------------------------
// Sidebar rebuild
// ---------------------------------------------------------------------------

fn rebuild_sidebar(
    mut commands: Commands,
    container: Query<Entity, With<SidebarContainer>>,
    wt: Res<Worktrees>,
    ui: Res<UiState>,
    fonts: Res<KitFonts>,
) {
    let Ok(c) = container.single() else {
        return;
    };
    commands.entity(c).despawn_children();

    // Tasks per project passing the done filter. Topics are groups *within* a
    // project, so the sidebar aggregates them.
    let mut per_project: BTreeMap<String, usize> = BTreeMap::new();
    for topic in &wt.topics {
        let n = topic
            .tasks
            .iter()
            .filter(|t| ui.show_done || !t.done)
            .count();
        *per_project.entry(topic.project.clone()).or_default() += n;
    }

    let total_active: usize = per_project.values().sum();
    spawn_project_row(
        &mut commands,
        c,
        &fonts,
        None,
        "All",
        total_active,
        ui.selected_project.is_none(),
    );

    for (project, count) in &per_project {
        let selected = ui.selected_project.as_deref() == Some(project.as_str());
        spawn_project_row(
            &mut commands,
            c,
            &fonts,
            Some(project.clone()),
            project,
            *count,
            selected,
        );
    }
}

fn spawn_project_row(
    commands: &mut Commands,
    parent: Entity,
    fonts: &KitFonts,
    topic: Option<String>,
    label: &str,
    count: usize,
    selected: bool,
) {
    let label_token = if selected { kt::ACCENT } else { ft_text_main() };
    let id = commands
        .spawn(sidebar_item(
            ProjectButton(topic),
            children![
                text(fonts, label.to_string(), 13.0, tok(label_token)),
                spacer_grow(),
                badge(fonts, count.to_string(), kt::CHIP_BG, tok(ft_text_dim())),
            ],
        ))
        .id();
    if selected {
        commands
            .entity(id)
            .insert((KitSelected, ThemeBackgroundColor(tok(kt::SIDEBAR_ITEM_SEL))));
    }
    commands.entity(parent).add_child(id);
}

// ---------------------------------------------------------------------------
// Visibility
// ---------------------------------------------------------------------------

/// Whether a task is currently rendered in the task list: it belongs to the selected
/// project (or none is selected), is not a hidden-when-`!show_done` done task, and
/// hasn't been session-hidden. Shared by [`rebuild_task_list`] and
/// [`dispatch_visible_prs`] so PR fetches track exactly what the user sees.
fn task_visible(task: &Task, ui: &UiState) -> bool {
    if let Some(sel) = &ui.selected_project
        && &task.id.project != sel
    {
        return false;
    }
    if task.done && !ui.show_done {
        return false;
    }
    if ui.hidden.contains(&task.id) {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Task list rebuild
// ---------------------------------------------------------------------------

fn rebuild_task_list(
    mut commands: Commands,
    container: Query<Entity, With<TaskListContainer>>,
    wt: Res<Worktrees>,
    ui: Res<UiState>,
    fonts: Res<KitFonts>,
) {
    let Ok(c) = container.single() else {
        return;
    };
    commands.entity(c).despawn_children();

    let mut shown = 0;
    for topic in &wt.topics {
        if let Some(sel) = &ui.selected_project
            && &topic.project != sel
        {
            continue;
        }
        for task in &topic.tasks {
            if !task_visible(task, &ui) {
                continue;
            }
            let prs_expanded = ui.pr_expanded.contains(&task.id);
            let card_id = spawn_task_card(&mut commands, &fonts, task, prs_expanded);
            commands.entity(c).add_child(card_id);
            shown += 1;
        }
    }

    if shown == 0 {
        commands.entity(c).with_child(text(
            &fonts,
            "No tasks here. Toggle \"show done\" or pick another project.",
            14.0,
            tok(ft_text_dim()),
        ));
    }
}

fn update_header(
    wt: Res<Worktrees>,
    ui: Res<UiState>,
    mut q: Query<&mut Text, With<HeaderSummary>>,
) {
    let (mut active, mut done) = (0usize, 0usize);
    for topic in &wt.topics {
        if let Some(sel) = &ui.selected_project
            && &topic.project != sel
        {
            continue;
        }
        for task in &topic.tasks {
            if ui.hidden.contains(&task.id) {
                continue;
            }
            if task.done {
                done += 1;
            } else {
                active += 1;
            }
        }
    }
    let scope = ui.selected_project.as_deref().unwrap_or("all projects");
    let mut summary = format!("{scope}  ·  {active} active · {done} done");
    if !ui.hidden.is_empty() {
        summary.push_str(&format!(" · {} hidden", ui.hidden.len()));
    }
    for mut text in &mut q {
        text.0 = summary.clone();
    }
}

fn spawn_task_card(
    commands: &mut Commands,
    fonts: &KitFonts,
    task: &Task,
    prs_expanded: bool,
) -> Entity {
    let dot = if task.done {
        DotState::Done
    } else if task.dirty_total() > 0 {
        DotState::Dirty
    } else {
        DotState::Clean
    };

    let card_id = commands.spawn(card(())).id();

    // ---- Header row: dot · name · (done badge) ----
    // No topic here: the meta row below already labels it.
    let header = commands.spawn(row(feathers_kit::sp::S, ())).id();
    let dot_id = commands.spawn(status_dot(dot)).id();
    let name_id = commands
        .spawn(text_bold(
            fonts,
            task.id.name.clone(),
            15.0,
            tok(ft_text_main()),
        ))
        .id();
    let grow_id = commands.spawn(spacer_grow()).id();
    commands
        .entity(header)
        .add_children(&[dot_id, name_id, grow_id]);
    if task.done {
        let b = commands
            .spawn(badge(fonts, "done", kt::DOT_DONE, tok(ft_text_main())))
            .id();
        commands.entity(header).add_child(b);
    } else if task.ready() {
        // Block has lifted: highlight in the attention colour with the date it freed up.
        let until = task
            .block
            .as_ref()
            .and_then(|b| b.until.clone())
            .unwrap_or_default();
        let b = commands
            .spawn(badge(
                fonts,
                format!("ready · {until}"),
                kt::DOT_DIRTY,
                tok(ft_text_main()),
            ))
            .id();
        commands.entity(header).add_child(b);
    } else if let Some(block) = &task.block {
        let label = match &block.until {
            Some(until) => format!("blocked · until {until}"),
            None => "blocked".to_string(),
        };
        let b = commands
            .spawn(badge(fonts, label, kt::CHIP_BG, tok(ft_text_dim())))
            .id();
        commands.entity(header).add_child(b);
    }
    // Worktree removed but metadata kept: flag it and offer a restore action below.
    if !task.present {
        let b = commands
            .spawn(badge(fonts, "removed", kt::CHIP_BG, tok(ft_text_dim())))
            .id();
        commands.entity(header).add_child(b);
    }
    commands.entity(card_id).add_child(header);

    // ---- Meta row: topic label + repo chips (or "no repos") ----
    let meta = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(feathers_kit::sp::S),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(feathers_kit::sp::XS),
            ..default()
        })
        .id();
    let topic_label = commands
        .spawn(text(fonts, task.id.topic.clone(), 12.0, tok(ft_text_dim())))
        .id();
    commands.entity(meta).add_child(topic_label);
    if task.repos.is_empty() {
        let none = commands
            .spawn(text(fonts, "no repos", 11.0, tok(ft_text_dim())))
            .id();
        commands.entity(meta).add_child(none);
    } else {
        for repo in &task.repos {
            let chip = spawn_repo_chip(commands, fonts, repo);
            commands.entity(meta).add_child(chip);
        }
    }
    if let Some(t) = task.last_active {
        let la = commands
            .spawn(text(
                fonts,
                format!("· {}", rel_time(t)),
                12.0,
                tok(ft_text_dim()),
            ))
            .id();
        commands.entity(meta).add_child(la);
    }
    if let Some(reason) = task.block.as_ref().and_then(|b| b.reason.as_ref()) {
        let r = commands
            .spawn(text(fonts, format!("· {reason}"), 12.0, tok(ft_text_dim())))
            .id();
        commands.entity(meta).add_child(r);
    }
    commands.entity(card_id).add_child(meta);

    // ---- PRs section (only if the task declares any) ----
    if !task.prs.is_empty() {
        let section = spawn_pr_section(commands, fonts, task, prs_expanded);
        commands.entity(card_id).add_child(section);
    }

    // ---- Subtasks section (only if the task owns any) ----
    if !task.subtasks.is_empty() {
        let section = spawn_subtask_section(commands, fonts, task);
        commands.entity(card_id).add_child(section);
    }

    // ---- Actions row ----
    let actions = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(feathers_kit::sp::S),
            ..default()
        })
        .id();
    let done_label = if task.done { "Reopen" } else { "Done" };
    let buttons = [
        commands
            .spawn(icon_button(
                Variant::Normal,
                TaskActionButton {
                    action: CardAction::ToggleDone,
                    id: task.id.clone(),
                },
                done_label,
            ))
            .id(),
        commands
            .spawn(icon_button(
                Variant::Normal,
                TaskActionButton {
                    action: CardAction::Terminal,
                    id: task.id.clone(),
                },
                "Terminal",
            ))
            .id(),
        commands
            .spawn(icon_button(
                Variant::Normal,
                TaskActionButton {
                    action: CardAction::Notes,
                    id: task.id.clone(),
                },
                "Notes",
            ))
            .id(),
        commands
            .spawn(icon_button(
                Variant::Normal,
                TaskActionButton {
                    action: CardAction::Hide,
                    id: task.id.clone(),
                },
                "Hide",
            ))
            .id(),
        {
            // Present tasks open a cdev session; removed tasks re-create their worktrees first.
            let (action, label) = if task.present {
                (CardAction::Cdev, "Open hcdev")
            } else {
                (CardAction::Restore, "Restore")
            };
            commands
                .spawn(icon_button(
                    Variant::Primary,
                    TaskActionButton {
                        action,
                        id: task.id.clone(),
                    },
                    label,
                ))
                .id()
        },
    ];
    commands.entity(actions).add_children(&buttons);
    commands.entity(card_id).add_child(actions);

    card_id
}

fn spawn_repo_chip(commands: &mut Commands, fonts: &KitFonts, repo: &Repo) -> Entity {
    let chip = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(feathers_kit::sp::XS),
                padding: UiRect::axes(Val::Px(feathers_kit::sp::S), Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            ThemeBackgroundColor(tok(kt::CHIP_BG)),
        ))
        .id();

    let name = commands
        .spawn(text(fonts, repo.name.clone(), 11.0, tok(ft_text_dim())))
        .id();
    commands.entity(chip).add_child(name);

    let branch = if !repo.git.loaded {
        "…".to_string()
    } else {
        repo.git
            .branch
            .clone()
            .or_else(|| repo.git.head.clone().map(|h| format!("@{h}")))
            .unwrap_or_else(|| "-".into())
    };
    let branch_id = commands
        .spawn(text_mono(fonts, branch, 11.0, tok(ft_text_main())))
        .id();
    commands.entity(chip).add_child(branch_id);

    if repo.git.dirty > 0 {
        let dirty = commands
            .spawn(text(
                fonts,
                format!("±{}", repo.git.dirty),
                11.0,
                tok(kt::DOT_DIRTY),
            ))
            .id();
        commands.entity(chip).add_child(dirty);
    }
    chip
}

/// The (label, color token) for a PR status. `Open` ("review") and the awaiting-review count both
/// use the amber attention hue so work waiting for approval stands out.
fn pr_status_style(status: PrStatus) -> (&'static str, &'static str) {
    match status {
        PrStatus::Loading => ("…", ft_text_dim()),
        PrStatus::Draft => ("draft", ft_text_dim()),
        PrStatus::Open => ("review", kt::DOT_DIRTY),
        PrStatus::Merged => ("merged", kt::STATUS_MERGED),
        PrStatus::Closed => ("closed", kt::STATUS_CLOSED),
        PrStatus::Unknown => ("?", ft_text_dim()),
    }
}

/// Brighter "hover" twins of the PR-state fill colors, registered by [`install_pr_hover_tokens`] so
/// a PR button lightens on mouseover (the affordance the feathers buttons get for free).
mod pr_hover {
    pub const DIM: &str = "hwt.pr.hover.dim";
    pub const DIRTY: &str = "hwt.pr.hover.dirty";
    pub const MERGED: &str = "hwt.pr.hover.merged";
    pub const CLOSED: &str = "hwt.pr.hover.closed";
}

/// The hover-color token paired with a PR status's normal fill (see [`pr_status_style`]).
fn pr_hover_token(status: PrStatus) -> &'static str {
    match status {
        PrStatus::Open => pr_hover::DIRTY,
        PrStatus::Merged => pr_hover::MERGED,
        PrStatus::Closed => pr_hover::CLOSED,
        PrStatus::Loading | PrStatus::Draft | PrStatus::Unknown => pr_hover::DIM,
    }
}

/// Register the PR-button hover colors (lighter shades of the state fills) into the theme.
fn install_pr_hover_tokens(mut theme: ResMut<Theme>) {
    let c = Color::srgb_u8;
    theme.set_color(pr_hover::DIM, c(0xa8, 0xb0, 0xbd)); // ft.text.dim → lighter
    theme.set_color(pr_hover::DIRTY, c(0xea, 0xb9, 0x4f)); // DOT_DIRTY amber → lighter
    theme.set_color(pr_hover::MERGED, c(0xbf, 0x99, 0xff)); // STATUS_MERGED purple → lighter
    theme.set_color(pr_hover::CLOSED, c(0xee, 0x85, 0x85)); // STATUS_CLOSED red → lighter
}

/// The collapsible PR section: a clickable header (`▸/▾ PRs (N)` + an amber awaiting-review count)
/// and, when expanded, one `- #N title (state)` line per PR. Returns the section's root entity.
fn spawn_pr_section(
    commands: &mut Commands,
    fonts: &KitFonts,
    task: &Task,
    expanded: bool,
) -> Entity {
    let section = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(feathers_kit::sp::XS),
            ..default()
        })
        .id();

    // Header: toggle button + (amber) awaiting-review count.
    let header = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(feathers_kit::sp::S),
            ..default()
        })
        .id();
    let arrow = if expanded { "▾" } else { "▸" };
    let toggle = commands
        .spawn(icon_button(
            Variant::Normal,
            TaskActionButton {
                action: CardAction::TogglePrs,
                id: task.id.clone(),
            },
            format!("{arrow} PRs ({})", task.prs.len()),
        ))
        .id();
    commands.entity(header).add_child(toggle);
    let awaiting = task.awaiting_review();
    if awaiting > 0 {
        let txt = commands
            .spawn(text(
                fonts,
                format!("· {awaiting} awaiting review"),
                12.0,
                tok(kt::DOT_DIRTY),
            ))
            .id();
        commands.entity(header).add_child(txt);
    }
    commands.entity(section).add_child(header);

    // Expanded list: one line per PR.
    if expanded {
        for pr in &task.prs {
            let line = spawn_pr_line(commands, fonts, pr);
            commands.entity(section).add_child(line);
        }
    }
    section
}

/// One PR rendered as a clickable button whose fill is colored by PR state (amber = awaiting
/// review, purple = merged, red = closed, dim = draft/loading/unknown). The label reads
/// `#N title (state) · repo · opened date`. Clicking opens the PR's GitHub URL in the browser.
fn spawn_pr_line(commands: &mut Commands, fonts: &KitFonts, pr: &Pr) -> Entity {
    let num = pr
        .number
        .map(|n| format!("#{n}"))
        .unwrap_or_else(|| "#?".into());
    let (state, color) = pr_status_style(pr.status);
    let hover = pr_hover_token(pr.status);

    let mut label = format!("{num} ");
    let title = pr.display_title();
    if !title.is_empty() {
        label.push_str(title);
        label.push(' ');
    }
    label.push_str(&format!("({state})"));
    if let Some(repo) = &pr.repo {
        label.push_str(&format!(" · {repo}"));
    }
    if let Some(opened) = &pr.opened {
        label.push_str(&format!(" · opened {opened}"));
    }

    // The button is a Node carrying the bare `Button` marker (which drives `on_activate`); we paint
    // the background with the state color ourselves rather than going through the feathers button,
    // whose styling system would reset the fill to its neutral variant color every frame.
    let btn = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(28.0),
                padding: UiRect::axes(Val::Px(feathers_kit::sp::M), Val::Px(0.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            ThemeBackgroundColor(tok(color)),
            KitHoverable {
                normal: color,
                hover,
            },
            Button,
            Hovered::default(),
            PrButton {
                url: pr.url.clone(),
            },
        ))
        .id();
    // Dark label text for contrast against the (bright) state-colored fill.
    let lbl = commands
        .spawn(text(fonts, label, 12.0, tok(kt::PANEL_BG)))
        .id();
    commands.entity(btn).add_child(lbl);
    btn
}

/// The Subtasks block: a dim `Subtasks (N)` label and one clickable line per owned subtask,
/// always shown and indented to nest it under the parent task. Returns the section root.
fn spawn_subtask_section(commands: &mut Commands, fonts: &KitFonts, task: &Task) -> Entity {
    let section = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(feathers_kit::sp::XS),
            // Indent the whole block to show the parent → subtask nesting.
            padding: UiRect::left(Val::Px(feathers_kit::sp::M)),
            ..default()
        })
        .id();

    let header = commands
        .spawn(text(
            fonts,
            format!("Subtasks ({})", task.subtasks.len()),
            12.0,
            tok(ft_text_dim()),
        ))
        .id();
    commands.entity(section).add_child(header);

    let owner_project = task.id.project.clone();
    let owner_topic = task.id.topic.clone();
    for sub in &task.subtasks {
        let line = spawn_subtask_line(commands, fonts, sub, &owner_project, &owner_topic);
        commands.entity(section).add_child(line);
    }
    section
}

/// One owned subtask as a clickable chip-filled button. The label reads `task · blurb-or-goal`.
/// Clicking filters the dashboard to the child's topic so its card comes into view; the child's
/// [`TaskId`] rides along via [`SubtaskButton`].
fn spawn_subtask_line(
    commands: &mut Commands,
    fonts: &KitFonts,
    sub: &SubtaskRef,
    owner_project: &str,
    owner_topic: &str,
) -> Entity {
    let id = sub.resolved_id(owner_project, owner_topic);
    let mut label = sub.task.clone();
    let detail = sub.display_label();
    if detail != sub.task {
        label.push_str(&format!(" · {detail}"));
    }
    if let Some(created) = &sub.created {
        label.push_str(&format!(" · {created}"));
    }

    let btn = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(28.0),
                padding: UiRect::axes(Val::Px(feathers_kit::sp::M), Val::Px(0.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            ThemeBackgroundColor(tok(kt::CHIP_BG)),
            KitHoverable {
                normal: kt::CHIP_BG,
                hover: kt::SIDEBAR_ITEM_SEL,
            },
            Button,
            Hovered::default(),
            SubtaskButton { id },
        ))
        .id();
    let lbl = commands
        .spawn(text(fonts, label, 12.0, tok(ft_text_main())))
        .id();
    commands.entity(btn).add_child(lbl);
    btn
}

// ---------------------------------------------------------------------------
// Click handling
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn on_activate(
    ev: On<Activate>,
    topics: Query<&ProjectButton>,
    toggles: Query<(), With<ShowDoneToggle>>,
    refreshes: Query<(), With<RefreshButton>>,
    clears: Query<(), With<ClearCacheButton>>,
    actions: Query<&TaskActionButton>,
    pr_buttons: Query<&PrButton>,
    subtask_buttons: Query<&SubtaskButton>,
    mut ui: ResMut<UiState>,
    mut wt: ResMut<Worktrees>,
    mut pr_channel: Option<ResMut<PrChannel>>,
    mut pr_cache: Option<ResMut<PrCache>>,
    mut commands: Commands,
) {
    let e = ev.entity;

    if let Ok(tb) = topics.get(e) {
        ui.selected_project = tb.0.clone();
        return;
    }
    if toggles.contains(e) {
        ui.show_done = !ui.show_done;
        return;
    }
    if refreshes.contains(e) {
        ui.hidden.clear();
        *wt = scan();
        kick_git(&wt, &mut commands);
        // Refresh re-scans and re-checks, but keeps the terminal-status cache: merged/closed PRs are
        // still served from it, so a refresh only re-queries the PRs that can actually change. Use
        // the Clear cache button to force merged/closed PRs to be re-checked too. The rescan resets
        // every PR to `Loading`; clearing the dispatch guard lets `kick_pr_on_change` re-query the
        // visible tasks (it runs because `wt`/`ui` just changed).
        if let Some(channel) = pr_channel.as_mut() {
            channel.dispatched.clear();
        }
        return;
    }
    if clears.contains(e) {
        // Wipe the terminal-status cache (disk + memory), then re-query the visible tasks right away
        // so even merged/closed PRs are checked against `gh` again.
        clear_pr_cache();
        if let Some(cache) = pr_cache.as_mut() {
            cache.map.clear();
        }
        if let (Some(channel), Some(cache)) = (pr_channel.as_mut(), pr_cache.as_ref()) {
            channel.dispatched.clear();
            dispatch_visible_prs(&wt, &ui, channel, &cache.map);
        }
        return;
    }
    if let Ok(pr) = pr_buttons.get(e) {
        open_url(&pr.url);
        return;
    }
    if let Ok(sub) = subtask_buttons.get(e) {
        ui.selected_project = Some(sub.id.project.clone());
        return;
    }
    if let Ok(act) = actions.get(e) {
        match act.action {
            CardAction::ToggleDone => {
                if let Some(task) = wt.task_mut(&act.id) {
                    match toggle_done(&act.id) {
                        Ok(done) => task.done = done,
                        Err(err) => warn!("toggle done failed for {:?}: {err}", act.id),
                    }
                }
            }
            CardAction::Cdev => {
                if let Some(task) = wt.task(&act.id) {
                    launch_cdev(task);
                }
            }
            CardAction::Restore => {
                if let Some(task) = wt.task(&act.id) {
                    launch_restore(task);
                }
            }
            CardAction::Terminal => {
                if let Some(task) = wt.task(&act.id) {
                    launch_terminal(task);
                }
            }
            CardAction::Notes => {
                if let Some(task) = wt.task(&act.id) {
                    open_notes(task);
                }
            }
            CardAction::Hide => {
                ui.hidden.insert(act.id.clone());
            }
            CardAction::TogglePrs => {
                if !ui.pr_expanded.remove(&act.id) {
                    ui.pr_expanded.insert(act.id.clone());
                }
            }
        }
    }
}
