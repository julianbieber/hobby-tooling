//! Whether a task's hcdev session already has a terminal window, runs detached, or does not run —
//! and focusing that window, which makes leftwm switch to its tag.

use std::collections::{BTreeSet, HashMap};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hwt_core::Interval;
use log::warn;

use crate::model::task::{Task, TaskId};

/// How often windows and zellij sessions are probed.
pub const PRESENCE_POLL_SECS: u64 = 2;
/// How long after a launch from a card further launches of the same task are ignored.
pub const LAUNCH_GRACE_SECS: u64 = 5;
const WINDOW_CLASSES: [&str; 2] = ["Alacritty", "hcdev-auto"];

/// Whether a window title shows the task's session: its zellij session name or the unclamped
/// `hcdev-PROJECT-TOPIC-TASK` an hdash launch titles the window with, alone or followed by
/// ` | <pane>`. Compared literally, never as a pattern.
pub fn title_shows(title: &str, task: &Task) -> bool {
    [task.session_name.clone(), task.window_title()]
        .iter()
        .filter(|name| !name.is_empty())
        .any(|name| {
            title == name
                || title
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with(" | "))
        })
}

/// The names of the zellij sessions that run (not exited), from `zellij list-sessions
/// --no-formatting`.
pub fn parse_sessions(out: &str) -> BTreeSet<String> {
    out.lines()
        .filter(|l| !l.contains("EXITED"))
        .filter_map(|l| l.split_once(" [").map(|(name, _)| name.trim().to_owned()))
        .filter(|name| !name.is_empty())
        .collect()
}

type Snapshot = (BTreeSet<String>, BTreeSet<String>);

/// Where a task's session stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Presence {
    Window,
    Detached,
    None,
    Unknown,
}

/// What the last probe saw: the hcdev window titles and the running sessions. `None` when the probe
/// failed (no xdotool or zellij, no X), so every card reads [`Presence::Unknown`].
#[derive(Default, PartialEq, Debug)]
pub struct Presences(pub Option<Snapshot>);

impl Presences {
    /// Where `task`'s session stood at the last probe: a window shows it, it runs without one, it
    /// does not run, or the probe failed.
    pub fn of(&self, task: &Task) -> Presence {
        let Some((titles, sessions)) = &self.0 else {
            return Presence::Unknown;
        };
        if titles.iter().any(|t| title_shows(t, task)) {
            Presence::Window
        } else if sessions.contains(&task.session_name) {
            Presence::Detached
        } else {
            Presence::None
        }
    }
}

fn hcdev_windows() -> Option<Vec<(u64, String)>> {
    let mut ids = BTreeSet::new();
    for class in WINDOW_CLASSES {
        let out = Command::new("xdotool")
            .args(["search", "--class", class])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() && (!out.stdout.is_empty() || !out.stderr.is_empty()) {
            return None;
        }
        ids.extend(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u64>().ok()),
        );
    }
    Some(
        ids.into_iter()
            .filter_map(|wid| {
                let name = Command::new("xdotool")
                    .args(["getwindowname", &wid.to_string()])
                    .stdin(Stdio::null())
                    .output()
                    .ok()
                    .filter(|o| o.status.success())?;
                let title = String::from_utf8_lossy(&name.stdout).trim().to_owned();
                title.starts_with("hcdev-").then_some((wid, title))
            })
            .collect(),
    )
}

