//! The static window chrome — sidebar, header bar, and the two containers the list
//! systems refill — plus the sidebar and header rebuilds.

use bevy::prelude::*;
use bevy_feathers::theme::ThemeBackgroundColor;
use std::collections::BTreeMap;

use feathers_kit::{
    KitFonts, KitSelected, Variant, badge, divider, feathers_tokens as ft, icon_button,
    kit_tokens as kt, scroll_list, sidebar_item, spacer_grow, text, text_bold, tok,
};

use crate::model::item::{Item, Model, Stage, UiState, backlog_visible};

use super::Pulls;

/// Sidebar width in logical pixels. Wide enough for the longest project name *plus* the
/// pull slot and the count badge — the three compete for one row.
const SIDEBAR_W: f32 = 290.0;

/// Fixed width of the pull control, so a running pull's `…` occupies exactly the space its
/// button did and the row does not twitch when one starts.
const PULL_SLOT_W: f32 = 60.0;

// ---------------------------------------------------------------------------
// Markers
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct SidebarContainer;

#[derive(Component)]
pub struct ItemListContainer;

/// A sidebar project button. `None` = the "All" entry.
#[derive(Component, Clone)]
pub struct ProjectButton(pub Option<String>);

/// The **Pull** button on a project's sidebar row, carrying that project's name. Nested
/// inside the row's own [`ProjectButton`]; `bevy_ui_widgets` stops the click from bubbling
/// once a `Button` has handled it, so pulling a project does not also select it.
#[derive(Component, Clone)]
pub struct PullButton(pub String);

#[derive(Component)]
pub struct ShowDoneToggle;

/// Header button switching the list between the open work and the backlog.
#[derive(Component)]
pub struct BacklogModeToggle;

#[derive(Component)]
pub struct RefreshButton;

/// Header button that wipes the terminal-PR-status cache (memory + disk) and re-queries the
/// visible items, so even merged/closed PRs are checked against `gh` again.
#[derive(Component)]
pub struct ClearCacheButton;

/// The dim summary text in the header.
#[derive(Component)]
pub struct HeaderSummary;

// ---------------------------------------------------------------------------
// Token helpers
// ---------------------------------------------------------------------------

/// Token key for main text (the feathers `TEXT_MAIN` token, seeded by the kit).
pub fn ft_text_main() -> &'static str {
    "feathers.text.main"
}

/// Token key for dim text.
pub fn ft_text_dim() -> &'static str {
    "feathers.text.dim"
}

// ---------------------------------------------------------------------------
// Static chrome
// ---------------------------------------------------------------------------

pub fn build_chrome(mut commands: Commands, fonts: Res<KitFonts>) {
    commands.spawn(Camera2d);

    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            min_height: Val::Px(0.0),
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
                    scroll_list(ItemListContainer, ())
                ],
            ),
        ],
    ));
}

fn header_bar(fonts: &KitFonts) -> impl Bundle {
    fn slot(width: f32, button: impl Bundle) -> impl Bundle {
        (
            Node {
                width: Val::Px(width),
                ..default()
            },
            children![button],
        )
    }

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
            text_bold(fonts, "Work", 18.0, tok(ft_text_main())),
            (text(fonts, "", 13.0, tok(ft_text_dim())), HeaderSummary),
            spacer_grow(),
            slot(
                110.0,
                icon_button(Variant::Normal, ShowDoneToggle, "Show done")
            ),
            slot(
                130.0,
                icon_button(Variant::Normal, BacklogModeToggle, "Show backlog")
            ),
            slot(90.0, icon_button(Variant::Normal, RefreshButton, "Refresh")),
            slot(
                110.0,
                icon_button(Variant::Normal, ClearCacheButton, "Clear cache")
            ),
        ],
    )
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

fn counts(item: &Item<'_>, ui: &UiState) -> bool {
    (ui.show_done || item.stage() != Stage::Done) && backlog_visible(item, ui)
}

