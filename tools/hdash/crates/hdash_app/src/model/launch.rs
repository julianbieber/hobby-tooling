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

    fn hcdev_cmd(&self) -> String {
        format!(
            "hcdev wt '{}' --project '{}' --topic '{}'",
            self.task, self.project, self.topic
        )
    }

    fn start_cmd(&self) -> String {
        format!(
            "hwt new '{}' -p '{}' -t '{}' --no-cd | ignore; {}",
            self.task,
            self.project,
            self.topic,
            self.hcdev_cmd()
        )
    }

    fn restore_cmd(&self) -> String {
        format!(
            "hwt restore '{}' --no-cd | ignore; {}",
            self.task,
            self.hcdev_cmd()
        )
    }

    fn session_title(&self) -> String {
        format!("hcdev-{}-{}-{}", self.project, self.topic, self.task)
    }
}

fn hold_on_error(cmd: &str) -> String {
    format!(
        "try {{ {cmd} }} catch {{|e| print $e.msg; print ''; print 'Failed. Press enter to close.'; input }}"
    )
}

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
        &hold_on_error(cmd),
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

/// Re-create a removed task's worktree and open an `hcdev` session in it, in one
/// terminal. The task dir does not exist yet, so the terminal opens in the worktrees
/// root; `hcdev wt` resolves the restored path itself from the three components.
pub fn launch_restore(spec: &LaunchSpec) {
    launch_nu(
        &worktrees_root().to_string_lossy(),
        &spec.session_title(),
        &spec.restore_cmd(),
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
        // Restore is a two-step command for the same reason Start is: re-creating the
        // worktree without opening the session leaves the click with nothing running.
        assert_eq!(
            spec.restore_cmd(),
            "hwt restore 'gh-7-chunk-loading' --no-cd | ignore; \
             hcdev wt 'gh-7-chunk-loading' --project 'wusel' --topic 'perf'"
        );
        assert_eq!(spec.session_title(), "hcdev-wusel-perf-gh-7-chunk-loading");
    }

    // Pins the wrapper every nu launch goes through: without the `catch`, a failing
    // command closes the terminal it reported into and the message is never seen.
    #[test]
    fn failures_hold_the_terminal_open() {
        let held = hold_on_error("hwt restore 'x'");
        assert!(held.starts_with("try { hwt restore 'x' } catch {|e|"));
        assert!(held.contains("print $e.msg"));
        assert!(held.trim_end().ends_with("input }"));
    }

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
