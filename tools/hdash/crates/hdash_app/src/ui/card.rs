//! One card per item, whatever stage it is at. The two pre-merge dashboards hand-rolled
//! the same recipe — header row, meta row, action row ending in a Primary button — over
//! their own data; here it is written once and switches on [`Stage`].

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy_feathers::theme::ThemeBackgroundColor;

use feathers_kit::{
    Button, DotState, KitFonts, KitHoverable, Theme, ThemeExt, Variant, badge, card, icon_button,
    kit_tokens as kt, row, spacer_grow, status_dot, text, text_bold, text_mono, tok,
};
use hwt_core::rel_time;

use super::chrome::{ft_text_dim, ft_text_main};
use crate::model::join::{Item, ItemKey, Stage};
use crate::model::task::{Pr, PrStatus, Repo, SubtaskRef, Task, TaskId};

/// What a card button does. The union of both pre-merge action sets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CardAction {
    /// Flip the `done` flag in the task's `task.json`.
    ToggleDone,
    /// Write the missing `task.json` for an orphan worktree, making it a task.
    Adopt,
    /// Flip the backlog mark on the task's `task.json` and the issue's `backlog` label,
    /// whichever exist.
    ToggleBacklog,
    /// Open an `hcdev` session in an existing worktree.
    Hcdev,
    /// `hwt new` the worktree, then open an `hcdev` session in it.
    Start,
    /// `hwt restore` a removed worktree.
    Restore,
    /// Open (or re-attach) the coordinator session for a task under the `coordinator` topic.
    Coordinator,
    /// Plain terminal in the worktree.
    Terminal,
    /// Terminal in the task's notes directory.
    Notes,
    /// Open the issue on GitHub.
    Browse,
    /// Dismiss this item for the session.
    Hide,
    /// Expand/collapse the PR list.
    TogglePrs,
}

/// A button on an item card. The [`ItemKey`] identifies the item; the model is re-read on
/// click so the action always uses current data.
#[derive(Component, Clone)]
pub struct CardButton {
    pub action: CardAction,
    pub key: ItemKey,
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
    let key = item.key();
    let card_id = commands.spawn(card(())).id();

    let header = header_row(commands, fonts, item, stage);
    let meta = meta_row(commands, fonts, item, stage);
    commands.entity(card_id).add_children(&[header, meta]);

    if let Some(task) = item.task {
        if !task.prs.is_empty() {
            let section = pr_section(commands, fonts, task, prs_expanded);
            commands.entity(card_id).add_child(section);
        }
        if !task.subtasks.is_empty() {
            let section = subtask_section(commands, fonts, task);
            commands.entity(card_id).add_child(section);
        }
    }

    let actions = action_row(commands, fonts, item, stage, &key);
    commands.entity(card_id).add_child(actions);
    card_id
}

