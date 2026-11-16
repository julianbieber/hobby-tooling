//! Block until a task's pipeline needs a person or has finished, so the coordinator
//! spends one shell call per task instead of a prompt per poll.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::model::{AcceptanceResult, BuildState, CiResult, LoadError, MergeResult};
use crate::output::Output;

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
/// `human` also resolves on a `play` phase that has sat untouched for
/// [`crate::model::IDLE_GATE_SECS`] — the backstop for a session that printed its
/// play-gate report without writing `await-play`, which otherwise leaves this call
/// returning `timeout` forever.
///
/// An unattended run's state file also carries what its acceptance checks did, and the
/// line then carries `acceptance` as `{passed, checks, failed}`, each failure naming its
/// command and exit status — the evidence a coordinator would otherwise have to read out
/// of the task's own pane. `acceptance` also carries `hand`, the number of checks nobody
/// ran because their scenario is `hand`, when that number is not zero; those checks are
/// counted in `checks` and are never in `failed`.
///
/// A run that merged its own PR carries `merge` as `{outcome, pr, reason, check,
/// merge_commit}`, `reason`/`check`/`merge_commit` present only when recorded — this is
/// how an un-updated coordinator's W4 learns to skip its own merge call and go straight
/// to `done`, or to read a merge failure's reason off a `blocked` outcome instead of the
/// acceptance failures.
///
/// A run that recorded its CI checks carries `ci` as `{outcome, case, check, reason}`,
/// each of the last three present only when recorded. `pr` is the PR the run opened,
/// when it recorded one. At `await-fix` the line also carries `decision`: one line naming
/// what is blocked — CI, the merge, or the acceptance checks, the first that applies —
/// which is what a coordinator puts on the task's card.
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
pub fn run(
    task_notes_dir: &Path,
    timeout_secs: u64,
    after: Option<&str>,
) -> Result<Output, LoadError> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    match resolve(task_notes_dir, deadline, POLL, after) {
        Resolution::Reached(line) => Ok(Output::line(&line, ExitCode::SUCCESS)),
        Resolution::TimedOut(line) => Ok(Output::line(&line, ExitCode::from(2))),
        Resolution::Unreadable(error) => Err(error),
    }
}

#[derive(Debug)]
enum Resolution {
    Reached(String),
    TimedOut(String),
    Unreadable(LoadError),
}

fn resolve(
    task_notes_dir: &Path,
    deadline: Instant,
    poll: Duration,
    after: Option<&str>,
) -> Resolution {
    let mut last_error: Option<LoadError>;
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
    let mut object = Map::new();
    object.insert("phase".to_owned(), Value::from(state.phase.as_str()));
    object.insert("outcome".to_owned(), Value::from(outcome));
    if let Some(acceptance) = &state.acceptance_result {
        object.insert("acceptance".to_owned(), acceptance_json(acceptance));
    }
    if let Some(merge) = &state.merge_result {
        object.insert("merge".to_owned(), merge_json(merge));
    }
    if let Some(ci) = &state.ci_result {
        object.insert("ci".to_owned(), ci_json(ci));
    }
    if let Some(pr) = state.pr_url() {
        object.insert("pr".to_owned(), Value::from(pr));
    }
    if let Some(decision) = state.decision() {
        object.insert("decision".to_owned(), Value::String(decision));
    }
    if let Some(phase_since) = &state.phase_since {
        object.insert("phase_since".to_owned(), Value::from(phase_since.as_str()));
    }
    if let Some(written) = &state.written {
        object.insert("written".to_owned(), Value::from(written.as_str()));
    }
    if let Some(unchanged_for) = state.unchanged_for() {
        object.insert("unchanged_for".to_owned(), Value::from(unchanged_for));
    }
    Value::Object(object).to_string()
}

fn present(object: &mut Map<String, Value>, fields: &[(&str, &Option<String>)]) {
    for &(key, value) in fields {
        if let Some(value) = value {
            object.insert(key.to_owned(), Value::from(value.as_str()));
        }
    }
}

fn merge_json(merge: &MergeResult) -> Value {
    let mut object = Map::new();
    object.insert("outcome".to_owned(), Value::from(merge.outcome.as_str()));
    present(
        &mut object,
        &[
            ("pr", &merge.pr),
            ("reason", &merge.reason),
            ("check", &merge.check),
            ("merge_commit", &merge.merge_commit),
        ],
    );
    Value::Object(object)
}

fn ci_json(ci: &CiResult) -> Value {
    let mut object = Map::new();
    object.insert("outcome".to_owned(), Value::from(ci.outcome.as_str()));
    present(
        &mut object,
        &[
            ("case", &ci.case),
            ("check", &ci.check),
            ("reason", &ci.reason),
        ],
    );
    Value::Object(object)
}

