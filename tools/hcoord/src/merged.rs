//! Block until a task's pull request has landed, so the coordinator holds the list on
//! one shell call per PR instead of a prompt per poll.

use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

/// Under the ten minutes Claude Code allows a single shell call.
pub const DEFAULT_TIMEOUT_SECS: u64 = 570;

const POLL: Duration = Duration::from_secs(15);

/// What the checks on the PR's head commit add up to. GitHub reports a check run's
/// verdict as `conclusion` once it has finished and `status` until then, and a commit
/// status's as `state`, so an entry with neither conclusion nor state is still running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    /// Every check concluded successfully, was neutral, or was skipped.
    Green,
    /// At least one check has not concluded.
    Pending,
    /// At least one check failed, errored, timed out, or was cancelled.
    Failing,
    /// The repository runs no checks on this PR.
    None,
}

impl Checks {
    fn name(self) -> &'static str {
        match self {
            Checks::Green => "green",
            Checks::Pending => "pending",
            Checks::Failing => "failing",
            Checks::None => "none",
        }
    }
}

/// What a poll of the PR resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The PR is merged. The task is done.
    Merged,
    /// Nobody can merge this without a decision: `reason` says which.
    Blocked(String),
    /// Checks pass and GitHub reports no conflict, so a merge would go through now.
    /// Only ever returned with `--ready`.
    Mergeable,
    /// Nothing has resolved yet within the timeout.
    Timeout,
}

impl Outcome {
    fn name(&self) -> &'static str {
        match self {
            Outcome::Merged => "merged",
            Outcome::Blocked(_) => "blocked",
            Outcome::Mergeable => "mergeable",
            Outcome::Timeout => "timeout",
        }
    }
}

/// Poll `gh pr view` until the PR is merged, until something blocks it, or until the
/// timeout passes. Prints one JSON line `{pr, outcome, state, checks, reason}` and exits
/// 0 when a state was reached, 2 on timeout, and 1 when `gh` could not be run or its
/// answer could not be parsed.
///
/// `ready` makes an open, green, conflict-free PR return `mergeable` instead of being
/// waited on — the coordinator passes it only when it is the one merging. Without it a
/// green PR is waited on until a person merges it.
///
/// A repository that runs no checks reports `checks: none`, which counts as green: the
/// alternative is waiting forever on a repo that has no CI.
pub fn run(pr: &str, timeout_secs: u64, ready: bool) -> ExitCode {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let view = match view(pr) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("hcoord merged: {e}");
                return ExitCode::from(1);
            }
        };
        let checks = view.checks();
        if let Some(outcome) = view.resolve(checks, ready) {
            let code = match outcome {
                Outcome::Timeout => ExitCode::from(2),
                _ => ExitCode::SUCCESS,
            };
            println!("{}", line(pr, &outcome, &view.state, checks));
            return code;
        }
        if Instant::now() >= deadline {
            println!("{}", line(pr, &Outcome::Timeout, &view.state, checks));
            return ExitCode::from(2);
        }
        std::thread::sleep(POLL);
    }
}

/// One snapshot of a PR's merge state, mergeability and check rollup, as `view` reads
/// them from `gh pr view`.
pub(crate) struct View {
    pub(crate) state: String,
    pub(crate) mergeable: String,
    pub(crate) rollup: Vec<Check>,
}

/// One entry from `statusCheckRollup`: its name, its verdict, and the URL a caller can
/// pull its log from when it failed.
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) verdict: String,
    pub(crate) details_url: String,
}

impl View {
    /// Reduces the rollup to one verdict: `Failing` if anything failed, else `Pending`
    /// if anything has not concluded, else `Green`, else `None` when there is no
    /// rollup at all.
    pub(crate) fn checks(&self) -> Checks {
        if self.rollup.is_empty() {
            return Checks::None;
        }
        let mut pending = false;
        for check in &self.rollup {
            match check.verdict.as_str() {
                "SUCCESS" | "NEUTRAL" | "SKIPPED" => {}
                "PENDING" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "REQUESTED" | "EXPECTED"
                | "" => pending = true,
                _ => return Checks::Failing,
            }
        }
        if pending {
            Checks::Pending
        } else {
            Checks::Green
        }
    }

    fn resolve(&self, checks: Checks, ready: bool) -> Option<Outcome> {
        match self.state.as_str() {
            "MERGED" => return Some(Outcome::Merged),
            "CLOSED" => return Some(Outcome::Blocked("closed without merging".into())),
            _ => {}
        }
        if checks == Checks::Failing {
            return Some(Outcome::Blocked(format!(
                "checks failing: {}",
                self.failed()
            )));
        }
        if self.mergeable == "CONFLICTING" {
            return Some(Outcome::Blocked("conflicts with the base branch".into()));
        }
        if ready && checks != Checks::Pending && self.mergeable == "MERGEABLE" {
            return Some(Outcome::Mergeable);
        }
        None
    }

    /// The checks that neither succeeded nor are still running — what `ci` repairs and
    /// what `failed` names.
    pub(crate) fn failing(&self) -> Vec<&Check> {
        self.rollup
            .iter()
            .filter(|c| {
                !matches!(
                    c.verdict.as_str(),
                    "SUCCESS"
                        | "NEUTRAL"
                        | "SKIPPED"
                        | "PENDING"
                        | "QUEUED"
                        | "IN_PROGRESS"
                        | "WAITING"
                        | "REQUESTED"
                        | "EXPECTED"
                        | ""
                )
            })
            .collect()
    }

