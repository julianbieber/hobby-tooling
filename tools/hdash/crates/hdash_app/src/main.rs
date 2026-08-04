//! `hdash` — a Bevy desktop dashboard of hobby work items.
//!
//! One card per piece of work, whatever stage it is at: a GitHub issue nobody has started,
//! the worktree it became, the PRs that came out of it, and the day it was marked done.
//! The two dashboards this replaces — `hwt-overview` (worktrees that exist) and
//! `hgh-issues` (issues that have none yet) — were the same app over two halves of one
//! list, and each had to reason about the other's half to draw its own cards.
//!
//! The bridge between the halves is the task name: an issue's worktree is called
//! `gh-<number>-<slug>`, so matching on the number joins the two. Topic — the middle level
//! of `~/fun_worktrees/PROJECT/TOPIC/TASK` — is the one component an issue cannot supply
//! by itself, which is why it carries a `topic:<name>` label.

mod model;
mod ui;

use bevy::prelude::*;
use bevy::winit::WinitSettings;
use feathers_kit::{FeathersPlugins, KitPlugin, Theme, create_dark_theme};

use crate::ui::UiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "hdash".into(),
                // Wider than the 1100 the two pre-merge dashboards used: the header carries
                // both content toggles now, and the cards carry a project as well as a topic.
                resolution: bevy::window::WindowResolution::new(1240, 800),
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
