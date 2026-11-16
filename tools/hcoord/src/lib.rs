//! The coordinator's tooling outside the Claude pane: a live view of its `tasks.json`,
//! blocking waits on a task's pipeline state and on its pull request, the seeding of a
//! task's brief from the goal file its list was split from, and the capture of the
//! screen a `hand` scenario names.

pub mod capture;
pub mod ci;
pub mod cli;
pub mod clock;
pub mod goal;
pub mod merged;
pub mod model;
pub mod output;
pub mod seed;
pub mod tui;
pub mod wait;
