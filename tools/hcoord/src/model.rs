//! The two JSON files a coordinator reads: its own `tasks.json`, and each task's
//! `build-task.json` as `/task:auto` writes it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file name a coordinator keeps its list in, inside its notes directory.
pub const TASKS_FILE: &str = "tasks.json";

/// The file name a task's pipeline records its phase in, inside the task's notes directory.
pub const STATE_FILE: &str = "build-task.json";

/// The file name the `track-pr` hook records a task's opened PRs in, inside the task's
/// notes directory.
pub const PRS_FILE: &str = "prs.json";

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
    /// The session finished and its PR is open; the list holds until that PR is merged.
    AwaitingMerge,
    /// The PR is merged, or the task finished without one.
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
            Status::AwaitingMerge => "⇡",
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

/// Who merges a task's pull request once its checks are green.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeBy {
    /// I merge; the coordinator reports the PR and holds the list until I have.
    #[default]
    Mine,
    /// The coordinator merges as soon as the checks pass and GitHub reports no conflict.
    Coordinator,
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
    /// Who merges each task's PR; `mine` when absent.
    #[serde(default)]
    pub merge: MergeBy,
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
    pub awaiting_merge: usize,
    pub done: usize,
    pub blocked: usize,
}

impl Counts {
    /// One line naming only the non-zero counts, or `no tasks`.
    pub fn summary(&self) -> String {
        let parts: Vec<String> = [
            (self.running, "running"),
            (self.awaiting_human, "waiting for you"),
            (self.awaiting_merge, "waiting on a merge"),
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

    /// The notes directory of one of this coordinator's tasks, given the coordinator's
    /// own notes directory. A task without a `topic` sits under the blurb, which is
    /// where `/task:coordinate` puts the tasks it creates.
    pub fn task_notes_dir(&self, dir: &Path, entry: &TaskEntry) -> PathBuf {
        let project = dir.parent().and_then(Path::parent).unwrap_or(dir);
        project
            .join(entry.topic.as_deref().unwrap_or(&self.blurb))
            .join(&entry.task)
    }

    /// Every PR URL known for a task, most authoritative first and without repeats:
    /// the task's `prs.json`, then the `pr` its `build-task.json` carries, then what
    /// this list has recorded. The first two exist the moment the PR opens; the last
    /// only once the coordinator's wait has returned, so a list read alone lags.
    pub fn pr_urls(&self, dir: &Path, entry: &TaskEntry) -> Vec<String> {
        let notes = self.task_notes_dir(dir, entry);
        let mut urls = read_prs(&notes);
        let recorded = BuildState::load(&notes)
            .ok()
            .flatten()
            .and_then(|s| s.pr)
            .into_iter()
            .chain(entry.pr.clone());
        for url in recorded {
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        urls
    }

    /// The files whose change should redraw a view of this list: `tasks.json` and each
    /// task's PR and state files.
    pub fn watched_files(&self, dir: &Path) -> Vec<PathBuf> {
        let mut files = vec![Self::tasks_path(dir)];
        for t in &self.tasks {
            let notes = self.task_notes_dir(dir, t);
            files.push(notes.join(PRS_FILE));
            files.push(notes.join(STATE_FILE));
        }
        files
    }

    /// Tally the statuses.
    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for t in &self.tasks {
            match t.status {
                Status::Todo => c.todo += 1,
                Status::Running => c.running += 1,
                Status::AwaitingHuman => c.awaiting_human += 1,
                Status::AwaitingMerge => c.awaiting_merge += 1,
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
    /// The PR URL, once the pipeline has opened one.
    #[serde(default)]
    pub pr: Option<String>,
}

#[derive(Deserialize)]
struct PrRecord {
    url: String,
}

fn read_prs(task_notes_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(task_notes_dir.join(PRS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<PrRecord>>(&text).ok())
        .map(|records| records.into_iter().map(|r| r.url).collect())
        .unwrap_or_default()
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
                {"task":"c","title":"C","status":"done"},
                {"task":"d","title":"D","status":"awaiting-merge"}]}"#,
        )
        .unwrap();
        assert_eq!(c.tasks[0].status, Status::AwaitingHuman);
        assert_eq!(c.tasks[3].status, Status::AwaitingMerge);
        assert_eq!(
            c.counts(),
            Counts {
                running: 1,
                awaiting_human: 1,
                awaiting_merge: 1,
                done: 1,
                ..Counts::default()
            }
        );
        assert_eq!(
            c.counts().summary(),
            "1 running · 1 waiting for you · 1 waiting on a merge · 1 done"
        );
    }

    // Merging is mine unless the file says otherwise, so a list written before the field
    // existed must not start merging on my behalf.
    #[test]
    fn merge_is_mine_unless_the_list_says_coordinator() {
        let mine = Coordinator::parse(r#"{"project":"p","blurb":"b","tasks":[]}"#).unwrap();
        assert_eq!(mine.merge, MergeBy::Mine);
        let theirs =
            Coordinator::parse(r#"{"project":"p","blurb":"b","merge":"coordinator","tasks":[]}"#)
                .unwrap();
        assert_eq!(theirs.merge, MergeBy::Coordinator);
    }

    // The view shows a PR as soon as the task's own notes record it, and shows it once
    // even when the coordinator has since copied it into the list.
    #[test]
    fn pr_urls_come_from_the_task_notes_first_and_dedupe() {
        let root = std::env::temp_dir().join(format!("hcoord-prs-{}", std::process::id()));
        let coordinator = root.join("p/coordinator/b");
        let task = root.join("p/b/b-one");
        std::fs::create_dir_all(&coordinator).unwrap();
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(
            task.join(PRS_FILE),
            r#"[{"url":"https://github.com/o/r/pull/2","repo":"o/r","title":"t","opened":"d"}]"#,
        )
        .unwrap();
        std::fs::write(
            task.join(STATE_FILE),
            r#"{"phase":"done","pr":"https://github.com/o/r/pull/2"}"#,
        )
        .unwrap();
        let c = Coordinator::parse(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"b-one","title":"One","pr":"https://github.com/o/r/pull/1"},
                {"task":"b-two","topic":"other","title":"Two"}]}"#,
        )
        .unwrap();
        assert_eq!(
            c.pr_urls(&coordinator, &c.tasks[0]),
            vec![
                "https://github.com/o/r/pull/2".to_owned(),
                "https://github.com/o/r/pull/1".to_owned()
            ]
        );
        assert_eq!(
            c.task_notes_dir(&coordinator, &c.tasks[1]),
            root.join("p/other/b-two")
        );
        assert!(c.pr_urls(&coordinator, &c.tasks[1]).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    // `wait` returns on exactly these phases; a new gate in `/task:auto` that does not
    // start with `await-` would otherwise hang the coordinator.
    #[test]
    fn human_and_terminal_phases() {
        let phase = |p: &str| BuildState {
            phase: p.into(),
            pr: None,
        };
        assert!(phase("await-play").needs_human());
        assert!(phase("await-acceptance").needs_human());
        assert!(phase("context").needs_human());
        assert!(!phase("implement").needs_human());
        assert!(phase("done").is_terminal());
        assert!(!phase("done").needs_human());
    }
}
