//! `hdash` — a Bevy desktop dashboard of hobby work: one card per task in the notes repo,
//! whatever stage it is at, from not started through its worktree and PRs to done.

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
