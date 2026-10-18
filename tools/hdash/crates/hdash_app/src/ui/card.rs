//! One card per item, whatever stage it is at: header row, meta row, action row ending in a
//! Primary button chosen by [`Stage`].

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy_feathers::theme::ThemeBackgroundColor;

use feathers_kit::{
    Button, DotState, KitFonts, KitHoverable, Theme, ThemeExt, Variant, badge, card, icon_button,
    kit_tokens as kt, row, spacer_grow, status_dot, text, text_bold, text_mono, tok,
};
use hwt_core::rel_time;

use super::chrome::{ft_text_dim, ft_text_main};
use crate::model::item::{Item, Stage};
use crate::model::task::{Pr, PrStatus, Repo, SubtaskRef, Task, TaskId};

/// What a card button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CardAction {
    /// Flip the `done` flag in the task's `task.json`.
    ToggleDone,
    /// Write the missing `task.json` for an orphan worktree, making it a task.
    Adopt,
    /// Flip the backlog mark in the task's `task.json`.
    ToggleBacklog,
    /// Open an `hcdev` session in an existing worktree.
    Hcdev,
    /// `hwt restore` the worktree — a removed one, or a first one for a task not started —
    /// then open an `hcdev` session in it.
    Restore,
    /// Open (or re-attach) the coordinator session for a task under the `coordinator` topic.
    Coordinator,
    /// Plain terminal in the worktree.
    Terminal,
    /// Terminal in the task's notes directory.
    Notes,
    /// Dismiss this item for the session.
    Hide,
    /// Expand/collapse the PR list.
    TogglePrs,
}

/// A button on an item card. The [`TaskId`] identifies the item; the model is re-read on
/// click so the action always uses current data.
#[derive(Component, Clone)]
pub struct CardButton {
    pub action: CardAction,
    pub key: TaskId,
}

/// A clickable PR button carrying the PR's GitHub URL; clicking opens it in the browser.
#[derive(Component, Clone)]
pub struct PrButton {
    pub url: String,
}

/// A clickable subtask line carrying the child task's id; clicking filters the dashboard to
/// the child's project so its card comes into view.
#[derive(Component, Clone)]
pub struct SubtaskButton {
    pub id: TaskId,
}

// ---------------------------------------------------------------------------
// The card
// ---------------------------------------------------------------------------

pub fn spawn_item_card(
    commands: &mut Commands,
    fonts: &KitFonts,
    item: &Item<'_>,
    prs_expanded: bool,
) -> Entity {
    let stage = item.stage();
    let key = item.task.id.clone();
    let card_id = commands.spawn(card(())).id();

    let header = header_row(commands, fonts, item, stage);
    let meta = meta_row(commands, fonts, item);
    commands.entity(card_id).add_children(&[header, meta]);

    let task = item.task;
    if !task.prs.is_empty() {
        let section = pr_section(commands, fonts, task, prs_expanded);
        commands.entity(card_id).add_child(section);
    }
    if !task.subtasks.is_empty() {
        let section = subtask_section(commands, fonts, task);
        commands.entity(card_id).add_child(section);
    }

    let actions = action_row(commands, fonts, item, stage, &key);
    commands.entity(card_id).add_child(actions);
    card_id
}

