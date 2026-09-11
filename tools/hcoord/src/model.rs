//! The two JSON files a coordinator reads: its own `tasks.json`, and each task's
//! `build-task.json` as `/task:auto` writes it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file name a coordinator keeps its list in, inside its notes directory.
pub const TASKS_FILE: &str = "tasks.json";

/// The file name a task's pipeline records its phase in, inside the task's notes directory.
pub const STATE_FILE: &str = "build-task.json";

/// Where a task is from the coordinator's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Not started.
    Todo,
    /// A session is working it unattended.
    Running,
    /// The session stopped at a gate and is waiting for a person.
    AwaitingHuman,
    /// The session finished; the PR, if any, is in `pr`.
    Done,
    /// Cannot proceed; `note` says why.
    Blocked,
}

impl Status {
    /// The one-character marker the TUI draws in front of the task.
    pub fn glyph(self) -> &'static str {
        match self {
            Status::Todo => "·",
            Status::Running => "▶",
            Status::AwaitingHuman => "!",
            Status::Done => "✓",
            Status::Blocked => "✗",
        }
    }
}

/// One entry of `tasks.json`. `task` and `title` are required; the rest is what the
/// coordinator has learned so far.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskEntry {
    /// Worktree and branch name — the third level under `~/fun_worktrees/<project>/<topic>/`.
    pub task: String,
    /// Topic directory the worktree lives under.
    #[serde(default)]
    pub topic: Option<String>,
    /// What a person gains once it lands.
    pub title: String,
    /// The observation that separates done from not done.
    #[serde(default)]
    pub done_when: Option<String>,
    /// Tasks (by `task`) whose branch this one builds on.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Current status; `todo` when absent.
    #[serde(default = "default_status")]
    pub status: Status,
    /// PR URL once one exists.
    #[serde(default)]
    pub pr: Option<String>,
    /// One line the coordinator wants shown next to the task.
    #[serde(default)]
    pub note: Option<String>,
    /// When `status` last changed, RFC 3339.
    #[serde(default)]
    pub updated: Option<String>,
}

fn default_status() -> Status {
    Status::Todo
}

/// The whole of `tasks.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coordinator {
    /// The project the tasks belong to.
    pub project: String,
    /// The coordinator's own name — the directory under `notes/<project>/coordinator/`.
    pub blurb: String,
    /// The description the coordinator was started with, verbatim.
    #[serde(default)]
    pub description: String,
    /// `YYYY-MM-DD` the coordinator was created.
    #[serde(default)]
    pub created: Option<String>,
    /// The list, in the order it is worked.
    #[serde(default)]
    pub tasks: Vec<TaskEntry>,
}

/// How many tasks are in each status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub todo: usize,
    pub running: usize,
    pub awaiting_human: usize,
    pub done: usize,
    pub blocked: usize,
}

impl Counts {
    /// One line naming only the non-zero counts, or `no tasks`.
    pub fn summary(&self) -> String {
        let parts: Vec<String> = [
            (self.running, "running"),
            (self.awaiting_human, "waiting for you"),
            (self.todo, "todo"),
            (self.done, "done"),
            (self.blocked, "blocked"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
        if parts.is_empty() {
            "no tasks".to_owned()
        } else {
            parts.join(" · ")
        }
    }
}

impl Coordinator {
    /// Path of the list inside a coordinator's notes directory.
    pub fn tasks_path(dir: &Path) -> PathBuf {
        dir.join(TASKS_FILE)
    }

    /// Read `tasks.json` from a coordinator's notes directory. A missing or malformed
    /// file is an error — the caller decides whether that means "empty" or "broken".
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = Self::tasks_path(dir);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Parse the file's contents.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// Tally the statuses.
    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for t in &self.tasks {
            match t.status {
                Status::Todo => c.todo += 1,
                Status::Running => c.running += 1,
                Status::AwaitingHuman => c.awaiting_human += 1,
                Status::Done => c.done += 1,
                Status::Blocked => c.blocked += 1,
            }
        }
        c
    }
}

/// The part of `build-task.json` the coordinator reacts to.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuildState {
    /// The pipeline's recorded phase.
    #[serde(default)]
    pub phase: String,
}

impl BuildState {
    /// Read a task's state file. `None` when the file does not exist yet — the session
    /// has not started, or has not reached its first transition.
    pub fn load(task_notes_dir: &Path) -> Result<Option<Self>, String> {
        let path = task_notes_dir.join(STATE_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read {}: {e}", path.display())),
        }
    }

    /// Whether the phase is one where the pipeline has stopped for a person: every
    /// `await-*` phase, plus the two waiting states `/task:build` names.
    pub fn needs_human(&self) -> bool {
        self.phase.starts_with("await-")
            || matches!(self.phase.as_str(), "context" | "assess-split")
    }

    /// Whether the pipeline has ended, one way or another.
    pub fn is_terminal(&self) -> bool {
        matches!(self.phase.as_str(), "done" | "handed-off")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins the defaults a hand-edited or freshly seeded list relies on: status and the
    // optional fields may all be absent.
    #[test]
    fn minimal_entry_parses_as_todo() {
        let c = Coordinator::parse(
            r#"{"project":"wusel","blurb":"food","tasks":[{"task":"food-fields","title":"Fields"}]}"#,
        )
        .unwrap();
        assert_eq!(c.tasks[0].status, Status::Todo);
        assert!(c.tasks[0].depends_on.is_empty());
        assert_eq!(c.counts().summary(), "1 todo");
    }

    // The status strings are the contract with the skill that writes them, so a rename
    // on either side must fail loudly here.
    #[test]
    fn status_strings_are_kebab_case() {
        let c = Coordinator::parse(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"a","title":"A","status":"awaiting-human"},
                {"task":"b","title":"B","status":"running"},
                {"task":"c","title":"C","status":"done"}]}"#,
        )
        .unwrap();
        assert_eq!(c.tasks[0].status, Status::AwaitingHuman);
        assert_eq!(
            c.counts(),
            Counts {
                running: 1,
                awaiting_human: 1,
                done: 1,
                ..Counts::default()
            }
        );
        assert_eq!(
            c.counts().summary(),
            "1 running · 1 waiting for you · 1 done"
        );
    }

    // `wait` returns on exactly these phases; a new gate in `/task:auto` that does not
    // start with `await-` would otherwise hang the coordinator.
    #[test]
    fn human_and_terminal_phases() {
        let phase = |p: &str| BuildState { phase: p.into() };
        assert!(phase("await-play").needs_human());
        assert!(phase("await-acceptance").needs_human());
        assert!(phase("context").needs_human());
        assert!(!phase("implement").needs_human());
        assert!(phase("done").is_terminal());
        assert!(!phase("done").needs_human());
    }
}