    fn failed(&self) -> String {
        self.failing()
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Runs `gh pr view --json state,mergeable,statusCheckRollup` for `pr` and parses its
/// output. Err when `gh` cannot be run or exits non-zero.
pub(crate) fn view(pr: &str) -> Result<View, String> {
    let out = Command::new("gh")
        .args([
            "pr",
            "view",
            pr,
            "--json",
            "state,mergeable,statusCheckRollup",
        ])
        .output()
        .map_err(|e| format!("cannot run gh: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "gh pr view {pr}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// Parses a `gh pr view --json state,mergeable,statusCheckRollup` response. Err only
/// on malformed JSON; a response missing an expected field yields an empty default for
/// it rather than failing.
pub(crate) fn parse(json: &str) -> Result<View, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let string = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let rollup = value
        .get("statusCheckRollup")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    let field = |key: &str| {
                        entry
                            .get(key)
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                    };
                    let verdict = [field("conclusion"), field("state"), field("status")]
                        .into_iter()
                        .find(|v| !v.is_empty())
                        .unwrap_or_default()
                        .to_owned();
                    let name = [field("name"), field("context")]
                        .into_iter()
                        .find(|v| !v.is_empty())
                        .unwrap_or("check")
                        .to_owned();
                    let details_url = [field("detailsUrl"), field("targetUrl")]
                        .into_iter()
                        .find(|v| !v.is_empty())
                        .unwrap_or_default()
                        .to_owned();
                    Check {
                        name,
                        verdict,
                        details_url,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(View {
        state: string("state"),
        mergeable: string("mergeable"),
        rollup,
    })
}

fn line(pr: &str, outcome: &Outcome, state: &str, checks: Checks) -> String {
    let mut object = serde_json::json!({
        "pr": pr,
        "outcome": outcome.name(),
        "state": state,
        "checks": checks.name(),
    });
    if let Outcome::Blocked(reason) = outcome {
        object["reason"] = serde_json::Value::String(reason.clone());
    }
    object.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(state: &str, mergeable: &str, rollup: &[(&str, &str)]) -> View {
        View {
            state: state.into(),
            mergeable: mergeable.into(),
            rollup: rollup
                .iter()
                .map(|(name, verdict)| Check {
                    name: (*name).into(),
                    verdict: (*verdict).into(),
                    details_url: String::new(),
                })
                .collect(),
        }
    }

    // The two shapes `statusCheckRollup` returns — a check run's conclusion/status and a
    // commit status's state — decide whether the coordinator may proceed, so reading the
    // wrong field would merge on a red build.
    #[test]
    fn rollup_reads_both_check_shapes() {
        let v = parse(
            r#"{"state":"OPEN","mergeable":"MERGEABLE","statusCheckRollup":[
                {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"},
                {"__typename":"StatusContext","context":"ci/legacy","state":"SUCCESS"}]}"#,
        )
        .unwrap();
        assert_eq!(v.checks(), Checks::Failing);
        assert_eq!(v.failed(), "test");
        let running = parse(
            r#"{"state":"OPEN","mergeable":"MERGEABLE","statusCheckRollup":[
                {"__typename":"CheckRun","name":"test","status":"IN_PROGRESS","conclusion":""}]}"#,
        )
        .unwrap();
        assert_eq!(running.checks(), Checks::Pending);
    }

    // A repo with no CI must not deadlock the list: every hobby repo but a handful runs
    // no checks at all.
    #[test]
    fn no_checks_counts_as_green() {
        let v = view("OPEN", "MERGEABLE", &[]);
        assert_eq!(v.checks(), Checks::None);
        assert_eq!(v.resolve(v.checks(), true), Some(Outcome::Mergeable));
    }

    // Without `--ready` a green PR is waited on rather than reported: merging is mine
    // unless the coordinator was told otherwise, and a `mergeable` line would end the wait.
    #[test]
    fn green_waits_unless_ready() {
        let v = view("OPEN", "MERGEABLE", &[("test", "SUCCESS")]);
        assert_eq!(v.resolve(v.checks(), false), None);
        assert_eq!(v.resolve(v.checks(), true), Some(Outcome::Mergeable));
    }

    // The three states a person has to settle, each reported rather than waited on: a
    // coordinator blocked on one of these would otherwise hold the list until the timeout
    // and then hold it again.
    #[test]
    fn failures_are_reported_not_waited_on() {
        let red = view("OPEN", "MERGEABLE", &[("build", "FAILURE")]);
        assert_eq!(
            red.resolve(red.checks(), false),
            Some(Outcome::Blocked("checks failing: build".into()))
        );
        let dirty = view("OPEN", "CONFLICTING", &[("build", "SUCCESS")]);
        assert_eq!(
            dirty.resolve(dirty.checks(), false),
            Some(Outcome::Blocked("conflicts with the base branch".into()))
        );
        let closed = view("CLOSED", "UNKNOWN", &[]);
        assert_eq!(
            closed.resolve(closed.checks(), false),
            Some(Outcome::Blocked("closed without merging".into()))
        );
    }

    // A merged PR is merged whatever the checks say — a branch protection that let it
    // through is not the coordinator's to re-litigate.
    #[test]
    fn merged_outranks_the_checks() {
        let v = view("MERGED", "UNKNOWN", &[("build", "FAILURE")]);
        assert_eq!(v.resolve(v.checks(), false), Some(Outcome::Merged));
    }

    // The coordinator switches on these strings from a prompt, so they are a contract.
    #[test]
    fn the_line_names_outcome_state_and_checks() {
        assert_eq!(
            line(
                "https://github.com/o/r/pull/7",
                &Outcome::Blocked("checks failing: build".into()),
                "OPEN",
                Checks::Failing
            ),
            r#"{"checks":"failing","outcome":"blocked","pr":"https://github.com/o/r/pull/7","reason":"checks failing: build","state":"OPEN"}"#
        );
    }
}
