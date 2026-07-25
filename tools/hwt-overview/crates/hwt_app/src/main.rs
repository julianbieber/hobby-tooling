//! `hwt-overview` — a Bevy desktop dashboard of hobby git-worktree dev tasks.
//!
//! Adapted from the work `wt-overview`. Two model differences drive the changes: the
//! tree is three levels (`~/fun_worktrees/PROJECT/TOPIC/TASK`) with the task dir being
//! the worktree itself, and the notes repo has a single author — so the sidebar filters
//! by project instead of topic, and the owner-filter bar is gone entirely.

mod ui;
mod worktree;

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use feathers_kit::{FeathersPlugins, KitPlugin, Theme, create_dark_theme};

use crate::ui::UiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "hwt-overview".into(),
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