pub fn rebuild_sidebar(
    mut commands: Commands,
    container: Query<Entity, With<SidebarContainer>>,
    model: Res<Model>,
    ui: Res<UiState>,
    pulls: Res<Pulls>,
    fonts: Res<KitFonts>,
) {
    let Ok(c) = container.single() else {
        return;
    };
    commands.entity(c).despawn_children();

    let mut per_project: BTreeMap<&str, usize> = model
        .projects
        .iter()
        .map(|p| (p.name.as_str(), 0))
        .collect();
    let items = model.items();
    for item in &items {
        let count = per_project.entry(item.project()).or_default();
        if counts(item, &ui) {
            *count += 1;
        }
    }

    let total: usize = per_project.values().sum();
    spawn_project_row(
        &mut commands,
        c,
        &fonts,
        None,
        "All",
        total,
        ui.selected_project.is_none(),
        None,
    );

    for (project, count) in per_project {
        let has_checkout = model
            .projects
            .iter()
            .any(|p| p.name == project && p.has_checkout);
        let selected = ui.selected_project.as_deref() == Some(project);
        spawn_project_row(
            &mut commands,
            c,
            &fonts,
            Some(project.to_owned()),
            project,
            count,
            selected,
            has_checkout.then(|| pulls.running(project)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_project_row(
    commands: &mut Commands,
    parent: Entity,
    fonts: &KitFonts,
    project: Option<String>,
    label: &str,
    count: usize,
    selected: bool,
    pulling: Option<bool>,
) {
    let label_token = if selected { kt::ACCENT } else { ft_text_main() };
    let id = commands
        .spawn(sidebar_item(
            ProjectButton(project.clone()),
            children![
                text(fonts, label.to_string(), 13.0, tok(label_token)),
                spacer_grow(),
            ],
        ))
        .id();

    if let (Some(name), Some(pulling)) = (project, pulling) {
        let slot = pull_slot(commands, fonts, &name, pulling);
        commands.entity(id).add_child(slot);
    }

    let badge_id = commands
        .spawn(badge(
            fonts,
            count.to_string(),
            kt::CHIP_BG,
            tok(ft_text_dim()),
        ))
        .id();
    commands.entity(id).add_child(badge_id);

    if selected {
        commands
            .entity(id)
            .insert((KitSelected, ThemeBackgroundColor(tok(kt::SIDEBAR_ITEM_SEL))));
    }
    commands.entity(parent).add_child(id);
}

fn pull_slot(commands: &mut Commands, fonts: &KitFonts, project: &str, pulling: bool) -> Entity {
    let inner = if pulling {
        commands
            .spawn(text(fonts, "…", 13.0, tok(ft_text_dim())))
            .id()
    } else {
        commands
            .spawn(icon_button(
                Variant::Normal,
                PullButton(project.to_owned()),
                "Pull",
            ))
            .id()
    };
    let slot = commands
        .spawn(Node {
            width: Val::Px(PULL_SLOT_W),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .id();
    commands.entity(slot).add_child(inner);
    slot
}

// ---------------------------------------------------------------------------
// Header summary
// ---------------------------------------------------------------------------

/// The one mutable piece of the header: a dim line reporting the scope, the item mix, and —
/// since the toggle buttons are static — what each toggle is currently suppressing.
pub fn update_header(
    model: Res<Model>,
    ui: Res<UiState>,
    mut q: Query<&mut Text, With<HeaderSummary>>,
) {
    let (mut active, mut unstarted, mut done, mut orphans, mut parked) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for item in model.items() {
        if let Some(sel) = &ui.selected_project
            && item.project() != sel
        {
            continue;
        }
        if !backlog_visible(&item, &ui) {
            parked += 1;
            continue;
        }
        match item.stage() {
            Stage::Done => done += 1,
            Stage::NotStarted => unstarted += 1,
            Stage::Active | Stage::Removed | Stage::Coordinator => active += 1,
            Stage::Orphan => orphans += 1,
        }
    }

    let scope = ui.selected_project.as_deref().unwrap_or("all projects");
    let mut summary = if ui.backlog_mode {
        format!(
            "{scope} backlog  ·  {} backlogged tasks",
            active + unstarted
        )
    } else {
        format!("{scope}  ·  {active} active · {unstarted} not started")
    };
    if orphans > 0 {
        summary.push_str(&format!(" · {orphans} orphan worktrees"));
    }
    if !ui.backlog_mode {
        if ui.show_done {
            summary.push_str(&format!(" · {done} done"));
        } else if done > 0 {
            summary.push_str(&format!(" · {done} done hidden"));
        }
        if parked > 0 {
            summary.push_str(&format!(" · {parked} backlogged hidden"));
        }
    }
    if !ui.hidden.is_empty() {
        summary.push_str(&format!(" · {} dismissed", ui.hidden.len()));
    }
    for mut text in &mut q {
        text.0 = summary.clone();
    }
}
