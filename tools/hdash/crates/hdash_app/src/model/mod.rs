//! Domain layer: the two halves of the model, their join, and the shell actions.
//! No Bevy UI here.
//!
//! - [`task`] — the filesystem half: worktrees ∪ notes-with-`task.json`, git status, PRs
//! - [`issue`] — the GitHub half: `gh issue list` per project, the `topic:` convention
//! - [`join`] — the union: one [`join::Item`] per piece of work, at some [`join::Stage`]
//! - [`launch`] — terminals, `hcdev` sessions, notes, browser

pub mod issue;
pub mod join;
pub mod launch;
pub mod task;