fn header_row(commands: &mut Commands, fonts: &KitFonts, item: &Item<'_>, stage: Stage) -> Entity {
    let header = commands.spawn(row(feathers_kit::sp::S, ())).id();

    let task = item.task;
    let dot = match stage {
        Stage::Done => DotState::Done,
        Stage::Active if task.dirty_total() == 0 => DotState::Clean,
        Stage::Coordinator if task.coordinator.as_ref().is_none_or(|c| c.wants_me() == 0) => {
            DotState::Clean
        }
        _ => DotState::Dirty,
    };
    let dot_id = commands.spawn(status_dot(dot)).id();
    commands.entity(header).add_child(dot_id);

    let title = commands
        .spawn(text_bold(
            fonts,
            item.title().to_owned(),
            15.0,
            tok(ft_text_main()),
        ))
        .id();
    let grow = commands.spawn(spacer_grow()).id();
    commands.entity(header).add_children(&[title, grow]);

    let topic = commands
        .spawn(badge(
            fonts,
            item.topic().to_owned(),
            kt::CHIP_BG,
            tok(ft_text_dim()),
        ))
        .id();
    commands.entity(header).add_child(topic);

    if task.done {
        let b = commands
            .spawn(badge(fonts, "done", kt::DOT_DONE, tok(ft_text_main())))
            .id();
        commands.entity(header).add_child(b);
    } else if task.ready() {
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
    let state_badge = match stage {
        Stage::Orphan => Some(("orphan", kt::DOT_DIRTY, ft_text_main())),
        _ if task.is_coordinator() => Some(("coordinator", kt::ACCENT, kt::PANEL_BG)),
        Stage::NotStarted => Some(("not started", kt::CHIP_BG, ft_text_dim())),
        _ if !task.present => Some(("removed", kt::CHIP_BG, ft_text_dim())),
        _ => None,
    };
    if let Some((label, bg, fg)) = state_badge {
        let b = commands.spawn(badge(fonts, label, bg, tok(fg))).id();
        commands.entity(header).add_child(b);
    }
    header
}

fn meta_row(commands: &mut Commands, fonts: &KitFonts, item: &Item<'_>) -> Entity {
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

    let task = item.task;
    let mut line = format!("{} · {}", item.project(), item.topic());
    if let Some(t) = task.last_active {
        line.push_str(&format!(" · {}", rel_time(t)));
    }
    let line_id = commands
        .spawn(text(fonts, line, 12.0, tok(ft_text_dim())))
        .id();
    commands.entity(meta).add_child(line_id);

    if task.present {
        for repo in &task.repos {
            let chip = repo_chip(commands, fonts, repo);
            commands.entity(meta).add_child(chip);
        }
    } else {
        let name = commands
            .spawn(text_mono(
                fonts,
                item.task_name().to_owned(),
                11.0,
                tok(ft_text_main()),
            ))
            .id();
        commands.entity(meta).add_child(name);
    }

    if task.is_coordinator() {
        let (line, color) = match &task.coordinator {
            Some(counts) if counts.wants_me() > 0 => (counts.summary(), kt::DOT_DIRTY),
            Some(counts) => (counts.summary(), ft_text_main()),
            None => ("no tasks.json".to_owned(), ft_text_dim()),
        };
        let c = commands
            .spawn(text(fonts, format!("· {line}"), 12.0, tok(color)))
            .id();
        commands.entity(meta).add_child(c);
    }

    if let Some(reason) = task.block.as_ref().and_then(|b| b.reason.as_ref()) {
        let r = commands
            .spawn(text(fonts, format!("· {reason}"), 12.0, tok(ft_text_dim())))
            .id();
        commands.entity(meta).add_child(r);
    }
    meta
}

fn action_row(
    commands: &mut Commands,
    fonts: &KitFonts,
    item: &Item<'_>,
    stage: Stage,
    key: &TaskId,
) -> Entity {
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
                CardButton {
                    action,
                    key: key.clone(),
                },
                label,
            ))
            .id()
    };

    let task = item.task;
    let mut buttons = Vec::new();
    if task.adopted {
        buttons.push(button(
            commands,
            Variant::Normal,
            CardAction::ToggleDone,
            if task.done { "Reopen" } else { "Done" },
        ));
        buttons.push(button(
            commands,
            Variant::Normal,
            CardAction::ToggleBacklog,
            if item.backlogged() {
                "Unbacklog"
            } else {
                "Backlog"
            },
        ));
    }
    if task.present {
        buttons.push(button(
            commands,
            Variant::Normal,
            CardAction::Terminal,
            "Terminal",
        ));
    }
    buttons.push(button(
        commands,
        Variant::Normal,
        CardAction::Notes,
        "Notes",
    ));
    buttons.push(button(commands, Variant::Normal, CardAction::Hide, "Hide"));

    match stage {
        Stage::Done if task.is_coordinator() => buttons.push(button(
            commands,
            Variant::Primary,
            CardAction::Coordinator,
            "Open coordinator",
        )),
        Stage::Active | Stage::Done => buttons.push(button(
            commands,
            Variant::Primary,
            CardAction::Hcdev,
            "Open hcdev",
        )),
        Stage::Removed => buttons.push(button(
            commands,
            Variant::Primary,
            CardAction::Restore,
            "Restore",
        )),
        Stage::Coordinator => buttons.push(button(
            commands,
            Variant::Primary,
            CardAction::Coordinator,
            "Open coordinator",
        )),
        Stage::NotStarted => buttons.push(button(
            commands,
            Variant::Primary,
            CardAction::Restore,
            "Start task",
        )),
        Stage::Orphan => {
            let hint = commands
                .spawn(text(
                    fonts,
                    "no task.json in the notes repo",
                    12.0,
                    tok(ft_text_dim()),
                ))
                .id();
            buttons.push(hint);
            buttons.push(button(
                commands,
                Variant::Primary,
                CardAction::Adopt,
                "Adopt",
            ));
        }
    }

    commands.entity(actions).add_children(&buttons);
    actions
}

