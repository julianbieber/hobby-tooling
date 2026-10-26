use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::SystemParam;

macro_rules! score_macro {
    () => {
        macro_made
    };
}

/// Wires the fixture's resource up.
///
/// See [`Score`] for what it keeps, and [the book](https://example.com/book).
/// [bad](javascript:alert(1)) <script>alert(1)</script>
pub struct ScorePlugin;

impl Plugin for ScorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>();
        app.configure_sets(
            Update,
            (ScoreSystems::Count, ScoreSystems::Report)
                .chain()
                .run_if(resource_exists::<Score>),
        );
        app.add_systems(
            Update,
            (count_progress, report_score)
                .chain()
                .in_set(ScoreSystems::Report),
        );
        app.add_systems(Update, |score: Res<Score>| {
            let _ = score.0;
        });
        app.add_systems(Update, score_macro!());
        app.add_systems(Update, (announce, hear, fire));
        app.add_observer(on_scored);
        app.add_plugins(extra_plugin);
    }
}

/// Ordering of the score systems.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScoreSystems {
    Count,
    Report,
}

/// Marks progress that is not counted.
#[derive(Component)]
pub struct Marker;

/// Reads the progress of everything.
#[derive(SystemParam)]
pub struct Tally<'w, 's> {
    scores: Res<'w, Score>,
    progress: Query<'w, 's, &'static Progress>,
}

/// Adds up the progress.
pub fn count_progress(
    progress: Query<(&Progress, Option<&Name>), (With<Progress>, Without<Marker>)>,
    mut score: ResMut<Score>,
) {
    score.0 = progress.iter().map(|(p, _)| p.0).sum();
}

/// Reports the score.
pub fn report_score(latest: Single<&Progress, Changed<Progress>>, maybe: Option<Res<Score>>, tally: Tally) {
    let _ = (latest.0, maybe, tally.scores.0, tally.progress.iter().count());
}

fn mixed(params: ParamSet<(Query<&Progress>, Res<Score>)>) {
    let _ = params;
}

fn macro_made() {}

fn on_scored(event: On<Scored>) {
    let _ = event;
}

/// Counted once per report.
#[derive(Message)]
pub struct Tallied(pub u32);

/// Announces the tally.
pub fn announce(mut out: MessageWriter<Tallied>) {
    out.write(Tallied(1));
}

/// Hears the tally.
pub fn hear(mut input: MessageReader<Tallied>) {
    let _ = input.read().count();
}

fn fire(mut commands: Commands) {
    commands.trigger(Scored);
}

fn heard_elsewhere(_: On<Scored>) {}

/// A plugin written as a function.
pub fn extra_plugin(app: &mut App) {
    app.add_systems(Update, mixed);
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

/// Calls into the dependency that cannot be documented.
pub fn answer() -> u32 {
    hatlas_brokendoc::answer()
}
