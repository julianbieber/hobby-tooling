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
/// `human`, `done` or `timeout`, and exits 0, 0 or 2 respectively. A file that does not
/// exist yet is waited on, and so is one that cannot be read or parsed — the ordinary
/// cause of both is the task session part-way through writing it. Exit 1 therefore means
/// the file never became parseable before the timeout, and the last error is printed.
///
/// An unattended run's state file also carries what its acceptance checks did, and the
/// line then carries `acceptance` as `{passed, checks, failed}`, each failure naming its
/// command and exit status — the evidence a coordinator would otherwise have to read out
/// of the task's own pane.
///
/// A state file that recorded `phase_since` also adds it and `unchanged_for` (seconds,
/// computed at print time) to the line, plus `written` when that was recorded too — a
/// coordinator polling a phase that has not moved can then say how long, instead of
/// sampling `/proc` or a pane it cannot read. A file written before these fields existed
/// carries none of the three.
///
/// `after` names a phase that does not count: a coordinator that has already reported
/// `await-play` passes it here so the call blocks until the person has answered and the
/// pipeline has moved on, instead of returning at once with the same phase.
pub fn run(task_notes_dir: &Path, timeout_secs: u64, after: Option<&str>) -> ExitCode {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    match resolve(task_notes_dir, deadline, POLL, after) {
        Resolution::Reached(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Resolution::TimedOut(line) => {
            println!("{line}");
            ExitCode::from(2)
        }
        Resolution::Unreadable(error) => {
            eprintln!("hcoord wait: {error}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, PartialEq)]
enum Resolution {
    Reached(String),
    TimedOut(String),
    Unreadable(String),
}

fn resolve(
    task_notes_dir: &Path,
    deadline: Instant,
    poll: Duration,
    after: Option<&str>,
) -> Resolution {
    let mut last_error: Option<String>;
    let mut last_state: Option<BuildState> = None;
    loop {
        match BuildState::load(task_notes_dir) {
            Ok(loaded) => {
                last_error = None;
                if let Some(state) = loaded {
                    if after != Some(state.phase.as_str()) {
                        if state.needs_human() {
                            return Resolution::Reached(line(&state, "human"));
                        }
                        if state.is_terminal() {
                            return Resolution::Reached(line(&state, "done"));
                        }
                    }
                    last_state = Some(state);
                }
            }
            Err(e) => last_error = Some(e),
        }
        if Instant::now() >= deadline {
            return match last_error {
                Some(e) => Resolution::Unreadable(e),
                None => Resolution::TimedOut(line(&last_state.unwrap_or_default(), "timeout")),
            };
        }
        std::thread::sleep(poll);
    }
}

fn line(state: &BuildState, outcome: &str) -> String {
    let mut object = serde_json::json!({ "phase": state.phase, "outcome": outcome });
    if let Some(acceptance) = &state.acceptance_result {
        object["acceptance"] = acceptance_json(acceptance);
    }
    if let Some(phase_since) = &state.phase_since {
        object["phase_since"] = serde_json::Value::String(phase_since.clone());
    }
    if let Some(written) = &state.written {
        object["written"] = serde_json::Value::String(written.clone());
    }
    if let Some(unchanged_for) = state.unchanged_for() {
        object["unchanged_for"] = serde_json::Value::from(unchanged_for);
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

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hcoord-wait-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn state_file(dir: &Path) -> std::path::PathBuf {
        dir.join(crate::model::STATE_FILE)
    }

    // The bug this waits on: a task session rewriting its state file leaves it zero bytes
    // for an instant, and reporting that as unreadable makes a coordinator block a list
    // whose task is fine. A file that parses on a later poll must resolve on its phase.
    #[test]
    fn a_file_that_is_unparseable_now_and_parses_later_resolves_on_its_phase() {
        let dir = temp_dir("rewritten");
        std::fs::write(state_file(&dir), "").unwrap();
        let writer = {
            let path = state_file(&dir);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                std::fs::write(path, r#"{"phase":"await-play"}"#).unwrap();
            })
        };
        let resolution = resolve(
            &dir,
            Instant::now() + Duration::from_secs(2),
            Duration::from_millis(10),
            None,
        );
        writer.join().unwrap();
        assert_eq!(
            resolution,
            Resolution::Reached(r#"{"outcome":"human","phase":"await-play"}"#.to_owned())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // Retrying must not swallow the real failure: a file that never parses is still exit
    // 1, and the message has to name the file so the coordinator's note says which one.
    #[test]
    fn a_file_that_never_parses_is_unreadable_at_the_deadline() {
        let dir = temp_dir("corrupt");
        std::fs::write(state_file(&dir), "not json").unwrap();
        let resolution = resolve(
            &dir,
            Instant::now() + Duration::from_millis(50),
            Duration::from_millis(10),
            None,
        );
        match resolution {
            Resolution::Unreadable(e) => assert!(e.contains(crate::model::STATE_FILE), "{e}"),
            other => panic!("expected Unreadable, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // Pins the error being forgotten once the file reads: a run whose state file was
    // briefly broken and then sat in a working phase is a timeout the coordinator calls
    // again on, not the exit 1 that stops its list.
    #[test]
    fn an_error_before_a_good_read_does_not_survive_to_the_deadline() {
        let dir = temp_dir("recovered");
        std::fs::write(state_file(&dir), "").unwrap();
        let resolution = resolve(
            &dir,
            Instant::now() + Duration::from_millis(50),
            Duration::from_millis(10),
            None,
        );
        assert!(matches!(resolution, Resolution::Unreadable(_)));

        std::fs::write(state_file(&dir), r#"{"phase":"implement"}"#).unwrap();
        let resolution = resolve(
            &dir,
            Instant::now() + Duration::from_millis(50),
            Duration::from_millis(10),
            None,
        );
        assert_eq!(
            resolution,
            Resolution::TimedOut(r#"{"outcome":"timeout","phase":"implement"}"#.to_owned())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

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

    // A coordinator waiting on a phase that has not moved needs the age, not just a
    // repeated phase name — `phase_since`, `written` and the computed `unchanged_for`
    // travel on the line whenever the file recorded them.
    #[test]
    fn the_line_carries_phase_since_written_and_the_computed_age() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"implement","written":"2020-01-01T00:00:00Z",
                "phase_since":"2020-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        let rendered = line(&state, "timeout");
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["phase_since"], "2020-01-01T00:00:00Z");
        assert_eq!(value["written"], "2020-01-01T00:00:00Z");
        assert!(value["unchanged_for"].as_i64().unwrap() > 0);
    }
}
