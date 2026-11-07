//! Live Claude session state per task: reads the per-session files the `session-state` hook
//! writes, attributes each to the task whose session it runs in, and corrects a busy state the
//! hook never closed using the Claude process's CPU time as a heartbeat.

use std::collections::{HashMap, VecDeque};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hwt_core::Interval;
use serde::Deserialize;

use crate::model::task::{TaskId, worktrees_root};

/// How often the state files are re-read.
pub const SESSION_POLL_SECS: u64 = 1;
/// CPU ticks below which a busy session counts as quiet over [`HEARTBEAT_WINDOW_SECS`].
pub const HEARTBEAT_MIN_TICKS: u64 = 60;
/// The window the heartbeat looks back over.
pub const HEARTBEAT_WINDOW_SECS: u64 = 30;
/// The directory name the hook writes into, under `$XDG_RUNTIME_DIR` (else `~/.cache`). Distinct
/// from the work account's `claude-session-state`, so neither dashboard reads or deletes the
/// other's records. `hooks/session-state/hook.sh` spells the same name.
pub const STATE_DIR_NAME: &str = "hobby-session-state";
const MAX_RECORD_BYTES: u64 = 4096;

/// Whether a session is mid-turn or waiting on the user.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Busy,
    Idle,
}

/// What a card shows about its task's session. `since` is epoch seconds when the state began.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Session {
    pub state: SessionState,
    pub since: u64,
}

/// Whose Claude session a record is: a task's own, or one of its aspects', named.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Owner {
    Task(TaskId),
    Aspect(TaskId, String),
}

/// The live sessions: each task's own, and each aspect's, keyed by its task and name.
/// Replaced only when a poll finds a change.
#[derive(Default, PartialEq, Debug)]
pub struct Sessions {
    pub tasks: HashMap<TaskId, Session>,
    pub aspects: HashMap<(TaskId, String), Session>,
}

impl Sessions {
    /// Each task's state counting its aspects, for the display order and the chips: idle when its
    /// own session or any aspect's waits on you, else busy when any of them runs.
    pub fn states(&self) -> HashMap<TaskId, SessionState> {
        let mut out: HashMap<TaskId, SessionState> = HashMap::new();
        let all = self
            .tasks
            .iter()
            .map(|(id, s)| (id, s.state))
            .chain(self.aspects.iter().map(|((id, _), s)| (id, s.state)));
        for (id, state) in all {
            let slot = out.entry(id.clone()).or_insert(state);
            if state == SessionState::Idle {
                *slot = SessionState::Idle;
            }
        }
        out
    }

    /// The session of aspect `name` of task `id`, if one runs.
    pub fn aspect(&self, id: &TaskId, name: &str) -> Option<Session> {
        self.aspects.get(&(id.clone(), name.to_owned())).copied()
    }

    /// The state of one task counting its aspects, as [`Sessions::states`] decides it.
    pub fn state_of(&self, id: &TaskId) -> Option<SessionState> {
        self.states().get(id).copied()
    }
}

/// One state file, as the hook writes it (schema in `hooks/session-state/README.md`).
#[derive(Clone, Debug, Deserialize)]
pub struct Record {
    pub version: u32,
    pub state: SessionState,
    pub since: u64,
    pub project_dir: String,
    #[serde(default)]
    pub zellij_session: String,
    pub pid: u32,
    pub pid_start: u64,
}

/// The parts of `/proc/<pid>/stat` the reader needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcStat {
    pub start: u64,
    pub cpu_ticks: u64,
}

/// A record that belongs to a live Claude process, with its file mtime and CPU time.
#[derive(Clone, Debug)]
pub struct Observed {
    pub record: Record,
    pub last_event: u64,
    pub cpu_ticks: u64,
}

/// Parse one state file; `None` for anything that is not a version-1 record.
pub fn parse_record(text: &str) -> Option<Record> {
    let r: Record = serde_json::from_str(text).ok()?;
    (r.version == 1).then_some(r)
}

