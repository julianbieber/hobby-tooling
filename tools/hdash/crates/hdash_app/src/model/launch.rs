//! The shell actions: opening terminals, `hcdev` sessions, notes, and the browser — the one
//! place each command line is spelled.

use std::path::Path;

use hwt_core::{Roots, spawn_detached, spawn_detached_with};

use super::task::{Task, TaskId, notes_path, worktrees_root};

pub use hwt_core::open_url;

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

    fn env(&self) -> [(&'static str, &str); 3] {
        [
            ("HDASH_PROJECT", &self.project),
            ("HDASH_TOPIC", &self.topic),
            ("HDASH_TASK", &self.task),
        ]
    }

    fn session_title(&self) -> String {
        format!("hcdev-{}-{}-{}", self.project, self.topic, self.task)
    }
}

/// Which panes a new `hcdev` session's tabs get: Helix beside Claude, or Claude beside the
/// `htab` overview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Editor,
    Claude,
}

impl Mode {
    fn script(self, script: &str) -> String {
        match self {
            Self::Editor => script.to_owned(),
            Self::Claude => format!("{script} --claude"),
        }
    }
}

const HCDEV_SCRIPT: &str =
    "hcdev wt $env.HDASH_TASK --project $env.HDASH_PROJECT --topic $env.HDASH_TOPIC";

const RESTORE_SCRIPT: &str = "hwt restore $env.HDASH_TASK --no-cd | ignore; \
     hcdev wt $env.HDASH_TASK --project $env.HDASH_PROJECT --topic $env.HDASH_TOPIC";

const TAB_SCRIPT: &str = "hwt aspect start $env.HDASH_TAB --no-tab --task $env.HDASH_TASK \
     --project $env.HDASH_PROJECT; hcdev tab $env.HDASH_TAB --task $env.HDASH_TASK \
     --project $env.HDASH_PROJECT --topic $env.HDASH_TOPIC";

const TAB_ATTACH_SCRIPT: &str = "hwt aspect start $env.HDASH_TAB --no-tab --task $env.HDASH_TASK \
     --project $env.HDASH_PROJECT; hcdev tab $env.HDASH_TAB --task $env.HDASH_TASK \
     --project $env.HDASH_PROJECT --topic $env.HDASH_TOPIC --attach";

fn hold_on_error(cmd: &str) -> String {
    format!(
        "try {{ {cmd} }} catch {{|e| print $e.msg; print ''; print 'Failed. Press enter to close.'; input }}"
    )
}

fn launch_nu(dir: &str, title: &str, cmd: &str, spec: &LaunchSpec) {
    launch_nu_with(dir, title, cmd, &spec.env());
}

fn launch_nu_with(dir: &str, title: &str, cmd: &str, envs: &[(&str, &str)]) {
    spawn_detached_with(
        &[
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
        ],
        envs,
    );
}

/// Open an `hcdev` session (Claude on the hobby account) in an existing worktree, in `mode`
/// when it is not already running.
pub fn launch_hcdev(dir: &Path, spec: &LaunchSpec, mode: Mode) {
    launch_nu(
        &dir.to_string_lossy(),
        &spec.session_title(),
        &mode.script(HCDEV_SCRIPT),
        spec,
    );
}

/// Create a recorded task's worktree — re-created from its branch, or on a new branch from
/// the main checkout's HEAD when it has none — and open an `hcdev` session in it, in one
/// terminal. The task dir does not exist yet, so the terminal opens in the worktrees root;
/// `hcdev wt` resolves the restored path itself from the three components.
pub fn launch_restore(spec: &LaunchSpec, mode: Mode) {
    launch_nu(
        &worktrees_root().to_string_lossy(),
        &spec.session_title(),
        &mode.script(RESTORE_SCRIPT),
        spec,
    );
}

/// Open aspect `name` of the task as a tab in the task's running session, starting the aspect
/// first when it is not checked out, with no terminal of its own; the session's window, if any,
/// is the caller's to focus.
pub fn open_tab(spec: &LaunchSpec, name: &str) {
    let mut envs = spec.env().to_vec();
    envs.push(("HDASH_TAB", name));
    spawn_detached_with(&["-f", "nu", "--login", "-c", TAB_SCRIPT], &envs);
}

/// Open aspect `name` of the task as a tab and attach to the task's session in a new terminal,
/// starting the aspect first when it is not checked out, and the session when it does not run.
pub fn launch_tab(dir: &Path, spec: &LaunchSpec, name: &str) {
    let mut envs = spec.env().to_vec();
    envs.push(("HDASH_TAB", name));
    launch_nu_with(
        &dir.to_string_lossy(),
        &spec.session_title(),
        TAB_ATTACH_SCRIPT,
        &envs,
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
    let dir = notes_path(&Roots::of_user(), &task.id)
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
    fn launch_scripts_read_the_task_from_the_environment() {
        let spec = LaunchSpec {
            project: "wusel".into(),
            topic: "perf".into(),
            task: "it's-'; rm -rf ~".into(),
        };
        for script in [HCDEV_SCRIPT, RESTORE_SCRIPT, TAB_SCRIPT, TAB_ATTACH_SCRIPT] {
            assert!(!script.contains('\''), "no literal in {script}");
            assert!(script.contains("$env.HDASH_TASK"));
        }
        assert!(
            RESTORE_SCRIPT.starts_with("hwt restore $env.HDASH_TASK --no-cd | ignore; hcdev wt")
        );
        assert_eq!(
            spec.env(),
            [
                ("HDASH_PROJECT", "wusel"),
                ("HDASH_TOPIC", "perf"),
                ("HDASH_TASK", "it's-'; rm -rf ~"),
            ]
        );
        assert_eq!(spec.session_title(), "hcdev-wusel-perf-it's-'; rm -rf ~");
    }

    #[test]
    fn failures_hold_the_terminal_open() {
        let held = hold_on_error("hwt restore 'x'");
        assert!(held.starts_with("try { hwt restore 'x' } catch {|e|"));
        assert!(held.contains("print $e.msg"));
        assert!(held.trim_end().ends_with("input }"));
    }

    #[test]
    fn claude_mode_ends_the_hcdev_command_with_its_flag() {
        assert_eq!(Mode::Editor.script(HCDEV_SCRIPT), HCDEV_SCRIPT);
        assert!(
            Mode::Claude
                .script(RESTORE_SCRIPT)
                .ends_with("hcdev wt $env.HDASH_TASK --project $env.HDASH_PROJECT --topic $env.HDASH_TOPIC --claude")
        );
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
