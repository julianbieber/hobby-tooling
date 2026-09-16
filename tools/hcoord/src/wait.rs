//! Block until a task's pipeline needs a person or has finished, so the coordinator
//! spends one shell call per task instead of a prompt per poll.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::model::{AcceptanceResult, BuildState};

/// Under the ten minutes Claude Code allows a single shell call.
pub const DEFAULT_TIMEOUT_SECS: u64 = 570;

const POLL: Duration = Duration::from_secs(2);

/// Poll the task's `build-task.json` until its phase needs a human or is terminal, or
/// the timeout passes. Prints one JSON line `{"phase": …, "outcome": …}` with outcome
/// `human`, `done` or `timeout`, and exits 0, 0 or 2 respectively; a file that exists
/// but cannot be read exits 1. A file that does not exist yet is waited on.
///
/// An unattended run's state file also carries what its acceptance checks did, and the
/// line then carries `acceptance` as `{passed, checks, failed}`, each failure naming its
/// command and exit status — the evidence a coordinator would otherwise have to read out
/// of the task's own pane.
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
                println!("{}", line(&state, "human"));
                return ExitCode::SUCCESS;
            }
            Ok(Some(state)) if state.is_terminal() => {
                println!("{}", line(&state, "done"));
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("hcoord wait: {e}");
                return ExitCode::from(1);
            }
        }
        if Instant::now() >= deadline {
            let state = BuildState::load(task_notes_dir)
                .ok()
                .flatten()
                .unwrap_or_default();
            println!("{}", line(&state, "timeout"));
            return ExitCode::from(2);
        }
        std::thread::sleep(POLL);
    }
}

fn line(state: &BuildState, outcome: &str) -> String {
    let mut object = serde_json::json!({ "phase": state.phase, "outcome": outcome });
    if let Some(acceptance) = &state.acceptance_result {
        object["acceptance"] = acceptance_json(acceptance);
    }
    object.to_string()
}

fn acceptance_json(acceptance: &AcceptanceResult) -> serde_json::Value {
    let failed: Vec<serde_json::Value> = acceptance
        .checks
        .iter()
        .filter(|c| !c.pass)
        .map(|c| serde_json::json!({ "name": c.name, "command": c.command, "exit": c.exit }))
        .collect();
    serde_json::json!({
        "passed": acceptance.passed,
        "checks": acceptance.checks.len(),
        "failed": failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The coordinator reads this line instead of the task's pane, so a run that recorded
    // an acceptance must name its failures here and one that recorded none must not grow
    // a field the coordinator would read as a pass.
    #[test]
    fn the_line_carries_an_acceptance_only_when_one_was_recorded() {
        let bare: BuildState =
            serde_json::from_str(r#"{"phase":"await-play","acceptance":"place a field"}"#).unwrap();
        assert_eq!(
            line(&bare, "human"),
            r#"{"outcome":"human","phase":"await-play"}"#
        );

        let failed: BuildState = serde_json::from_str(
            r#"{"phase":"await-fix","acceptance_result":{"checks":[
                {"name":"tests","command":"cargo test","exit":0,"pass":true},
                {"name":"cli","command":"hcoord wait .","exit":1}]}}"#,
        )
        .unwrap();
        assert_eq!(
            line(&failed, "human"),
            r#"{"acceptance":{"checks":2,"failed":[{"command":"hcoord wait .","exit":1,"name":"cli"}],"passed":false},"outcome":"human","phase":"await-fix"}"#
        );
    }
}