/// The task a project dir lies in: `<root>/<project>/<topic>/<task>[/...]`, compared by whole
/// components.
pub fn task_of(project_dir: &Path, root: &Path) -> Option<TaskId> {
    let mut parts = project_dir.strip_prefix(root).ok()?.components();
    let (
        Some(Component::Normal(project)),
        Some(Component::Normal(topic)),
        Some(Component::Normal(task)),
    ) = (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if parts.any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(TaskId {
        project: project.to_string_lossy().into_owned(),
        topic: topic.to_string_lossy().into_owned(),
        name: task.to_string_lossy().into_owned(),
    })
}

/// The aspect a project dir lies in: `<root>/<project>/<topic>/<task>/<name>[/...]`,
/// compared by whole components.
pub fn aspect_of(project_dir: &Path, root: &Path) -> Option<(TaskId, String)> {
    let rest = project_dir.strip_prefix(root).ok()?;
    let mut parts = rest.components();
    let (Some(Component::Normal(project)), Some(Component::Normal(topic))) =
        (parts.next(), parts.next())
    else {
        return None;
    };
    let (Some(Component::Normal(task)), Some(Component::Normal(name))) =
        (parts.next(), parts.next())
    else {
        return None;
    };
    if parts.any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some((
        TaskId {
            project: project.to_string_lossy().into_owned(),
            topic: topic.to_string_lossy().into_owned(),
            name: task.to_string_lossy().into_owned(),
        },
        name.to_string_lossy().into_owned(),
    ))
}

/// Keep, per task, the newest record whose Claude process still runs (same pid and start time, so
/// a reused pid does not count). A record belongs to what its project dir lies in: an aspect's
/// worktree under `aspects`, else a task's worktree under `root` — so a tab of a task's session
/// that works an aspect is credited to that aspect, although every tab shares the
/// task's zellij session name. Only a dir in neither, such as a coordinator's main checkout, is
/// matched by the zellij session it ran in. Every Claude process counts, whatever pane or stage
/// started it.
pub fn attribute(
    records: impl IntoIterator<Item = (Record, u64)>,
    by_session: &HashMap<String, TaskId>,
    root: &Path,
    aspects: &Path,
    proc_stat: impl Fn(u32) -> Option<ProcStat>,
) -> HashMap<Owner, Observed> {
    let mut out: HashMap<Owner, Observed> = HashMap::new();
    for (record, last_event) in records {
        let Some(stat) = proc_stat(record.pid).filter(|s| s.start == record.pid_start) else {
            continue;
        };
        let dir = Path::new(&record.project_dir);
        let Some(id) = aspect_of(dir, aspects)
            .map(|(id, name)| Owner::Aspect(id, name))
            .or_else(|| task_of(dir, root).map(Owner::Task))
            .or_else(|| {
                by_session
                    .get(&record.zellij_session)
                    .cloned()
                    .map(Owner::Task)
            })
        else {
            continue;
        };
        if out.get(&id).is_some_and(|o| o.last_event >= last_event) {
            continue;
        }
        out.insert(
            id,
            Observed {
                record,
                last_event,
                cpu_ticks: stat.cpu_ticks,
            },
        );
    }
    out
}

/// CPU-time samples per Claude process, used to spot a busy session that has gone quiet.
#[derive(Default)]
pub struct Heartbeat {
    samples: HashMap<(u32, u64), VecDeque<(u64, u64)>>,
}

impl Heartbeat {
    /// Record a CPU-time sample for a process at `now` (epoch seconds).
    pub fn record(&mut self, key: (u32, u64), now: u64, ticks: u64) {
        let q = self.samples.entry(key).or_default();
        q.push_back((now, ticks));
        while q.len() > 1 && now.saturating_sub(q[1].0) >= HEARTBEAT_WINDOW_SECS {
            q.pop_front();
        }
    }

    /// Whether the process used fewer than [`HEARTBEAT_MIN_TICKS`] over the last full window.
    /// `false` until a whole window has been sampled.
    pub fn quiet(&self, key: (u32, u64), now: u64) -> bool {
        let Some(q) = self.samples.get(&key) else {
            return false;
        };
        let (Some(&(t0, first)), Some(&(_, last))) = (q.front(), q.back()) else {
            return false;
        };
        now.saturating_sub(t0) >= HEARTBEAT_WINDOW_SECS
            && last.saturating_sub(first) < HEARTBEAT_MIN_TICKS
    }

    /// Drop samples of processes no longer observed.
    pub fn retain(&mut self, live: &[(u32, u64)]) {
        self.samples.retain(|k, _| live.contains(k));
    }
}

/// What a card shows for an observed session: the hook's state, unless it has been busy for at
/// least a whole window and the process was quiet over that window — then idle since the last hook
/// event.
pub fn effective(o: &Observed, heartbeat: &Heartbeat, now: u64) -> Session {
    let key = (o.record.pid, o.record.pid_start);
    let busy_whole_window = o.record.state == SessionState::Busy
        && now.saturating_sub(o.record.since) >= HEARTBEAT_WINDOW_SECS;
    if busy_whole_window && heartbeat.quiet(key, now) {
        Session {
            state: SessionState::Idle,
            since: o.last_event,
        }
    } else {
        Session {
            state: o.record.state,
            since: o.record.since,
        }
    }
}

/// The directory the hook writes session files to: `${XDG_RUNTIME_DIR:-$HOME/.cache}/` plus
/// [`STATE_DIR_NAME`].
pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| hwt_core::home().join(".cache"))
        .join(STATE_DIR_NAME)
}