fn acceptance_json(acceptance: &AcceptanceResult) -> Value {
    let failed: Vec<Value> = acceptance
        .checks
        .iter()
        .filter(|c| !c.is_hand() && !c.passed())
        .map(|c| serde_json::json!({ "name": c.label(), "command": c.command, "exit": c.exit }))
        .collect();
    let mut object = Map::new();
    object.insert("passed".to_owned(), Value::from(acceptance.passed));
    object.insert("checks".to_owned(), Value::from(acceptance.checks.len()));
    object.insert("failed".to_owned(), Value::Array(failed));
    let hand = acceptance.hand();
    if hand > 0 {
        object.insert("hand".to_owned(), Value::from(hand));
    }
    Value::Object(object)
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
        assert!(
            matches!(&resolution, Resolution::Reached(line) if line == r#"{"outcome":"human","phase":"await-play"}"#),
            "{resolution:?}"
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
            Resolution::Unreadable(e) => {
                assert!(e.to_string().contains(crate::model::STATE_FILE), "{e}")
            }
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
        assert!(
            matches!(&resolution, Resolution::TimedOut(line) if line == r#"{"outcome":"timeout","phase":"implement"}"#),
            "{resolution:?}"
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
            r#"{"acceptance":{"checks":2,"failed":[{"command":"hcoord wait .","exit":1,"name":"cli"}],"passed":false},"decision":"acceptance failed: cli","outcome":"human","phase":"await-fix"}"#
        );
        let value: serde_json::Value = serde_json::from_str(&line(&failed, "human")).unwrap();
        assert!(value["acceptance"].get("hand").is_none());
    }

    // What a coordinator needs to see when a scenario was left to hand: the count, apart
    // from `failed`, and a run that passed everything it could run still reading as passed.
    #[test]
    fn the_line_counts_hand_checks_apart_from_the_ones_that_ran() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"done","acceptance_result":{"passed":true,"checks":[
                {"scenario":"S1","command":"a","exit":0,"state":"pass"},
                {"scenario":"S2","command":"b","exit":0,"state":"pass"},
                {"scenario":"S3","command":"c","exit":0,"state":"pass"},
                {"scenario":"S6","state":"hand","screen":"screens/S6.png"}]}}"#,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&line(&state, "done")).unwrap();
        assert_eq!(value["acceptance"]["hand"], 1);
        assert_eq!(value["acceptance"]["checks"], 4);
        assert_eq!(value["acceptance"]["passed"], true);
        assert_eq!(value["acceptance"]["failed"], serde_json::json!([]));
    }

    // The line a coordinator reads to go straight to done with the PR and make no merge
    // call of its own.
    #[test]
    fn the_line_carries_a_merged_result() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"done","merge_result":{"by":"session","outcome":"merged",
                "pr":"https://github.com/o/r/pull/9","merge_commit":"abc123"}}"#,
        )
        .unwrap();
        let rendered = line(&state, "done");
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["merge"]["outcome"], "merged");
        assert_eq!(value["merge"]["pr"], "https://github.com/o/r/pull/9");
    }

    // The reason and the failing check have to reach the card, and `await-fix` is what
    // stops the list on a merge a session could not make.
    #[test]
    fn the_line_carries_a_blocked_merge_reason_and_check() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"await-fix","merge_result":{"by":"session","outcome":"blocked",
                "pr":"https://github.com/o/r/pull/9","reason":"checks failing: build",
                "check":"build","log":"error[E0308] mismatched types"}}"#,
        )
        .unwrap();
        let rendered = line(&state, "human");
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["merge"]["reason"], "checks failing: build");
        assert_eq!(value["merge"]["check"], "build");
    }

    // A CI block with its PR open must reach the line as a CI decision carrying the PR,
    // not as a failed acceptance with the PR lost.
    #[test]
    fn the_line_carries_a_ci_block_as_the_decision_with_its_pr() {
        let state: BuildState = serde_json::from_str(
            r#"{"phase":"await-fix","auto":true,"pr":"https://github.com/o/r/pull/7",
                "acceptance_result":{"passed":true,"checks":[]},
                "ci_result":{"outcome":"blocked","case":"decision","check":"build",
                    "log":"failed to select a version","rounds":0}}"#,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&line(&state, "human")).unwrap();
        assert_eq!(value["decision"], "CI decision: build");
        assert_eq!(value["pr"], "https://github.com/o/r/pull/7");
        assert_eq!(value["ci"]["case"], "decision");
        assert!(value["ci"].get("log").is_none());
    }

    // A state file with no merge result must not grow a `merge` field the coordinator
    // could misread as a merge having happened.
    #[test]
    fn the_line_carries_no_merge_field_when_none_was_recorded() {
        let state: BuildState = serde_json::from_str(r#"{"phase":"implement"}"#).unwrap();
        let rendered = line(&state, "timeout");
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert!(value.get("merge").is_none());
    }

    // The bug behind #45: a session that printed its play-gate report and never wrote
    // `await-play` leaves a coordinator polling `play` forever. A `play` untouched since
    // 2020 has to resolve as `human` through `resolve`, not just through `needs_human`
    // in isolation.
    #[test]
    fn resolve_reports_an_idle_play_as_human() {
        let dir = temp_dir("idle-play");
        std::fs::write(
            state_file(&dir),
            r#"{"phase":"play","phase_since":"2020-01-01T00:00:00Z",
                "written":"2020-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        let resolution = resolve(
            &dir,
            Instant::now() + Duration::from_millis(50),
            Duration::from_millis(10),
            None,
        );
        match resolution {
            Resolution::Reached(line) => {
                let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(value["outcome"], "human");
                assert_eq!(value["phase"], "play");
            }
            other => panic!("expected Reached, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
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
