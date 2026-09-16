//! The two JSON files a coordinator reads: its own `tasks.json`, and each task's
//! `build-task.json` as `/task:auto` writes it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::clock;

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

    /// One line per non-terminal task that has a readable state file, in list order,
    /// naming its phase and how long it has sat there — what `status` prints under
    /// the counts so telling a working session from a stalled one does not need
    /// sampling `/proc` or reading a pane that cannot be read.
    pub fn phase_lines(&self, dir: &Path) -> Vec<String> {
        self.tasks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let notes = self.task_notes_dir(dir, t);
                let state = BuildState::load(&notes).ok().flatten()?;
                if state.is_terminal() || state.phase.is_empty() {
                    return None;
                }
                Some(format!("  {}. {} — {}", i + 1, t.task, state.phase_summary()))
            })
            .collect()
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

/// One acceptance command an unattended run executed, and how it went.
#[derive(Clone, Debug, Deserialize)]
pub struct AcceptanceCheck {
    /// What the check is called in the plan.
    pub name: String,
    /// The command as it was run, when the pipeline recorded one.
    #[serde(default)]
    pub command: Option<String>,
    /// The command's exit status, when the pipeline recorded one.
    #[serde(default)]
    pub exit: Option<i32>,
    /// Whether this check passed. Absent in the file means it did not.
    #[serde(default)]
    pub pass: bool,
}

/// What an unattended `/task:auto` run recorded about its own acceptance. Absent from
/// the state file of an interactive run, which stops at the play gate instead.
#[derive(Clone, Debug, Deserialize)]
pub struct AcceptanceResult {
    /// Whether every check passed.
    #[serde(default)]
    pub passed: bool,
    /// One entry per check, in the order the plan listed them.
    #[serde(default)]
    pub checks: Vec<AcceptanceCheck>,
}

impl AcceptanceResult {
    /// The names of the checks that did not pass, in plan order.
    pub fn failed(&self) -> Vec<&str> {
        self.checks
            .iter()
            .filter(|c| !c.pass)
            .map(|c| c.name.as_str())
            .collect()
    }

    /// One line for the narrow coordinator pane: the tally, a pass or fail glyph, and the
    /// first failing check's name when there is one. `passed` decides the glyph, so a
    /// result whose flag disagrees with its checks reads as its flag.
    pub fn summary(&self) -> String {
        let total = self.checks.len();
        let passed = self.checks.iter().filter(|c| c.pass).count();
        let tally = if total == 0 {
            String::new()
        } else {
            format!("{passed}/{total} ")
        };
        if self.passed {
            return format!("acceptance {tally}\u{2713}");
        }
        match self.failed().first() {
            Some(name) => format!("acceptance {tally}\u{2717} {name}"),
            None => format!("acceptance {tally}\u{2717}"),
        }
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
    /// The acceptance run of an unattended pipeline; `None` until one has happened.
    /// Named apart from the state file's `acceptance`, which is the observation itself.
    #[serde(default)]
    pub acceptance_result: Option<AcceptanceResult>,
    /// When this file was last written, RFC 3339. Absent from a file written before
    /// this field existed.
    #[serde(default)]
    pub written: Option<String>,
    /// When `phase` last changed, RFC 3339. Carried forward unchanged by a write that
    /// leaves the phase alone. Absent from a file written before this field existed.
    #[serde(default)]
    pub phase_since: Option<String>,
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
    /// `await-*` phase — `await-play` interactively, `await-fix` when an unattended run
    /// could not make its acceptance pass — plus the two waiting states `/task:build`
    /// names.
    pub fn needs_human(&self) -> bool {
        self.phase.starts_with("await-")
            || matches!(self.phase.as_str(), "context" | "assess-split")
    }

    /// Whether the pipeline has ended, one way or another.
    pub fn is_terminal(&self) -> bool {
        matches!(self.phase.as_str(), "done" | "handed-off")
    }

    /// Seconds since `phase_since`, when the file recorded one that parses. Can come
    /// back negative when the writing machine's clock runs ahead of this one's.
    pub fn unchanged_for(&self) -> Option<i64> {
        self.phase_since.as_deref().and_then(clock::seconds_since)
    }

    /// Seconds since `written`, when the file recorded one that parses.
    pub fn written_ago(&self) -> Option<i64> {
        self.written.as_deref().and_then(clock::seconds_since)
    }

    /// One line for a person or a script: `"<phase>, unchanged for <human>"`, with a
    /// `", last write <human> ago"` clause added only when `written` parses and
    /// differs from `phase_since`, or `"<phase>, no time recorded"` when the file
    /// carries neither.
    pub fn phase_summary(&self) -> String {
        let Some(unchanged) = self.unchanged_for() else {
            return format!("{}, no time recorded", self.phase);
        };
        let mut summary = format!("{}, unchanged for {}", self.phase, clock::human(unchanged));
        if self.written != self.phase_since
            && let Some(written_ago) = self.written_ago()
        {
            summary.push_str(&format!(", last write {} ago", clock::human(written_ago)));
        }
        summary
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
            ..Default::default()
        };
        assert!(phase("await-play").needs_human());
        assert!(phase("await-acceptance").needs_human());
        assert!(phase("await-fix").needs_human());
        assert!(phase("context").needs_human());
        assert!(!phase("implement").needs_human());
        assert!(!phase("verify").needs_human());
        assert!(phase("done").is_terminal());
        assert!(!phase("done").needs_human());
    }