fn read_records(dir: &Path) -> Vec<(PathBuf, Record, u64)> {
    let Ok(dir_meta) = std::fs::metadata(dir) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() || meta.uid() != dir_meta.uid() || meta.len() > MAX_RECORD_BYTES {
                return None;
            }
            let record = parse_record(&std::fs::read_to_string(e.path()).ok()?)?;
            Some((e.path(), record, epoch_secs(meta.modified().ok()?)))
        })
        .collect()
}

fn read_proc_stat(pid: u32) -> Option<ProcStat> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if &stat[open + 1..close] != "claude" {
        return None;
    }
    let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    let start: u64 = fields.get(19)?.parse().ok()?;
    Some(ProcStat {
        start,
        cpu_ticks: utime + stime,
    })
}

fn epoch_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Current time as epoch seconds.
pub fn now_secs() -> u64 {
    epoch_secs(SystemTime::now())
}

/// Poll interval plus heartbeat samples.
pub struct SessionReader {
    every: Interval,
    heartbeat: Heartbeat,
}

impl Default for SessionReader {
    fn default() -> Self {
        Self {
            every: Interval::new(Duration::from_secs(SESSION_POLL_SECS), false),
            heartbeat: Heartbeat::default(),
        }
    }
}

impl SessionReader {
    /// Refresh `sessions` when the interval completes; answer whether they changed.
    pub fn tick(
        &mut self,
        delta: Duration,
        sessions: &mut Sessions,
        by_session: &HashMap<String, TaskId>,
    ) -> bool {
        self.every.tick(delta) && self.refresh(sessions, by_session)
    }

