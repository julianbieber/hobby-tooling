//! The shell actions: opening terminals, `hcdev` sessions, notes, and the browser — the one
//! place each command line is spelled.

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
    /// Topic (second level).
    pub topic: String,
    /// Task/branch name.
    pub task: String,
}

impl LaunchSpec {
    /// The spec for a task, from its path.
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

    fn restore_cmd(&self) -> String {
        format!(
            "hwt restore '{}' --no-cd | ignore; {}",
            self.task,
            self.hcdev_cmd()
        )
    }

    fn coordinator_cmd(&self) -> String {
        format!(
            "hcdev coordinator '{}' --project '{}'",
            self.task, self.project
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

/// Create a recorded task's worktree — re-created from its branch, or on a new branch from
/// the main checkout's HEAD when it has none — and open an `hcdev` session in it, in one
/// terminal. The task dir does not exist yet, so the terminal opens in the worktrees root;
/// `hcdev wt` resolves the restored path itself from the three components.
pub fn launch_restore(spec: &LaunchSpec) {
    launch_nu(
        &worktrees_root().to_string_lossy(),
        &spec.session_title(),
        &spec.restore_cmd(),
    );
}

/// Open (or re-attach) a coordinator session: the task list beside Claude, in the
/// project's main checkout, since a coordinator has no worktree of its own. `spec.task` is
/// the coordinator's blurb.
pub fn launch_coordinator(spec: &LaunchSpec) {
    let dir = projects_root().join(&spec.project);
    launch_nu(
        &dir.to_string_lossy(),
        &spec.session_title(),
        &spec.coordinator_cmd(),
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

    // Pins the exact nu command lines a card click runs, since none can be clicked in a test.
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
        // Restore is a two-step command: re-creating the worktree without opening the
        // session leaves the click with nothing running.
        assert_eq!(
            spec.restore_cmd(),
            "hwt restore 'gh-7-chunk-loading' --no-cd | ignore; \
             hcdev wt 'gh-7-chunk-loading' --project 'wusel' --topic 'perf'"
        );
        assert_eq!(spec.session_title(), "hcdev-wusel-perf-gh-7-chunk-loading");
    }

    // A coordinator is addressed by project and blurb only: the topic is the reserved
    // `coordinator`, which `hcdev coordinator` supplies itself.
    #[test]
    fn coordinator_command_takes_blurb_and_project() {
        let spec = LaunchSpec {
            project: "wusel".into(),
            topic: "coordinator".into(),
            task: "food-economy".into(),
        };
        assert_eq!(
            spec.coordinator_cmd(),
            "hcdev coordinator 'food-economy' --project 'wusel'"
        );
        assert_eq!(spec.session_title(), "hcdev-wusel-coordinator-food-economy");
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
