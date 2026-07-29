//! UI layer: static chrome built once, systems that rebuild the sidebar and issue list
//! when the model or selection changes, and a global observer turning clicks into
//! actions. Structurally a sibling of `hwt-overview`'s `ui.rs` — same channel-drain,
//! same lazy dispatch, same rebuild-on-resource-changed shape.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender};

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_feathers::theme::ThemeBackgroundColor;

use feathers_kit::{
    Activate, DotState, KitFonts, KitSelected, Variant, badge, card, divider,
    feathers_tokens as ft, icon_button, kit_tokens as kt, row, scroll_list, sidebar_item,
    spacer_grow, status_dot, text, text_bold, text_mono, tok,
};

use crate::issues::{
    FetchState, Issue, IssueMsg, LaunchSpec, Model, UiState, fetch_issues, launch_hcdev,
    launch_start, launch_terminal, open_url, rel_time, scan,
};

/// Holds both ends of the async issue query. `tx` is retained so fetches can be
/// dispatched incrementally as projects become visible, all feeding one receiver;
/// `dispatched` records which projects have already been sent to `gh` so a filter change
/// never re-queries one.
#[derive(Resource)]
struct IssueChannel {
    tx: Sender<IssueMsg>,
    rx: Mutex<Receiver<IssueMsg>>,
    dispatched: HashSet<String>,
}

/// Sidebar width in logical pixels.
const SIDEBAR_W: f32 = 250.0;

// ---------------------------------------------------------------------------
// Markers
// ---------------------------------------------------------------------------

#[derive(Component)]
struct SidebarContainer;

#[derive(Component)]
struct IssueListContainer;

/// A sidebar project button. `None` = the "All" entry.
#[derive(Component, Clone)]
struct ProjectButton(Option<String>);

#[derive(Component)]
struct ShowUntaggedToggle;

#[derive(Component)]
struct RefreshButton;

/// The dim count summary text in the header.
#[derive(Component)]
struct HeaderSummary;

#[derive(Clone, Copy)]
enum IssueAction {
    /// Open the issue on GitHub.
    Browse,
    /// Open an `hcdev` session in the issue's existing worktree.
    Hcdev,
    /// Create the worktree, then open an `hcdev` session in it.
    Start,
    /// Plain terminal in the issue's worktree.
    Terminal,
    /// Hide this issue for the session.
    Hide,
}

/// A button on an issue card. The (project, number) pair identifies the issue; the model
/// is re-read on click so the action always uses current data.
#[derive(Component, Clone)]
struct IssueActionButton {
    action: IssueAction,
    project: String,
    number: u64,
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Wires up the issue dashboard UI.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Model>()
            .init_resource::<UiState>()
            .add_systems(Startup, (initial_scan, kick_startup, build_chrome).chain())
            .add_systems(
                Update,
                (
                    drain_issues,
                    kick_on_change
                        .run_if(resource_changed::<Model>.or_else(resource_changed::<UiState>)),
                    rebuild_sidebar
                        .run_if(resource_changed::<Model>.or_else(resource_changed::<UiState>)),
                    rebuild_issue_list
                        .run_if(resource_changed::<Model>.or_else(resource_changed::<UiState>)),
                    update_header
                        .run_if(resource_changed::<Model>.or_else(resource_changed::<UiState>)),
                )
                    .chain(),
            )
            .add_observer(on_activate);
    }
}

fn initial_scan(mut model: ResMut<Model>) {
    *model = scan();
}

fn kick_startup(model: Res<Model>, ui: Res<UiState>, mut commands: Commands) {
    let (tx, rx) = std::sync::mpsc::channel::<IssueMsg>();
    let mut channel = IssueChannel {
        tx,
        rx: Mutex::new(rx),
        dispatched: HashSet::new(),
    };
    dispatch_visible_projects(&model, &ui, &mut channel);
    commands.insert_resource(channel);
}

