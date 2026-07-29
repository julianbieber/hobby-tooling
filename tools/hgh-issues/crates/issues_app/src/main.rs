//! `hgh-issues` — a Bevy desktop dashboard of the open GitHub issues of every hobby
//! project, with one click from an issue to a running `hcdev` session.
//!
//! The sibling of `hwt-overview`: that one starts from the worktrees that exist, this one
//! from the issues that don't have one yet. It reuses `hwt-overview`'s `feathers_kit` and
//! `hwt_core` crates directly rather than copying them.
//!
//! The bridge between an issue and a worktree path is a **`topic:<name>` label**: project
//! comes from the repo the issue lives in, task from `gh-<number>-<slug>`, and the topic —
//! the one component nothing else supplies — comes from the tag. `/gh-issue-create` writes
//! it.

mod issues;
mod ui;

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use feathers_kit::{FeathersPlugins, KitPlugin, Theme, create_dark_theme};

use crate::ui::UiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "hgh-issues".into(),
                resolution: bevy::window::WindowResolution::new(1100, 760),
                ..default()
            }),
            ..default()
        }))
        // Render reactively (only on input/events) like a normal desktop app rather than a
        // continuous game loop — keeps idle CPU/GPU usage near zero.
        .insert_resource(WinitSettings::desktop_app())
        .insert_resource(Theme(create_dark_theme()))
        .add_plugins(FeathersPlugins)
        .add_plugins(KitPlugin)
        .add_plugins(UiPlugin)
        .run();
}