fn repo_chip(commands: &mut Commands, fonts: &KitFonts, repo: &Repo) -> Entity {
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

// ---------------------------------------------------------------------------
// PR section
// ---------------------------------------------------------------------------

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

mod pr_hover {
    pub const DIM: &str = "hdash.pr.hover.dim";
    pub const DIRTY: &str = "hdash.pr.hover.dirty";
    pub const MERGED: &str = "hdash.pr.hover.merged";
    pub const CLOSED: &str = "hdash.pr.hover.closed";
}

fn pr_hover_token(status: PrStatus) -> &'static str {
    match status {
        PrStatus::Open => pr_hover::DIRTY,
        PrStatus::Merged => pr_hover::MERGED,
        PrStatus::Closed => pr_hover::CLOSED,
        PrStatus::Loading | PrStatus::Draft | PrStatus::Unknown => pr_hover::DIM,
    }
}

/// Register the PR-button hover colors (lighter shades of the state fills) into the theme.
pub fn install_pr_hover_tokens(mut theme: ResMut<Theme>) {
    let c = Color::srgb_u8;
    theme.set_color(pr_hover::DIM, c(0xa8, 0xb0, 0xbd)); // ft.text.dim → lighter
    theme.set_color(pr_hover::DIRTY, c(0xea, 0xb9, 0x4f)); // DOT_DIRTY amber → lighter
    theme.set_color(pr_hover::MERGED, c(0xbf, 0x99, 0xff)); // STATUS_MERGED purple → lighter
    theme.set_color(pr_hover::CLOSED, c(0xee, 0x85, 0x85)); // STATUS_CLOSED red → lighter
}

fn pr_section(commands: &mut Commands, fonts: &KitFonts, task: &Task, expanded: bool) -> Entity {
    let section = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(feathers_kit::sp::XS),
            ..default()
        })
        .id();

    let header = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(feathers_kit::sp::S),
            ..default()
        })
        .id();
    // ASCII, not `▸`/`▾`: the embedded feathers fonts have no Geometric Shapes block, so
    // the triangles render as tofu. `·` and `±` elsewhere are fine — those are Latin-1.
    let arrow = if expanded { "[-]" } else { "[+]" };
    let toggle = commands
        .spawn(icon_button(
            Variant::Normal,
            CardButton {
                action: CardAction::TogglePrs,
                key: task.id.clone(),
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

    if expanded {
        for pr in &task.prs {
            let line = pr_line(commands, fonts, pr);
            commands.entity(section).add_child(line);
        }
    }
    section
}

fn pr_line(commands: &mut Commands, fonts: &KitFonts, pr: &Pr) -> Entity {
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

// ---------------------------------------------------------------------------
// Subtask section
// ---------------------------------------------------------------------------

fn subtask_section(commands: &mut Commands, fonts: &KitFonts, task: &Task) -> Entity {
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

    for sub in &task.subtasks {
        let line = subtask_line(commands, fonts, sub, &task.id.project, &task.id.topic);
        commands.entity(section).add_child(line);
    }
    section
}

fn subtask_line(
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