    /// Re-read the state files and replace `sessions` when the result differs; answer whether it
    /// did. Deletes, inside the state dir only, every record (and its lock) whose Claude process
    /// has exited or whose pid now belongs to another process.
    pub fn refresh(
        &mut self,
        sessions: &mut Sessions,
        by_session: &HashMap<String, TaskId>,
    ) -> bool {
        let now = now_secs();
        let mut stats: HashMap<u32, Option<ProcStat>> = HashMap::new();
        let mut records = Vec::new();
        for (path, record, last_event) in read_records(&state_dir()) {
            let stat = *stats
                .entry(record.pid)
                .or_insert_with(|| read_proc_stat(record.pid));
            if stat.is_some_and(|s| s.start == record.pid_start) {
                records.push((record, last_event));
            } else {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_file(path.with_extension("lock"));
            }
        }
        let observed = attribute(
            records,
            by_session,
            &worktrees_root(),
            &hwt_core::aspects_root(),
            |pid| stats.get(&pid).copied().flatten(),
        );
        let live: Vec<(u32, u64)> = observed
            .values()
            .map(|o| (o.record.pid, o.record.pid_start))
            .collect();
        self.heartbeat.retain(&live);
        for o in observed.values() {
            self.heartbeat
                .record((o.record.pid, o.record.pid_start), now, o.cpu_ticks);
        }
        let mut next = Sessions::default();
        for (owner, o) in &observed {
            let session = effective(o, &self.heartbeat, now);
            match owner {
                Owner::Task(id) => {
                    next.tasks.insert(id.clone(), session);
                }
                Owner::Aspect(id, name) => {
                    next.aspects.insert((id.clone(), name.clone()), session);
                }
            }
        }
        if *sessions == next {
            return false;
        }
        *sessions = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(dir: &str, session: &str, state: SessionState, pid: u32) -> Record {
        Record {
            version: 1,
            state,
            since: 100,
            project_dir: dir.into(),
            zellij_session: session.into(),
            pid,
            pid_start: 7,
        }
    }

    fn id(project: &str, topic: &str, name: &str) -> TaskId {
        TaskId {
            project: project.into(),
            topic: topic.into(),
            name: name.into(),
        }
    }

    fn alive(_: u32) -> Option<ProcStat> {
        Some(ProcStat {
            start: 7,
            cpu_ticks: 50,
        })
    }

    // Pins the record format the hook writes, and that another version is ignored.
    #[test]
    fn parse_record_accepts_hook_output_and_rejects_other_versions() {
        let line = r#"{"version":1,"state":"busy","since":5,"project_dir":"/w/p/t/x","zellij_session":"hcdev-p-t-x","zellij_pane":"1","pid":42,"pid_start":9}"#;
        let r = parse_record(line).unwrap();
        assert_eq!(r.state, SessionState::Busy);
        assert_eq!(r.zellij_session, "hcdev-p-t-x");
        assert!(parse_record(&line.replace(r#""version":1"#, r#""version":2"#)).is_none());
        assert!(parse_record("{").is_none());
    }

    // Pins that a project dir maps to its task by the three path levels, compared whole.
    #[test]
    fn task_of_maps_three_whole_components_under_root() {
        let root = Path::new("/home/u/fun_worktrees");
        assert_eq!(
            task_of(Path::new("/home/u/fun_worktrees/p/topic/task"), root),
            Some(id("p", "topic", "task"))
        );
        assert_eq!(
            task_of(Path::new("/home/u/fun_worktrees/p/topic/task/src"), root),
            Some(id("p", "topic", "task"))
        );
        assert_eq!(
            task_of(Path::new("/home/u/fun_worktrees/p/topic"), root),
            None
        );
        assert_eq!(
            task_of(Path::new("/home/u/fun_worktreesx/a/b/c"), root),
            None
        );
        assert_eq!(
            task_of(Path::new("/home/u/fun_worktrees/a/b/c/../../d/e"), root),
            None
        );
    }

    // Pins that a coordinator's pane, whose directory is the main checkout, is found by its zellij
    // session.
    #[test]
    fn attribute_finds_coordinators_by_session_name() {
        let coordinator = id("p", "coordinator", "list");
        let by_session =
            HashMap::from([("hcdev-p-coordinator-list".to_owned(), coordinator.clone())]);
        let out = attribute(
            [
                (
                    rec(
                        "/home/u/fun_repos/p",
                        "hcdev-p-coordinator-list",
                        SessionState::Idle,
                        1,
                    ),
                    10,
                ),
                (rec("/w/p/t/a", "", SessionState::Busy, 2), 10),
                (
                    rec("/elsewhere", "cdev-work-task", SessionState::Idle, 3),
                    10,
                ),
            ],
            &by_session,
            Path::new("/w"),
            Path::new("/asp"),
            alive,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[&Owner::Task(coordinator)].record.pid, 1);
        assert_eq!(out[&Owner::Task(id("p", "t", "a"))].record.pid, 2);
    }

    // Pins the rule when several Claude processes run for one task (an auto run's stages, a
    // nested claude): the newest live record decides; a dead or reused pid never does.
    #[test]
    fn attribute_keeps_the_newest_live_record_per_task() {
        let stat = |pid: u32| match pid {
            3 => None,
            4 => Some(ProcStat {
                start: 8,
                cpu_ticks: 0,
            }),
            _ => alive(pid),
        };
        let out = attribute(
            [
                (rec("/w/p/t/a", "", SessionState::Busy, 1), 10),
                (rec("/w/p/t/a", "", SessionState::Idle, 2), 20),
                (rec("/w/p/t/a", "", SessionState::Busy, 3), 30),
                (rec("/w/p/t/a", "", SessionState::Busy, 4), 40),
            ],
            &HashMap::new(),
            Path::new("/w"),
            Path::new("/asp"),
            stat,
        );
        let a = &out[&Owner::Task(id("p", "t", "a"))];
        assert_eq!((a.record.pid, a.last_event), (2, 20));
    }

    // Pins that an aspect tab's Claude, which shares its task's zellij session name, is credited
    // to that aspect by its worktree and not to the task; the task's own pane stays the task's.
    #[test]
    fn attribute_credits_aspect_tabs_to_their_aspect() {
        let task = id("p", "t", "a");
        let by_session = HashMap::from([("hcdev-p-t-a".to_owned(), task.clone())]);
        let out = attribute(
            [
                (rec("/w/p/t/a", "hcdev-p-t-a", SessionState::Busy, 1), 10),
                (
                    rec("/asp/p/t/a/spike", "hcdev-p-t-a", SessionState::Idle, 2),
                    10,
                ),
                (
                    rec("/asp/p/t/a/spike/src", "hcdev-p-t-a", SessionState::Idle, 3),
                    5,
                ),
                (rec("/asp/p/t/a", "hcdev-p-t-a", SessionState::Idle, 4), 1),
            ],
            &by_session,
            Path::new("/w"),
            Path::new("/asp"),
            alive,
        );
        assert_eq!(out[&Owner::Task(task.clone())].record.pid, 1);
        assert_eq!(out[&Owner::Aspect(task, "spike".into())].record.pid, 2);
        assert_eq!(
            out.len(),
            2,
            "a dir above an aspect falls back to the session name"
        );
    }

    // Pins how a task's sessions and its aspects' combine for ordering: any idle one makes the
    // task idle, and an aspect alone still makes it busy or idle.
    #[test]
    fn states_count_a_tasks_aspects() {
        let s = |state| Session { state, since: 0 };
        let mut sessions = Sessions::default();
        sessions
            .tasks
            .insert(id("p", "t", "a"), s(SessionState::Busy));
        sessions
            .aspects
            .insert((id("p", "t", "a"), "x".into()), s(SessionState::Idle));
        sessions
            .aspects
            .insert((id("p", "t", "b"), "y".into()), s(SessionState::Busy));
        assert_eq!(
            sessions.state_of(&id("p", "t", "a")),
            Some(SessionState::Idle)
        );
        assert_eq!(
            sessions.state_of(&id("p", "t", "b")),
            Some(SessionState::Busy)
        );
        assert_eq!(sessions.state_of(&id("p", "t", "c")), None);
        assert_eq!(
            sessions.aspect(&id("p", "t", "a"), "x").map(|s| s.state),
            Some(SessionState::Idle)
        );
    }

    // Pins the stuck-busy correction: busy for a whole window with almost no CPU reads idle,
    // dated from the last hook event.
    #[test]
    fn busy_with_quiet_heartbeat_reads_idle_since_last_event() {
        let o = Observed {
            record: rec("/w/p/t/a", "", SessionState::Busy, 1),
            last_event: 150,
            cpu_ticks: 0,
        };
        let key = (1, 7);
        let mut hb = Heartbeat::default();
        hb.record(key, 1000, 500);
        hb.record(key, 1015, 503);
        assert_eq!(effective(&o, &hb, 1015).state, SessionState::Busy);
        hb.record(key, 1030, 505);
        assert_eq!(
            effective(&o, &hb, 1030),
            Session {
                state: SessionState::Idle,
                since: 150
            }
        );
    }

    // Pins that a turn which just started is not judged by quiet samples from before it.
    #[test]
    fn busy_that_just_started_ignores_quiet_samples_from_before() {
        let mut fresh = rec("/w/p/t/a", "", SessionState::Busy, 1);
        fresh.since = 1020;
        let o = Observed {
            record: fresh,
            last_event: 1020,
            cpu_ticks: 0,
        };
        let key = (1, 7);
        let mut hb = Heartbeat::default();
        hb.record(key, 1000, 500);
        hb.record(key, 1030, 505);
        assert_eq!(effective(&o, &hb, 1030).state, SessionState::Busy);
    }

    // Pins that a session using CPU stays busy however long its turn runs.
    #[test]
    fn busy_with_cpu_activity_stays_busy() {
        let o = Observed {
            record: rec("/w/p/t/a", "", SessionState::Busy, 1),
            last_event: 150,
            cpu_ticks: 0,
        };
        let key = (1, 7);
        let mut hb = Heartbeat::default();
        for (i, t) in (1000..=1040).step_by(5).enumerate() {
            hb.record(key, t, 500 + 20 * i as u64);
        }
        assert_eq!(effective(&o, &hb, 1040).state, SessionState::Busy);
    }

    // Pins that the heartbeat judges only the last window, not the whole history.
    #[test]
    fn heartbeat_window_slides() {
        let key = (1, 7);
        let mut hb = Heartbeat::default();
        hb.record(key, 0, 0);
        hb.record(key, 10, 100);
        hb.record(key, 40, 105);
        hb.record(key, 45, 106);
        assert!(hb.quiet(key, 45));
    }
}
