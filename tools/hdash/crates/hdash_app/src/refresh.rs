//! Keeps the model current without the Refresh button: a cheap fingerprint of the notes and
//! worktree trees is taken off the main thread every few seconds, and when it changes the model is
//! rescanned in the background for the view to merge in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, SystemTime};

use hwt_core::Interval;

use crate::model::item::{Model, scan};
use crate::model::task::{notes_root, worktrees_root};

/// How often the fingerprint is taken.
pub const FINGERPRINT_POLL_SECS: u64 = 2;

const NOTES_FILES: [&str; 6] = [
    "task.json",
    "prs.json",
    "subtasks.json",
    "build-task.json",
    "issue.md",
    "tasks.json",
];

/// Modification times of everything whose change can alter the model: the worktree and notes
/// roots and their project, topic and task dirs (a task appearing or vanishing), and per notes
/// task the files a card reads: `task.json`, `prs.json`, `subtasks.json`, `build-task.json`,
/// `issue.md` and `tasks.json`.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Fingerprint(HashMap<PathBuf, SystemTime>);

/// Take the fingerprint of a `<worktrees>/PROJECT/TOPIC/TASK` tree and a
/// `<notes>/PROJECT/TOPIC/TASK` tree. Dot dirs (the notes repo's `.git`) are skipped.
pub fn fingerprint(worktrees: &Path, notes: &Path) -> Fingerprint {
    let mut fp = HashMap::new();
    for (dir, _) in three_levels(worktrees) {
        record(&mut fp, &dir);
    }
    for (dir, depth) in three_levels(notes) {
        record(&mut fp, &dir);
        if depth == 3 {
            for file in NOTES_FILES {
                record(&mut fp, &dir.join(file));
            }
            let aspects = dir.join("aspects");
            record(&mut fp, &aspects);
            for aspect in visible_dirs(&aspects) {
                record(&mut fp, &aspect.join("aspect.json"));
            }
        }
    }
    Fingerprint(fp)
}

fn three_levels(root: &Path) -> Vec<(PathBuf, u8)> {
    let mut out = vec![(root.to_path_buf(), 0)];
    let mut frontier = vec![root.to_path_buf()];
    for depth in 1..=3 {
        let next: Vec<PathBuf> = frontier.iter().flat_map(|d| visible_dirs(d)).collect();
        out.extend(next.iter().map(|d| (d.clone(), depth)));
        frontier = next;
    }
    out
}

fn visible_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect()
}

fn record(fp: &mut HashMap<PathBuf, SystemTime>, path: &Path) {
    if let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) {
        fp.insert(path.to_path_buf(), modified);
    }
}

struct Round {
    fingerprint: Fingerprint,
    generation: u64,
    fresh: Option<Model>,
}

/// A rescan that landed, and whether it was a Refresh (replace the model) or a fingerprint change
/// (merge it in).
pub enum Landed {
    Merge(Model),
    Replace(Model),
}

/// Poll interval, the last fingerprint and the round in flight. Refresh starts a generation: a
/// round started before it lands as nothing.
pub struct RefreshPoller {
    every: Interval,
    last: Option<Fingerprint>,
    in_flight: Option<Receiver<Round>>,
    generation: u64,
    refresh: Option<Receiver<Model>>,
}

impl Default for RefreshPoller {
    fn default() -> Self {
        Self {
            every: Interval::new(Duration::from_secs(FINGERPRINT_POLL_SECS), false),
            last: None,
            in_flight: None,
            generation: 0,
            refresh: None,
        }
    }
}

impl RefreshPoller {
    /// Rescan everything now on its own thread, superseding any fingerprint round in flight; the
    /// result lands from [`RefreshPoller::tick`] as [`Landed::Replace`].
    pub fn refresh(&mut self) {
        self.generation += 1;
        self.last = None;
        let (tx, rx) = std::sync::mpsc::channel();
        self.refresh = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(scan());
        });
    }

    /// Whether a Refresh rescan is still running.
    pub fn refreshing(&self) -> bool {
        self.refresh.is_some()
    }

    /// Land whatever finished, then start a fingerprint round on its own thread when the interval
    /// completes and none is running.
    pub fn tick(&mut self, delta: Duration) -> Option<Landed> {
        let landed = self.land_refresh().or_else(|| self.land_round());
        if self.every.tick(delta) && self.in_flight.is_none() && self.refresh.is_none() {
            let last = self.last.clone();
            let generation = self.generation;
            let (tx, rx) = std::sync::mpsc::channel();
            self.in_flight = Some(rx);
            std::thread::spawn(move || {
                let fingerprint = fingerprint(&worktrees_root(), &notes_root());
                let fresh = (last.as_ref() != Some(&fingerprint)).then(scan);
                let _ = tx.send(Round {
                    fingerprint,
                    generation,
                    fresh,
                });
            });
        }
        landed
    }

    fn land_refresh(&mut self) -> Option<Landed> {
        match self.refresh.as_ref()?.try_recv() {
            Ok(model) => {
                self.refresh = None;
                Some(Landed::Replace(model))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.refresh = None;
                None
            }
        }
    }

    fn land_round(&mut self) -> Option<Landed> {
        let round = match self.in_flight.as_ref()?.try_recv() {
            Ok(round) => round,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                self.in_flight = None;
                return None;
            }
        };
        self.in_flight = None;
        if round.generation != self.generation {
            return None;
        }
        self.last = Some(round.fingerprint);
        round.fresh.map(Landed::Merge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins what the fingerprint watches: task dirs three levels down and the notes files a card
    // reads, including each aspect's aspect.json, but not a worktree's contents or the notes
    // repo's .git.
    #[test]
    fn fingerprint_sees_task_dirs_and_notes_files_but_not_repo_contents() {
        let root = std::env::temp_dir().join(format!("hdash_fp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (wt, notes) = (root.join("worktrees"), root.join("notes"));
        std::fs::create_dir_all(wt.join("p/topic/task/src")).unwrap();
        std::fs::create_dir_all(notes.join("p/topic/task")).unwrap();
        std::fs::create_dir_all(notes.join(".git/objects/ab")).unwrap();
        std::fs::write(notes.join("p/topic/task/task.json"), "{}").unwrap();

        let fp = fingerprint(&wt, &notes);
        assert!(fp.0.contains_key(&wt.join("p/topic/task")));
        assert!(fp.0.contains_key(&notes.join("p/topic/task/task.json")));
        assert!(!fp.0.contains_key(&wt.join("p/topic/task/src")));
        assert!(!fp.0.keys().any(|p| p.starts_with(notes.join(".git"))));
        assert_eq!(fp, fingerprint(&wt, &notes));

        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(notes.join("p/topic/task/build-task.json"), "{}").unwrap();
        assert_ne!(fp, fingerprint(&wt, &notes), "a phase write is seen");

        let fp = fingerprint(&wt, &notes);
        std::thread::sleep(Duration::from_millis(20));
        std::fs::create_dir_all(notes.join("p/topic/task/aspects/spike")).unwrap();
        std::fs::write(notes.join("p/topic/task/aspects/spike/aspect.json"), "{}").unwrap();
        let with_aspect = fingerprint(&wt, &notes);
        assert_ne!(fp, with_aspect, "a new aspect is seen");
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            notes.join("p/topic/task/aspects/spike/aspect.json"),
            r#"{"state":"reported"}"#,
        )
        .unwrap();
        assert_ne!(
            with_aspect,
            fingerprint(&wt, &notes),
            "an aspect state write is seen"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