fn header_row(commands: &mut Commands, fonts: &KitFonts, item: &Item<'_>, stage: Stage) -> Entity {
    let header = commands.spawn(row(feathers_kit::sp::S, ())).id();

    let dot = match stage {
        Stage::Done => DotState::Done,
        // Green means "a live worktree with nothing uncommitted". Everything else — an
        // unstarted issue, a removed worktree, uncommitted work — is amber.
        Stage::Active if item.task.is_some_and(|t| t.dirty_total() == 0) => DotState::Clean,
        Stage::Coordinator
            if item
                .task
                .and_then(|t| t.coordinator.as_ref())
                .is_none_or(|c| c.wants_me() == 0) =>
        {
            DotState::Clean
        }
        Stage::Orphan => DotState::Dirty,
        _ => DotState::Dirty,
    };
    let dot_id = commands.spawn(status_dot(dot)).id();
    commands.entity(header).add_child(dot_id);

    // The issue number, when there is an issue — the one thing a joined card shows that
    // neither pre-merge dashboard could.
    if let Some(issue) = item.issue {
        let num = commands
            .spawn(text_mono(
                fonts,
                format!("#{}", issue.number),
                13.0,
                tok(ft_text_dim()),
            ))
            .id();
        commands.entity(header).add_child(num);
    }

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

    // Topic badge. Accent when it comes from a `topic:` label — there it is the thing
    // that makes the item launchable — and a plain chip when it is just a path level.
    match (item.topic(), stage) {
        (_, Stage::Untagged) => {
            // Nothing to launch without a topic, so say so rather than showing a dead button.
            let b = commands
                .spawn(badge(fonts, "untagged", kt::CHIP_BG, tok(ft_text_dim())))
                .id();
            commands.entity(header).add_child(b);
        }
        (Some(topic), Stage::Startable) => {
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
        (Some(topic), _) => {
            let b = commands
                .spawn(badge(
                    fonts,
                    topic.to_owned(),
                    kt::CHIP_BG,
                    tok(ft_text_dim()),
                ))
                .id();
            commands.entity(header).add_child(b);
        }
        (None, _) => {}
    }

    if let Some(task) = item.task {
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
        if !task.adopted {
            let b = commands
                .spawn(badge(fonts, "orphan", kt::DOT_DIRTY, tok(ft_text_main())))
                .id();
            commands.entity(header).add_child(b);
        } else if task.is_coordinator() {
            let b = commands
                .spawn(badge(fonts, "coordinator", kt::ACCENT, tok(kt::PANEL_BG)))
                .id();
            commands.entity(header).add_child(b);
        } else if !task.present {
            let b = commands
                .spawn(badge(fonts, "removed", kt::CHIP_BG, tok(ft_text_dim())))
                .id();
            commands.entity(header).add_child(b);
        }
    }
    header
}

fn meta_row(commands: &mut Commands, fonts: &KitFonts, item: &Item<'_>, stage: Stage) -> Entity {
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

    // With one flat list across projects, the project has to be on the card — it is no
    // longer implied by a section the card sits in.
    let mut line = item.project.to_owned();
    if let Some(topic) = item.topic() {
        line.push_str(&format!(" · {topic}"));
    }
    if let Some(task) = item.task
        && let Some(t) = task.last_active
    {
        line.push_str(&format!(" · {}", rel_time(t)));
    } else if let Some(t) = item.issue.and_then(|i| i.updated) {
        line.push_str(&format!(" · updated {}", rel_time(t)));
    }
    if let Some(author) = item.issue.and_then(|i| i.author.as_ref()) {
        line.push_str(&format!(" · @{author}"));
    }
    let line_id = commands
        .spawn(text(fonts, line, 12.0, tok(ft_text_dim())))
        .id();
    commands.entity(meta).add_child(line_id);

    // Repo chip only for a live worktree: a removed task has no git to read, so its chip
    // would sit at "…" forever.
    if let Some(task) = item.task
        && task.present
    {
        for repo in &task.repos {
            let chip = repo_chip(commands, fonts, repo);
            commands.entity(meta).add_child(chip);
        }
    }

    // The task/branch name — for an unstarted issue this is what **Start task** will
    // create, which is worth seeing before clicking; for a removed task it is what
    // **Restore** will re-create. A live worktree already prints it as the branch in the
    // chip just above, so printing it again only doubled the row.
    if !item.task.is_some_and(|t| t.present) {
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

    // Labels only on cards with no task half: once there is a worktree the row is already
    // carrying project, topic, time, branch and dirty count, and `bug`/`enhancement` chips
    // stop earning their space.
    if matches!(stage, Stage::Startable | Stage::Untagged)
        && let Some(issue) = item.issue
    {
        for label in issue.other_labels() {
            let chip = commands
                .spawn(badge(fonts, label.clone(), kt::CHIP_BG, tok(ft_text_dim())))
                .id();
            commands.entity(meta).add_child(chip);
        }
    }

    if let Some(task) = item.task
        && task.is_coordinator()
    {
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

    if let Some(err) = item.issue.and_then(|i| i.label_error.as_ref()) {
        let e = commands
            .spawn(text(
                fonts,
                format!("· backlog label not written: {err}"),
                12.0,
                tok(kt::STATUS_CLOSED),
            ))
            .id();
        commands.entity(meta).add_child(e);
    }

    if let Some(reason) = item
        .task
        .and_then(|t| t.block.as_ref())
        .and_then(|b| b.reason.as_ref())
    {
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
    key: &ItemKey,
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

    let mut buttons = Vec::new();
    if let Some(task) = item.task {
        // An orphan has no `task.json` to flip, and Adopt is the only thing to do with it.
        if task.adopted {
            buttons.push(button(
                commands,
                Variant::Normal,
                CardAction::ToggleDone,
                if task.done { "Reopen" } else { "Done" },
            ));
        }
    }
    if item.task.map_or(item.issue.is_some(), |t| t.adopted) {
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
    if let Some(task) = item.task {
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
    }
    if item.issue.is_some() {
        buttons.push(button(
            commands,
            Variant::Normal,
            CardAction::Browse,
            "Browser",
        ));
    }
    buttons.push(button(commands, Variant::Normal, CardAction::Hide, "Hide"));

    match stage {
        Stage::Done if item.task.is_some_and(Task::is_coordinator) => buttons.push(button(
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
        Stage::Startable => buttons.push(button(
            commands,
            Variant::Primary,
            CardAction::Start,
            "Start task",
        )),
        Stage::Untagged => {
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
                key: ItemKey::Task(task.id.clone()),
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