/// Fetch issues for any newly-visible project when the selection changes (or the model is
/// rescanned). The `dispatched` guard keeps this to one `gh` call per project per scan, so
/// selecting a project loads it on demand instead of querying every checkout up front —
/// which matters because `~/fun_repos` holds repos that are not on GitHub at all.
fn kick_on_change(model: Res<Model>, ui: Res<UiState>, channel: Option<ResMut<IssueChannel>>) {
    let Some(mut channel) = channel else {
        return;
    };
    dispatch_visible_projects(&model, &ui, &mut channel);
}

/// Spawn one `IoTaskPool` task per not-yet-queried visible project, streaming results back
/// over the shared channel.
fn dispatch_visible_projects(model: &Model, ui: &UiState, channel: &mut IssueChannel) {
    let pool = IoTaskPool::get();
    for entry in &model.projects {
        if !project_visible(&entry.project.name, ui) {
            continue;
        }
        if !channel.dispatched.insert(entry.project.name.clone()) {
            continue;
        }
        let tx = channel.tx.clone();
        let project = entry.project.clone();
        pool.spawn(async move {
            let result = fetch_issues(&project);
            let _ = tx.send(IssueMsg {
                project: project.name,
                result,
            });
        })
        .detach();
    }
}

/// Drain any fetch results that have arrived and fold them into the model. Touches `Model`
/// mutably only when there is at least one message, so idle frames don't trigger a rebuild.
fn drain_issues(channel: Option<Res<IssueChannel>>, mut model: ResMut<Model>) {
    let Some(channel) = channel else {
        return;
    };
    let msgs: Vec<IssueMsg> = {
        let Ok(rx) = channel.rx.lock() else {
            return;
        };
        let mut v = Vec::new();
        while let Ok(m) = rx.try_recv() {
            v.push(m);
            if v.len() >= 64 {
                break;
            }
        }
        v
    };
    if msgs.is_empty() {
        return;
    }
    for msg in msgs {
        let Some(entry) = model.project_mut(&msg.project) else {
            continue;
        };
        match msg.result {
            Ok(issues) => {
                entry.issues = issues;
                entry.state = FetchState::Ready;
            }
            Err(reason) => {
                entry.issues.clear();
                entry.state = FetchState::Failed(reason);
            }
        }
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
                    scroll_list(IssueListContainer, ())
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
            text_bold(fonts, "Open issues", 18.0, tok(ft_text_main())),
            (text(fonts, "", 13.0, tok(ft_text_dim())), HeaderSummary,),
            spacer_grow(),
            (
                Node {
                    // Wide enough for the label on one line; a narrower box wraps it and
                    // the text spills out of the button.
                    width: Val::Px(150.0),
                    ..default()
                },
                children![icon_button(
                    Variant::Normal,
                    ShowUntaggedToggle,
                    "Toggle untagged"
                )],
            ),
            (
                Node {
                    width: Val::Px(90.0),
                    ..default()
                },
                children![icon_button(Variant::Normal, RefreshButton, "Refresh")],
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
    model: Res<Model>,
    ui: Res<UiState>,
    fonts: Res<KitFonts>,
) {
    let Ok(c) = container.single() else {
        return;
    };
    commands.entity(c).despawn_children();

    let total: usize = model
        .projects
        .iter()
        .map(|p| p.issues.iter().filter(|i| passes_filters(i, &ui)).count())
        .sum();
    spawn_project_row(
        &mut commands,
        c,
        &fonts,
        None,
        "All",
        total,
        ui.selected_project.is_none(),
    );

    for entry in &model.projects {
        let count = entry
            .issues
            .iter()
            .filter(|i| passes_filters(i, &ui))
            .count();
        let selected = ui.selected_project.as_deref() == Some(entry.project.name.as_str());
        spawn_project_row(
            &mut commands,
            c,
            &fonts,
            Some(entry.project.name.clone()),
            &entry.project.name,
            count,
            selected,
        );
    }
}

fn spawn_project_row(
    commands: &mut Commands,
    parent: Entity,
    fonts: &KitFonts,
    project: Option<String>,
    label: &str,
    count: usize,
    selected: bool,
) {
    let label_token = if selected { kt::ACCENT } else { ft_text_main() };
    let id = commands
        .spawn(sidebar_item(
            ProjectButton(project),
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

/// Whether a project's issues are on screen — and therefore whether its `gh` call is
/// worth making. Shared by [`rebuild_issue_list`] and [`dispatch_visible_projects`] so
/// fetches track exactly what the user selected.
fn project_visible(project: &str, ui: &UiState) -> bool {
    ui.selected_project
        .as_deref()
        .is_none_or(|sel| sel == project)
}

/// Whether an issue passes the untagged toggle. Deliberately *not* checking `ui.hidden`:
/// the sidebar counts and the header use this, and a hide is a per-session dismissal that
/// should not make the project's issue count drop — the header reports hides separately.
fn passes_filters(issue: &Issue, ui: &UiState) -> bool {
    issue.topic.is_some() || ui.show_untagged
}

// ---------------------------------------------------------------------------
// Issue list rebuild
// ---------------------------------------------------------------------------

fn rebuild_issue_list(
    mut commands: Commands,
    container: Query<Entity, With<IssueListContainer>>,
    model: Res<Model>,
    ui: Res<UiState>,
    fonts: Res<KitFonts>,
) {
    let Ok(c) = container.single() else {
        return;
    };
    commands.entity(c).despawn_children();

    let mut shown = 0;
    for entry in &model.projects {
        if !project_visible(&entry.project.name, &ui) {
            continue;
        }
        // A project that is still loading, failed, or simply has nothing open says so —
        // an empty area would be indistinguishable from a broken fetch.
        match &entry.state {
            FetchState::Loading => {
                let line = commands
                    .spawn(text(
                        &fonts,
                        format!("{} · loading…", entry.project.name),
                        12.0,
                        tok(ft_text_dim()),
                    ))
                    .id();
                commands.entity(c).add_child(line);
                continue;
            }
            FetchState::Failed(reason) => {
                let line = commands
                    .spawn(text(
                        &fonts,
                        format!("{} · no issues from gh: {reason}", entry.project.name),
                        12.0,
                        tok(kt::STATUS_CLOSED),
                    ))
                    .id();
                commands.entity(c).add_child(line);
                continue;
            }
            FetchState::Ready => {}
        }

        let visible: Vec<&Issue> = entry
            .issues
            .iter()
            .filter(|i| {
                passes_filters(i, &ui)
                    && !ui.hidden.contains(&(entry.project.name.clone(), i.number))
            })
            .collect();
        if visible.is_empty() {
            let line = commands
                .spawn(text(
                    &fonts,
                    format!("{} · no open issues", entry.project.name),
                    12.0,
                    tok(ft_text_dim()),
                ))
                .id();
            commands.entity(c).add_child(line);
            continue;
        }
        for issue in visible {
            let card_id = spawn_issue_card(&mut commands, &fonts, &entry.project.name, issue);
            commands.entity(c).add_child(card_id);
            shown += 1;
        }
    }

    if shown == 0 && model.projects.is_empty() {
        commands.entity(c).with_child(text(
            &fonts,
            "No projects under ~/fun_repos.",
            14.0,
            tok(ft_text_dim()),
        ));
    }
}

fn update_header(
    model: Res<Model>,
    ui: Res<UiState>,
    mut q: Query<&mut Text, With<HeaderSummary>>,
) {
    let (mut open, mut tagged, mut started) = (0usize, 0usize, 0usize);
    for (project, issue) in model.all() {
        if !project_visible(project, &ui) {
            continue;
        }
        open += 1;
        if issue.topic.is_some() {
            tagged += 1;
        }
        if issue.worktree.is_some() {
            started += 1;
        }
    }
    let scope = ui.selected_project.as_deref().unwrap_or("all projects");
    let mut summary =
        format!("{scope}  ·  {open} open · {tagged} tagged · {started} with a worktree");
    if open > tagged && !ui.show_untagged {
        summary.push_str(&format!(" · {} untagged hidden", open - tagged));
    }
    if !ui.hidden.is_empty() {
        summary.push_str(&format!(" · {} hidden", ui.hidden.len()));
    }
    for mut text in &mut q {
        text.0 = summary.clone();
    }
}

/// One issue card: `#N title`, a topic (or `untagged`) badge, a meta line, and the action
/// row whose primary button is **Open hcdev** for an issue that already has a worktree and
/// **Start task** for one that does not.
fn spawn_issue_card(
    commands: &mut Commands,
    fonts: &KitFonts,
    project: &str,
    issue: &Issue,
) -> Entity {
    let card_id = commands.spawn(card(())).id();

    // ---- Header row: dot · #N · title · topic badge ----
    // Green dot = a worktree exists (work under way); amber = nothing started yet.
    let dot = if issue.worktree.is_some() {
        DotState::Clean
    } else {
        DotState::Dirty
    };
    let header = commands.spawn(row(feathers_kit::sp::S, ())).id();
    let dot_id = commands.spawn(status_dot(dot)).id();
    let num_id = commands
        .spawn(text_mono(
            fonts,
            format!("#{}", issue.number),
            13.0,
            tok(ft_text_dim()),
        ))
        .id();
    let title_id = commands
        .spawn(text_bold(
            fonts,
            issue.title.clone(),
            15.0,
            tok(ft_text_main()),
        ))
        .id();
    let grow_id = commands.spawn(spacer_grow()).id();
    commands
        .entity(header)
        .add_children(&[dot_id, num_id, title_id, grow_id]);

    match &issue.topic {
        Some(topic) => {
            let b = commands
                .spawn(badge(
                    fonts,
                    format!("topic:{topic}"),
                    kt::ACCENT,
                    tok(kt::PANEL_BG),
                ))
                .id();
            commands.entity(header).add_child(b);
        }
        None => {
            // Nothing to launch without a topic, so say why rather than showing a dead button.
            let b = commands
                .spawn(badge(fonts, "untagged", kt::CHIP_BG, tok(ft_text_dim())))
                .id();
            commands.entity(header).add_child(b);
        }
    }
    if issue.worktree.is_some() {
        let b = commands
            .spawn(badge(fonts, "worktree", kt::DOT_CLEAN, tok(kt::PANEL_BG)))
            .id();
        commands.entity(header).add_child(b);
    }
    commands.entity(card_id).add_child(header);

    // ---- Meta row: project · task name · updated · author · other labels ----
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
    let mut line = project.to_owned();
    if let Some(t) = issue.updated {
        line.push_str(&format!(" · updated {}", rel_time(t)));
    }
    if let Some(author) = &issue.author {
        line.push_str(&format!(" · @{author}"));
    }
    let line_id = commands
        .spawn(text(fonts, line, 12.0, tok(ft_text_dim())))
        .id();
    commands.entity(meta).add_child(line_id);
    // The task/branch name an action would use — visible before clicking, since it is
    // what the worktree and branch get called.
    let task_id = commands
        .spawn(text_mono(
            fonts,
            issue.task.clone(),
            11.0,
            tok(ft_text_main()),
        ))
        .id();
    commands.entity(meta).add_child(task_id);
    for label in issue.other_labels() {
        let chip = commands
            .spawn(badge(fonts, label.clone(), kt::CHIP_BG, tok(ft_text_dim())))
            .id();
        commands.entity(meta).add_child(chip);
    }
    commands.entity(card_id).add_child(meta);

    // ---- Actions row ----
    let actions = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(feathers_kit::sp::S),
            ..default()
        })
        .id();
    let button = |commands: &mut Commands, variant, action, label: &str| {
        commands
            .spawn(icon_button(
                variant,
                IssueActionButton {
                    action,
                    project: project.to_owned(),
                    number: issue.number,
                },
                label,
            ))
            .id()
    };

    let mut buttons = vec![
        button(commands, Variant::Normal, IssueAction::Browse, "Browser"),
        button(commands, Variant::Normal, IssueAction::Hide, "Hide"),
    ];
    if issue.worktree.is_some() {
        buttons.push(button(
            commands,
            Variant::Normal,
            IssueAction::Terminal,
            "Terminal",
        ));
        buttons.push(button(
            commands,
            Variant::Primary,
            IssueAction::Hcdev,
            "Open hcdev",
        ));
    } else if issue.topic.is_some() {
        buttons.push(button(
            commands,
            Variant::Primary,
            IssueAction::Start,
            "Start task",
        ));
    } else {
        // Untagged: nothing to start. Name the missing piece where the button would be.
        let hint = commands
            .spawn(text(
                fonts,
                "add a topic:<name> label to start a task",
                12.0,
                tok(ft_text_dim()),
            ))
            .id();
        buttons.push(hint);
    }
    commands.entity(actions).add_children(&buttons);
    commands.entity(card_id).add_child(actions);

    card_id
}

// ---------------------------------------------------------------------------
// Click handling
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn on_activate(
    ev: On<Activate>,
    projects: Query<&ProjectButton>,
    untagged: Query<(), With<ShowUntaggedToggle>>,
    refreshes: Query<(), With<RefreshButton>>,
    actions: Query<&IssueActionButton>,
    mut ui: ResMut<UiState>,
    mut model: ResMut<Model>,
    mut channel: Option<ResMut<IssueChannel>>,
) {
    let e = ev.entity;

    if let Ok(pb) = projects.get(e) {
        ui.selected_project = pb.0.clone();
        return;
    }
    if untagged.contains(e) {
        ui.show_untagged = !ui.show_untagged;
        return;
    }
    if refreshes.contains(e) {
        // Re-scan projects (a new checkout may have appeared) and clear the dispatch guard
        // so `kick_on_change` re-queries the visible ones — it runs because both resources
        // just changed. Worktree state is re-read as part of the fetch, so a task started
        // from here shows its `worktree` badge after a refresh.
        ui.hidden.clear();
        *model = scan();
        if let Some(channel) = channel.as_mut() {
            channel.dispatched.clear();
        }
        return;
    }
    let Ok(act) = actions.get(e) else {
        return;
    };
    let Some(issue) = model.issue(&act.project, act.number) else {
        return;
    };
    match act.action {
        IssueAction::Browse => open_url(&issue.url),
        IssueAction::Terminal => {
            if let Some(dir) = &issue.worktree {
                launch_terminal(dir, &format!("term-{}", issue.task));
            }
        }
        IssueAction::Hcdev => {
            // The worktree on disk is the authority here, not the issue's label: it was
            // found by issue number, so its own topic directory and task-dir name are what
            // `hcdev wt` has to be given. The `topic:` label can disagree — relabelled
            // since the worktree was made, or missing entirely.
            if let Some(dir) = &issue.worktree {
                match spec_from_worktree_path(dir, &act.project) {
                    Some(spec) => launch_hcdev(dir, &spec),
                    None => warn!(
                        "worktree {} is not under <project>/<topic>/<task>",
                        dir.display()
                    ),
                }
            }
        }
        IssueAction::Start => {
            if let Some(spec) = issue.launch_spec(&act.project) {
                launch_start(&spec);
            }
        }
        IssueAction::Hide => {
            ui.hidden.insert((act.project.clone(), act.number));
        }
    }
}

/// Read the topic and task back out of an existing worktree path
/// (`~/fun_worktrees/<project>/<topic>/<task>`). `None` for a path too shallow to carry
/// both, which is the only way `hcdev wt` could be handed nonsense.
fn spec_from_worktree_path(dir: &std::path::Path, project: &str) -> Option<LaunchSpec> {
    let task = dir.file_name()?.to_string_lossy().into_owned();
    let topic = dir.parent()?.file_name()?.to_string_lossy().into_owned();
    Some(LaunchSpec {
        project: project.to_owned(),
        topic,
        task,
    })
}