fn running_sessions() -> Option<BTreeSet<String>> {
    let out = Command::new("zellij")
        .args(["list-sessions", "--no-formatting"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    Some(parse_sessions(&String::from_utf8_lossy(&out.stdout)))
}

fn probe() -> Option<Snapshot> {
    let titles = hcdev_windows()?.into_iter().map(|(_, t)| t).collect();
    Some((titles, running_sessions()?))
}

/// The lowest-id window showing the task's session right now, looked up fresh.
pub fn find_window(task: &Task) -> Option<u64> {
    hcdev_windows()?
        .into_iter()
        .find(|(_, title)| title_shows(title, task))
        .map(|(wid, _)| wid)
}

/// Activate a window: leftwm switches to its tag and focuses it. `false` when that failed (the
/// window closed meanwhile, no xdotool).
pub fn activate(wid: u64) -> bool {
    Command::new("xdotool")
        .args(["windowactivate", &wid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Poll interval and the channel of the probe in flight.
pub struct PresenceProbe {
    every: Interval,
    in_flight: Option<Receiver<Option<Snapshot>>>,
    warned: bool,
}

impl Default for PresenceProbe {
    fn default() -> Self {
        Self {
            every: Interval::new(Duration::from_secs(PRESENCE_POLL_SECS), true),
            in_flight: None,
            warned: false,
        }
    }
}

impl PresenceProbe {
    /// Fold a finished probe into `presences`, and start the next one on its own thread every
    /// [`PRESENCE_POLL_SECS`] (one at a time, the first on the first pass). Answers whether
    /// `presences` changed.
    pub fn tick(&mut self, delta: Duration, presences: &mut Presences) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.in_flight {
            match rx.try_recv() {
                Ok(result) => {
                    self.in_flight = None;
                    if result.is_none() && !self.warned {
                        warn!(
                            "cannot probe windows/zellij sessions (xdotool or zellij missing?); cards show Open"
                        );
                        self.warned = true;
                    }
                    let next = Presences(result);
                    if *presences != next {
                        *presences = next;
                        changed = true;
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.in_flight = None,
            }
        }
        if self.every.tick(delta) && self.in_flight.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.in_flight = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(probe());
            });
        }
        changed
    }
}

/// When each task was last launched from its card, for the click grace period.
#[derive(Default)]
pub struct Launches(HashMap<TaskId, Instant>);

impl Launches {
    /// Whether a launch for `id` is younger than [`LAUNCH_GRACE_SECS`].
    pub fn pending(&self, id: &TaskId) -> bool {
        self.0
            .get(id)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(LAUNCH_GRACE_SECS))
    }

    pub fn record(&mut self, id: TaskId) {
        self.0.insert(id, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task::{GitInfo, Repo};
    use std::path::PathBuf;

    fn task(project: &str, topic: &str, name: &str, session: &str) -> Task {
        let id = TaskId {
            project: project.into(),
            topic: topic.into(),
            name: name.into(),
        };
        let dir = PathBuf::from("/nowhere").join(id.rel());
        Task {
            repos: vec![Repo {
                path: dir.clone(),
                git: GitInfo::default(),
            }],
            id,
            dir,
            adopted: true,
            present: true,
            done: false,
            block: None,
            backlog: None,
            title: None,
            has_branch: true,
            prs: Vec::new(),
            subtasks: Vec::new(),
            aspects: Vec::new(),
            last_active: None,
            coordinator: None,
            phase: None,
            todo: Vec::new(),
            waiting_on: None,
            session_name: session.into(),
        }
    }

    // Pins that a window is found under either spelling a session's title takes — the clamped
    // session name zellij and `hcdev auto -w` use, or the unclamped one an hdash launch sets.
    #[test]
    fn titles_match_session_or_unclamped_name_literally() {
        let long = task(
            "wusel",
            "food",
            "gh-6-simulate-food",
            "hcdev-wusel-food-gh-6-si-a1b2c3",
        );
        assert!(title_shows(
            "hcdev-wusel-food-gh-6-si-a1b2c3 | claude",
            &long
        ));
        assert!(title_shows("hcdev-wusel-food-gh-6-simulate-food", &long));
        let short = task("a", "b", "c", "hcdev-a-b-c");
        assert!(title_shows("hcdev-a-b-c", &short));
        assert!(title_shows("hcdev-a-b-c | editor", &short));
        assert!(!title_shows("hcdev-a-b-cd", &short));
        assert!(!title_shows(
            "hcdev-a-b-c.* | claude",
            &task("a", "b", "x", "")
        ));
    }

    // Pins that an exited session or zellij's chatter is not read as a running session.
    #[test]
    fn parse_sessions_skips_exited_and_noise() {
        let out = "hcdev-a-b-c [Created 2h ago] (current)\nhcdev-c-d-e [Created 2months ago] (EXITED - attach to resurrect)\nNo active zellij sessions found.\n";
        assert_eq!(
            parse_sessions(out),
            BTreeSet::from(["hcdev-a-b-c".to_owned()])
        );
    }

    // Pins the label order of the primary button: a window, then a running session, then none;
    // a failed probe is unknown.
    #[test]
    fn presence_prefers_window_then_running_session() {
        let t = task("a", "b", "c", "hcdev-a-b-c");
        let titles = BTreeSet::from(["hcdev-a-b-c | claude".to_owned()]);
        let running = BTreeSet::from(["hcdev-a-b-c".to_owned()]);
        assert_eq!(
            Presences(Some((titles, running.clone()))).of(&t),
            Presence::Window
        );
        assert_eq!(
            Presences(Some((BTreeSet::new(), running))).of(&t),
            Presence::Detached
        );
        assert_eq!(
            Presences(Some((BTreeSet::new(), BTreeSet::new()))).of(&t),
            Presence::None
        );
        assert_eq!(Presences(None).of(&t), Presence::Unknown);
    }
}
