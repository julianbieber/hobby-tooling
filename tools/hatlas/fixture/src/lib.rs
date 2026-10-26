use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;

/// Wires the fixture's resource up.
///
/// See [`Score`] for what it keeps, and [the book](https://example.com/book).
/// [bad](javascript:alert(1)) <script>alert(1)</script>
pub struct ScorePlugin;

impl Plugin for ScorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>();
    }
}

struct HiddenPlugin;

impl Plugin for HiddenPlugin {
    fn build(&self, _app: &mut App) {}
}

/// How far the player has got.
#[derive(Component)]
pub struct Progress(pub u32);

/// The running total.
#[derive(Resource, Default)]
pub struct Score(pub u32);

#[derive(Event)]
pub struct Scored;

mod hidden {
    use bevy_ecs::prelude::*;

    /// Kept inside its own module.
    #[derive(Resource, Default)]
    pub(crate) struct Ledger(pub u32);
}

pub mod fake {
    pub trait Component {}
    pub struct NotBevy;
    impl Component for NotBevy {}
}