    // An unattended run's evidence has to survive the trip through the file, because the
    // coordinator sees it and never sees the pane that produced it.
    #[test]
    fn an_acceptance_result_parses_and_summarises() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"done","acceptance":"place a field, watch it fill",
                "acceptance_result":{"passed":true,"checks":[
                {"name":"tests","command":"cargo test","exit":0,"pass":true},
                {"name":"cli","command":"hcoord wait .","exit":0,"pass":true}]}}"#,
        )
        .unwrap();
        let result = state.acceptance_result.unwrap();
        assert!(result.failed().is_empty());
        assert_eq!(result.summary(), "acceptance 2/2 \u{2713}");

        let failing: BuildState = serde_json::from_str(
            r#"{"phase":"await-fix","acceptance_result":{"checks":[
                {"name":"tests","exit":0,"pass":true},
                {"name":"cli","exit":1}]}}"#,
        )
        .unwrap();
        let result = failing.acceptance_result.unwrap();
        assert_eq!(result.failed(), vec!["cli"]);
        assert_eq!(result.summary(), "acceptance 1/2 \u{2717} cli");
    }

    // The interactive path stops at the play gate and writes no result block, so a state
    // file without one must not read as a failure — and `acceptance` there is the
    // observation as prose, which must not be mistaken for the result.
    #[test]
    fn a_state_file_without_a_result_has_none() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"await-play","pr":null,"acceptance":"place a field"}"#,
        )
        .unwrap();
        assert!(state.acceptance_result.is_none());
    }

    // A write that changed the phase leaves `written` and `phase_since` equal, so the
    // summary must not grow a redundant "last write" clause pointing at the same instant.
    #[test]
    fn phase_summary_with_both_timestamps_equal_has_no_last_write_clause() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"implement","written":"2020-01-01T00:00:00Z",
                "phase_since":"2020-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert!(state.unchanged_for().unwrap() > 0);
        assert_eq!(state.written_ago(), state.unchanged_for());
        assert!(!state.phase_summary().contains("last write"));
    }

    // A write that left the phase alone moves `written` forward but not `phase_since` —
    // the one case the two timestamps exist to tell apart — so the summary must show both.
    #[test]
    fn phase_summary_with_differing_timestamps_names_both() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"implement","written":"2020-01-01T00:04:00Z",
                "phase_since":"2020-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        let summary = state.phase_summary();
        assert!(summary.starts_with("implement, unchanged for"));
        assert!(summary.contains("last write"));
    }

    // A state file with neither timestamp — the shape a session predating this field
    // wrote — must read as "no time recorded" rather than panic or claim an age of zero.
    #[test]
    fn phase_summary_with_no_timestamps_says_so() {
        let state: BuildState = serde_json::from_str(r#"{"phase":"implement"}"#).unwrap();
        assert!(state.unchanged_for().is_none());
        assert_eq!(state.phase_summary(), "implement, no time recorded");
    }

    // `status` shows only the tasks still worth watching: a done task and one with no
    // state file yet are both silently skipped rather than printed with nothing to say.
    #[test]
    fn phase_lines_skips_done_and_missing_state() {
        let root = std::env::temp_dir().join(format!("hcoord-phase-lines-{}", std::process::id()));
        let coordinator = root.join("p/coordinator/b");
        std::fs::create_dir_all(&coordinator).unwrap();
        let running = root.join("p/b/b-one");
        std::fs::create_dir_all(&running).unwrap();
        std::fs::write(
            running.join(STATE_FILE),
            r#"{"phase":"implement","phase_since":"2020-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        let finished = root.join("p/b/b-two");
        std::fs::create_dir_all(&finished).unwrap();
        std::fs::write(finished.join(STATE_FILE), r#"{"phase":"done"}"#).unwrap();

        let c = Coordinator::parse(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"b-one","title":"One"},
                {"task":"b-two","title":"Two"},
                {"task":"b-three","title":"Three"}]}"#,
        )
        .unwrap();
        let lines = c.phase_lines(&coordinator);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("b-one"));
        assert!(lines[0].contains("implement"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
