//! The shell actions: opening terminals, `hcdev` sessions, notes, and the browser.
//!
//! Both pre-merge dashboards spelled `hcdev wt …` and the `hcdev-<p>-<t>-<task>` window
//! title for themselves, and `hwt-overview` open-coded the alacritty invocation four
//! times. Everything funnels through [`launch_nu`] and [`LaunchSpec`] here, so the
//! command line exists once.

use std::path::Path;

use hwt_core::spawn_detached;

use super::task::{Task, TaskId, notes_path, worktrees_root};

pub use hwt_core::open_url;
pub use hwt_core::projects_root;

/// The three path components a launch needs: `~/fun_worktrees/<project>/<topic>/<task>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSpec {
    /// Project (and the repo under `~/fun_repos` the worktree derives from).
    pub project: String,
    /// Topic — from an existing worktree's path, or from the issue's `topic:` label.
    pub topic: String,
    /// Task/branch name.
    pub task: String,
}

impl LaunchSpec {
    /// The spec for an existing task. The worktree path is the authority, not an issue's
    /// `topic:` label: the label can have been changed since the worktree was made, and
    /// it is the path that `hcdev wt` has to find.
    pub fn from_task(id: &TaskId) -> Self {
        Self {
            project: id.project.clone(),
            topic: id.topic.clone(),
            task: id.name.clone(),
        }
    }

    /// `hcdev wt <task> --project <p> --topic <t>` — project and topic are passed
    /// explicitly because the same task name can exist under two projects and `hcdev wt`
    /// errors on an ambiguous name rather than guessing.
    fn hcdev_cmd(&self) -> String {
        format!(
            "hcdev wt '{}' --project '{}' --topic '{}'",
            self.task, self.project, self.topic
        )
    }

    /// `hwt new …` then the `hcdev wt`, as one nushell command line.
    ///
    /// `| ignore` discards the path `hwt new` returns: a bare value in a non-final
    /// `;`-separated nushell statement is not something to rely on.
    fn start_cmd(&self) -> String {
        format!(
            "hwt new '{}' -p '{}' -t '{}' --no-cd | ignore; {}",
            self.task,
            self.project,
            self.topic,
            self.hcdev_cmd()
        )
    }

    /// The terminal title for this task's session, spelled the way `hcdev` spells a
    /// session name. Cosmetic only — `hcdev` shortens the name it hands zellij when the
    /// three components overflow the socket path, and a window title has no such limit.
    fn session_title(&self) -> String {
        format!("hcdev-{}-{}-{}", self.project, self.topic, self.task)
    }
}

/// Run a nushell command in a detached alacritty. `--login` is what loads `hwt.nu` and
/// `hcdev.nu`, so both are only reachable this way.
fn launch_nu(dir: &str, title: &str, cmd: &str) {
    spawn_detached(&[
        "-f",
        "alacritty",
        "--working-directory",
        dir,
        "--title",
        title,
        "-e",
        "nu",
        "--login",
        "-c",
        cmd,
    ]);
}

/// Open an `hcdev` session (Zellij hobby-dev layout, Claude on the hobby account) in an
/// existing worktree.
pub fn launch_hcdev(dir: &Path, spec: &LaunchSpec) {
    launch_nu(
        &dir.to_string_lossy(),
        &spec.session_title(),
        &spec.hcdev_cmd(),
    );
}

/// Create the worktree for an issue and open an `hcdev` session in it, in one terminal.
///
/// Runs from the project's main checkout, because the worktree does not exist yet and
/// `hwt new` derives the project from either `--project` or the current repo. `hwt new`
/// is idempotent (an existing worktree is reported and reused), so a double click costs
/// a second session and nothing worse.
pub fn launch_start(spec: &LaunchSpec) {
    let dir = projects_root().join(&spec.project);
    launch_nu(
        &dir.to_string_lossy(),
        &spec.session_title(),
        &spec.start_cmd(),
    );
}

/// Re-create a removed task's worktree by running `hwt restore` in a new terminal. The
/// task dir does not exist yet, so the terminal opens in the worktrees root.
pub fn launch_restore(id: &TaskId) {
    launch_nu(
        &worktrees_root().to_string_lossy(),
        &format!("restore-{}", id.name),
        &format!("hwt restore '{}'", id.name),
    );
}

/// Open a plain terminal in a directory.
pub fn launch_terminal(dir: &Path, title: &str) {
    spawn_detached(&[
        "-f",
        "alacritty",
        "--working-directory",
        &dir.to_string_lossy(),
        "--title",
        title,
    ]);
}

/// Open a terminal in the task's notes directory (falls back to the task directory).
pub fn open_notes(task: &Task) {
    let dir = notes_path(&task.id)
        .map(|p| {
            if p.is_dir() {
                p
            } else {
                p.parent().map(Path::to_path_buf).unwrap_or(p)
            }
        })
        .unwrap_or_else(|| task.dir.clone());
    launch_terminal(&dir, "notes");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The launch commands quote every interpolated component and pass project/topic
    /// explicitly, so an ambiguous task name cannot make `hcdev` guess.
    #[test]
    fn launch_commands_are_explicit() {
        let spec = LaunchSpec {
            project: "wusel".into(),
            topic: "perf".into(),
            task: "gh-7-chunk-loading".into(),
        };
        assert_eq!(
            spec.hcdev_cmd(),
            "hcdev wt 'gh-7-chunk-loading' --project 'wusel' --topic 'perf'"
        );
        // The one command with a real side effect, and the one that cannot be clicked in a
        // test: `hwt new` creates the worktree, `| ignore` swallows the path it returns, and
        // the session opens in it.
        assert_eq!(
            spec.start_cmd(),
            "hwt new 'gh-7-chunk-loading' -p 'wusel' -t 'perf' --no-cd | ignore; \
             hcdev wt 'gh-7-chunk-loading' --project 'wusel' --topic 'perf'"
        );
        assert_eq!(spec.session_title(), "hcdev-wusel-perf-gh-7-chunk-loading");
    }

    /// A task's spec comes from its path components, so a relabelled issue cannot send
    /// `hcdev wt` to a topic directory that does not exist.
    #[test]
    fn task_specs_come_from_the_worktree_path() {
        let id = TaskId {
            project: "wusel".into(),
            topic: "perf".into(),
            name: "issue-7_chunk_loading".into(),
        };
        assert_eq!(
            LaunchSpec::from_task(&id),
            LaunchSpec {
                project: "wusel".into(),
                topic: "perf".into(),
                task: "issue-7_chunk_loading".into(),
            }
        );
    }
}
