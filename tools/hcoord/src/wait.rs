//! Block until a task's pipeline needs a person or has finished, so the coordinator
//! spends one shell call per task instead of a prompt per poll.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::model::BuildState;

/// Under the ten minutes Claude Code allows a single shell call.
pub const DEFAULT_TIMEOUT_SECS: u64 = 570;

const POLL: Duration = Duration::from_secs(2);

/// Poll the task's `build-task.json` until its phase needs a human or is terminal, or
/// the timeout passes. Prints one JSON line `{"phase": …, "outcome": …}` with outcome
/// `human`, `done` or `timeout`, and exits 0, 0 or 2 respectively; a file that exists
/// but cannot be read exits 1. A file that does not exist yet is waited on.
///
/// `after` names a phase that does not count: a coordinator that has already reported
/// `await-play` passes it here so the call blocks until the person has answered and the
/// pipeline has moved on, instead of returning at once with the same phase.
pub fn run(task_notes_dir: &Path, timeout_secs: u64, after: Option<&str>) -> ExitCode {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match BuildState::load(task_notes_dir) {
            Ok(Some(state)) if after == Some(state.phase.as_str()) => {}
            Ok(Some(state)) if state.needs_human() => {
                println!("{}", line(&state.phase, "human"));
                return ExitCode::SUCCESS;
            }
            Ok(Some(state)) if state.is_terminal() => {
                println!("{}", line(&state.phase, "done"));
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("hcoord wait: {e}");
                return ExitCode::from(1);
            }
        }
        if Instant::now() >= deadline {
            let phase = BuildState::load(task_notes_dir)
                .ok()
                .flatten()
                .map(|s| s.phase)
                .unwrap_or_default();
            println!("{}", line(&phase, "timeout"));
            return ExitCode::from(2);
        }
        std::thread::sleep(POLL);
    }
}

fn line(phase: &str, outcome: &str) -> String {
    serde_json::json!({ "phase": phase, "outcome": outcome }).to_string()
}
