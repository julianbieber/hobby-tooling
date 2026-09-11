//! The static window chrome — sidebar, header bar, and the two containers the list
//! systems refill — plus the sidebar and header rebuilds.

use bevy::prelude::*;
use bevy_feathers::theme::ThemeBackgroundColor;
use std::collections::BTreeMap;

use feathers_kit::{
    KitFonts, KitSelected, Variant, badge, divider, feathers_tokens as ft, icon_button,
    kit_tokens as kt, scroll_list, sidebar_item, spacer_grow, text, text_bold, tok,
};

use crate::model::join::{Item, Model, Stage, UiState};

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

#[derive(Component)]
pub struct ShowUntaggedToggle;

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
                140.0,
                icon_button(Variant::Normal, ShowUntaggedToggle, "Show untagged")
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
    !matches!(
        (item.stage(), ui.show_done, ui.show_untagged),
        (Stage::Done, false, _) | (Stage::Untagged, _, false)
    )
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

    // Every project gets a row, even one whose issue fetch failed and which has no
    // worktrees — otherwise a project silently disappears from the sidebar. Since issues
    // are fetched only for the selected project, a row is also the only way to reach them.
    let mut per_project: BTreeMap<&str, usize> = model
        .projects
        .iter()
        .map(|p| (p.project.name.as_str(), 0))
        .collect();
    let items = model.items();
    for item in items.iter().filter(|i| counts(i, &ui)) {
        *per_project.entry(item.project).or_default() += 1;
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
        false,
    );

    let mut worked_in = 0usize;
    for (project, count, has_notes) in project_order(&model, &per_project) {
        // One divider where the worked-in projects end and the rest of `~/fun_repos` begins.
        if !has_notes && worked_in > 0 {
            let d = commands.spawn(divider()).id();
            commands.entity(c).add_child(d);
            worked_in = 0;
        }
        if has_notes {
            worked_in += 1;
        }
        let selected = ui.selected_project.as_deref() == Some(project);
        spawn_project_row(
            &mut commands,
            c,
            &fonts,
            Some(project.to_owned()),
            project,
            count,
            selected,
            pulls.running(project),
        );
    }
}

fn project_order<'a>(
    model: &'a Model,
    per_project: &BTreeMap<&'a str, usize>,
) -> Vec<(&'a str, usize, bool)> {
    let mut rows: Vec<(&str, usize, bool)> = model
        .projects
        .iter()
        .map(|p| {
            let name = p.project.name.as_str();
            (
                name,
                per_project.get(name).copied().unwrap_or(0),
                p.project.has_notes,
            )
        })
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
    rows
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
    pulling: bool,
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

    // The "All" row gets no pull control: `~/fun_repos` holds upstream clones as well as my
    // own projects (see the README's known limitations), so one click that pulls every
    // checkout would fire dozens of network fetches nobody asked for.
    if let Some(name) = project {
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
    let (mut active, mut issues, mut done, mut untagged, mut orphans) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for item in model.items() {
        if let Some(sel) = &ui.selected_project
            && item.project != sel
        {
            continue;
        }
        match item.stage() {
            Stage::Done => done += 1,
            Stage::Untagged => {
                untagged += 1;
                issues += 1;
            }
            Stage::Startable => issues += 1,
            Stage::Active | Stage::Removed | Stage::Coordinator => active += 1,
            Stage::Orphan => orphans += 1,
        }
    }

    let scope = ui.selected_project.as_deref().unwrap_or("all projects");
    let mut summary = format!("{scope}  ·  {active} active");
    // Issues are fetched for the selected project only, so under "All" the count is not a
    // fact about the projects — it is a fact about which ones happened to be visited.
    if ui.selected_project.is_some() {
        summary.push_str(&format!(" · {issues} open issues"));
    }
    if orphans > 0 {
        summary.push_str(&format!(" · {orphans} orphan worktrees"));
    }
    if ui.show_done {
        summary.push_str(&format!(" · {done} done"));
    } else if done > 0 {
        summary.push_str(&format!(" · {done} done hidden"));
    }
    if !ui.show_untagged && untagged > 0 {
        summary.push_str(&format!(" · {untagged} untagged hidden"));
    }
    if !ui.hidden.is_empty() {
        summary.push_str(&format!(" · {} dismissed", ui.hidden.len()));
    }
    for mut text in &mut q {
        text.0 = summary.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::issue::{FetchState, Project, ProjectIssues};
    use std::path::PathBuf;

    fn model_of(projects: &[(&str, bool)]) -> Model {
        Model {
            tasks: Vec::new(),
            projects: projects
                .iter()
                .map(|(name, has_notes)| ProjectIssues {
                    project: Project {
                        name: (*name).to_owned(),
                        path: PathBuf::from("/nowhere").join(name),
                        has_notes: *has_notes,
                    },
                    state: FetchState::Loading,
                    issues: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn worked_in_projects_lead_the_sidebar() {
        let model = model_of(&[
            ("bevy", false),
            ("wusel", true),
            ("helix", false),
            ("hobby-tooling", true),
        ]);
        let mut counts = BTreeMap::new();
        counts.insert("wusel", 3);

        let rows = project_order(&model, &counts);
        let names: Vec<&str> = rows.iter().map(|r| r.0).collect();
        assert_eq!(names, vec!["hobby-tooling", "wusel", "bevy", "helix"]);

        assert!(rows[0].2, "worked-in projects first");
        assert!(!rows[2].2, "the clones follow");
        assert_eq!(rows[1].1, 3, "the count rides along");
        assert_eq!(
            rows[0].1, 0,
            "a project with no counted items still gets a row"
        );
    }
}
