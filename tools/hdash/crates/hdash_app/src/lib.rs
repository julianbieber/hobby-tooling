//! `hdash` — a terminal dashboard of hobby work: one card per task in the notes repo, whatever
//! stage it is at, from not started through its worktree, PRs and Claude session to done.

mod app;
mod facets;
mod focus;
mod hits;
mod model;
mod refresh;
mod session;
#[cfg(test)]
mod testkit;
mod view;
mod widgets;

pub use app::run;
