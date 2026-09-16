//! Decide whether a task's own PR can be made green by repairing its diff, and enforce
//! the bound on how many attempts that gets before the fix loop reports back blocked.

use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use crate::merged::{self, Checks};

/// Matches the acceptance-repair cap `/task:auto`'s `verify` phase already uses, so the
/// pipeline has one bound rather than two (`business-context.md` `## fix-loop-bound`).
pub const DEFAULT_BOUND: u32 = 3;

const POLL: Duration = Duration::from_secs(15);

const FLAKE_VERDICTS: &[&str] = &["CANCELLED", "TIMED_OUT", "STARTUP_FAILURE", "STALE"];

const FLAKE_LOG_PATTERNS: &[&str] = &[
    "runner has received a shutdown signal",
    "lost communication with the server",
    "Connection reset by peer",
    "Could not resolve host",
    "503 Service Unavailable",
    "The operation was canceled",
];

const DECISION_LOG_PATTERNS: &[&str] = &[
    "Unable to update https://",
    "failed to select a version",
    "no matching package named",
    "?rev=",
    "failed to load source for dependency",
];

const LOG_SIGNATURE_NEEDLES: &[&str] =
    &["error", "Error", "error[", "FAILED", "panicked", "warning:"];

/// Which of the three non-diff failures this is, or that the diff itself is at fault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    NotMine,
    Decision,
    Flake,
    OwnDiff,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Case::NotMine => "not-mine",
            Case::Decision => "decision",
            Case::Flake => "flake",
            Case::OwnDiff => "own-diff",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Verdict {
    Fix,
    Rerun(String),
    Blocked(Case, String),
}

/// One failing check, with the log evidence `classify` reads.
pub struct Failed {
    pub name: String,
    pub verdict: String,
    pub log: String,
    pub run: String,
}

/// What `run` gathers before classifying: the PR's own failing checks and, only when
/// they might not be the diff's fault, the same checks on the base branch's tip.
pub struct Evidence {
    pub failing: Vec<Failed>,
    pub base_failing: Vec<String>,
}

struct Report {
    outcome: &'static str,
    case: Option<Case>,
    reason: Option<String>,
}

fn classify(evidence: &Evidence) -> Verdict {
    for f in &evidence.failing {
        if evidence.base_failing.iter().any(|b| b == &f.name) {
            return Verdict::Blocked(
                Case::NotMine,
                format!("{} is already failing on the base branch", f.name),
            );
        }
    }
    for f in &evidence.failing {
        if FLAKE_VERDICTS.contains(&f.verdict.as_str())
            || FLAKE_LOG_PATTERNS.iter().any(|p| f.log.contains(p))
        {
            return Verdict::Rerun(format!("{} looks like an infrastructure flake", f.name));
        }
    }
    for f in &evidence.failing {
        if DECISION_LOG_PATTERNS.iter().any(|p| f.log.contains(p)) {
            return Verdict::Blocked(
                Case::Decision,
                format!(
                    "{} fails on a pinned revision, which is a project decision",
                    f.name
                ),
            );
        }
    }
    Verdict::Fix
}

fn resolve(verdict: Verdict, spent: u32, bound: u32) -> Report {
    match verdict {
        Verdict::Fix if spent < bound => Report {
            outcome: "fix",
            case: Some(Case::OwnDiff),
            reason: None,
        },
        Verdict::Fix => Report {
            outcome: "blocked",
            case: Some(Case::OwnDiff),
            reason: Some(format!(
                "{bound} fix attempts spent and the checks are still red"
            )),
        },
        Verdict::Rerun(reason) if spent < bound => Report {
            outcome: "rerun",
            case: Some(Case::Flake),
            reason: Some(reason),
        },
        Verdict::Rerun(reason) => Report {
            outcome: "blocked",
            case: Some(Case::Flake),
            reason: Some(reason),
        },
        Verdict::Blocked(case, reason) => Report {
            outcome: "blocked",
            case: Some(case),
            reason: Some(reason),
        },
    }
}

fn excerpt(log: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let signature: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| LOG_SIGNATURE_NEEDLES.iter().any(|n| l.contains(n)))
        .collect();
    let chosen: &[&str] = if signature.is_empty() { &lines } else { &signature };
    let start = chosen.len().saturating_sub(max_lines);
    let joined = chosen[start..].join("\n");
    let char_count = joined.chars().count();
    if char_count > 2000 {
        joined.chars().skip(char_count - 2000).collect()
    } else {
        joined
    }
}

fn repo_from_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_owned(), repo.to_owned()))
}

fn ids_from_details_url(url: &str) -> Option<(String, String)> {
    let marker = "/actions/runs/";
    let rest = &url[url.find(marker)? + marker.len()..];
    let mut segments = rest.split('/');
    let run = segments.next()?;
    if segments.next()? != "job" {
        return None;
    }
    let job = segments
        .next()?
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    if run.is_empty() || job.is_empty() {
        return None;
    }
    Some((run.to_owned(), job.to_owned()))
}

fn parse_base_checks(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .get("check_runs")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|e| {
                    let status = e
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    let conclusion = e
                        .get("conclusion")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    status == "completed" && !matches!(conclusion, "success" | "neutral" | "skipped")
                })
                .filter_map(|e| {
                    e.get("name")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn gh_stdout(args: &[&str]) -> Option<String> {
    let out = Command::new("gh").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn job_log(job: &str) -> String {
    gh_stdout(&["run", "view", "--job", job, "--log"]).unwrap_or_default()
}

fn base_failing_checks(pr: &str) -> Vec<String> {
    let Some(pr_json) = gh_stdout(&["pr", "view", pr, "--json", "url,baseRefName"]) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&pr_json) else {
        return Vec::new();
    };
    let url = value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let base = value
        .get("baseRefName")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let Some((owner, repo)) = repo_from_url(url) else {
        return Vec::new();
    };
    if base.is_empty() {
        return Vec::new();
    }
    let endpoint = format!("repos/{owner}/{repo}/commits/{base}/check-runs");
    match gh_stdout(&["api", &endpoint]) {
        Some(json) => parse_base_checks(&json),
        None => Vec::new(),
    }
}

fn gather_evidence(pr: &str, view: &merged::View) -> Evidence {
    let failing = view
        .failing()
        .into_iter()
        .map(|c| {
            let (run, log) = match ids_from_details_url(&c.details_url) {
                Some((run, job)) => (run, excerpt(&job_log(&job), 20)),
                None => (String::new(), String::new()),
            };
            Failed {
                name: c.name.clone(),
                verdict: c.verdict.clone(),
                log,
                run,
            }
        })
        .collect();
    Evidence {
        failing,
        base_failing: base_failing_checks(pr),
    }
}

fn line(pr: &str, outcome: &str, extra: serde_json::Value, spent: u32, bound: u32) -> String {
    let mut object = serde_json::json!({
        "pr": pr,
        "outcome": outcome,
        "spent": spent,
        "bound": bound,
    });
    if let serde_json::Value::Object(fields) = extra {
        for (k, v) in fields {
            object[k] = v;
        }
    }
    object.to_string()
}

/// Poll the PR's checks until they resolve, then decide what happens next: `hcoord ci`
/// is the fix loop's eyes, separating the three failures that are not this diff's own
/// from the one that is, and applying the bound so a session that loses count still
/// stops. Prints one JSON line and exits 0, except when `merged::view` itself fails (1)
/// or the checks never resolve within the timeout (2) — the same split `hcoord merged`
/// uses. A repo with no checks is green here exactly as it is in `merged`: anything else
/// would stop every PR in a repo with no CI, this one included.
pub fn run(pr: &str, spent: u32, bound: u32, timeout_secs: u64) -> ExitCode {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let view = match merged::view(pr) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("hcoord ci: {e}");
                return ExitCode::from(1);
            }
        };
        match view.checks() {
            Checks::Pending => {
                if Instant::now() >= deadline {
                    println!(
                        "{}",
                        line(pr, "timeout", serde_json::json!({}), spent, bound)
                    );
                    return ExitCode::from(2);
                }
                std::thread::sleep(POLL);
            }
            Checks::Green => {
                println!(
                    "{}",
                    line(
                        pr,
                        "green",
                        serde_json::json!({"checks": "green"}),
                        spent,
                        bound
                    )
                );
                return ExitCode::SUCCESS;
            }
            Checks::None => {
                println!(
                    "{}",
                    line(
                        pr,
                        "green",
                        serde_json::json!({"checks": "none"}),
                        spent,
                        bound
                    )
                );
                return ExitCode::SUCCESS;
            }
            Checks::Failing => {
                let evidence = gather_evidence(pr, &view);
                let verdict = classify(&evidence);
                let report = resolve(verdict, spent, bound);
                let first = evidence.failing.first();
                let mut extra = serde_json::json!({});
                if let Some(case) = report.case {
                    extra["case"] = serde_json::Value::String(case.name().to_owned());
                }
                if let Some(reason) = &report.reason {
                    extra["reason"] = serde_json::Value::String(reason.clone());
                }
                if let Some(f) = first {
                    extra["check"] = serde_json::Value::String(f.name.clone());
                    extra["run"] = serde_json::Value::String(f.run.clone());
                    extra["log"] = serde_json::Value::String(f.log.clone());
                }
                println!("{}", line(pr, report.outcome, extra, spent, bound));
                return ExitCode::SUCCESS;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(name: &str, verdict: &str, log: &str) -> Failed {
        Failed {
            name: name.into(),
            verdict: verdict.into(),
            log: log.into(),
            run: "1".into(),
        }
    }

    // A check already red on the base branch is not this diff's doing, whatever its log
    // says — the fact has to outrank every heuristic below it.
    #[test]
    fn base_failure_blocks_as_not_mine() {
        let evidence = Evidence {
            failing: vec![failed("clippy", "FAILURE", "error: pinned rev broken")],
            base_failing: vec!["clippy".into()],
        };
        assert_eq!(
            classify(&evidence),
            Verdict::Blocked(
                Case::NotMine,
                "clippy is already failing on the base branch".into()
            )
        );
    }

    // A cancelled run's log describes the runner dying, not the diff, so infrastructure
    // must outrank a coincidental match against the decision patterns.
    #[test]
    fn cancelled_run_reruns_before_decision_patterns_are_checked() {
        let evidence = Evidence {
            failing: vec![failed(
                "build",
                "CANCELLED",
                "Unable to update https://github.com/x/y?rev=abc",
            )],
            base_failing: vec![],
        };
        assert_eq!(
            classify(&evidence),
            Verdict::Rerun("build looks like an infrastructure flake".into())
        );
    }

    // A pinned-revision failure is a project decision, not a repair the session can make.
    #[test]
    fn pinned_revision_log_blocks_as_decision() {
        let evidence = Evidence {
            failing: vec![failed(
                "bevy_lint",
                "FAILURE",
                "error: failed to select a version for `bevy`\nno matching package named `bevy_cli`",
            )],
            base_failing: vec![],
        };
        assert_eq!(
            classify(&evidence),
            Verdict::Blocked(
                Case::Decision,
                "bevy_lint fails on a pinned revision, which is a project decision".into()
            )
        );
    }

    // Anything left over is this diff's own to fix.
    #[test]
    fn ordinary_failure_is_fix() {
        let evidence = Evidence {
            failing: vec![failed("test", "FAILURE", "assertion failed: left == right")],
            base_failing: vec![],
        };
        assert_eq!(classify(&evidence), Verdict::Fix);
    }

    // `NotMine` and `Decision` never resolve by trying again, so the bound must not
    // apply to them — reporting them at spent=0 has to look identical to spent=99.
    #[test]
    fn not_mine_and_decision_ignore_the_bound() {
        let r = resolve(Verdict::Blocked(Case::NotMine, "x".into()), 0, 3);
        assert_eq!(r.outcome, "blocked");
        let r = resolve(Verdict::Blocked(Case::Decision, "y".into()), 99, 3);
        assert_eq!(r.outcome, "blocked");
    }

    // A bound that fired at `spent == bound - 1` would stop the loop one attempt early.
    #[test]
    fn fix_is_allowed_up_to_the_bound() {
        let r = resolve(Verdict::Fix, 2, 3);
        assert_eq!(r.outcome, "fix");
        let r = resolve(Verdict::Fix, 3, 3);
        assert_eq!(r.outcome, "blocked");
        assert_eq!(r.case, Some(Case::OwnDiff));
    }

    // A flake is blocked from editing code, not from trying again, until the bound runs
    // out — only then does it stop being a rerun and become a report.
    #[test]
    fn flake_reruns_then_blocks_once_spent() {
        let r = resolve(Verdict::Rerun("r".into()), 2, 3);
        assert_eq!(r.outcome, "rerun");
        let r = resolve(Verdict::Rerun("r".into()), 3, 3);
        assert_eq!(r.outcome, "blocked");
        assert_eq!(r.case, Some(Case::Flake));
    }

    // The excerpt has to find the signature lines buried in an otherwise quiet log,
    // not just the tail of the file.
    #[test]
    fn excerpt_prefers_signature_lines() {
        let log = "note: compiling\nerror: it broke\nnote: done\n";
        assert_eq!(excerpt(log, 20), "error: it broke");
    }

    // With no signature line at all, the tail is the only evidence there is.
    #[test]
    fn excerpt_falls_back_to_the_tail() {
        let log = "one\ntwo\nthree\n";
        assert_eq!(excerpt(log, 2), "two\nthree");
    }

    // A wrong owner or repo sends the base-branch check-runs call to the wrong
    // repository entirely, so `not-mine` would silently never fire.
    #[test]
    fn repo_from_url_splits_owner_and_repo() {
        assert_eq!(
            repo_from_url("https://github.com/julianbieber/hobby-tooling/pull/49"),
            Some(("julianbieber".into(), "hobby-tooling".into()))
        );
        assert_eq!(repo_from_url("not a github url"), None);
    }

    // GitHub appends a query string to a check's details URL, and a job id that
    // still carried it would make `gh run view --job` fail to find the job.
    #[test]
    fn ids_from_details_url_reads_run_and_job() {
        assert_eq!(
            ids_from_details_url(
                "https://github.com/o/r/actions/runs/12/job/34?check_suite_focus=true"
            ),
            Some(("12".into(), "34".into()))
        );
        assert_eq!(ids_from_details_url("https://example.com/status/x"), None);
    }

    // A still-running check on the base branch is not evidence of anything yet, and
    // counting it as failing would misclassify an ordinary diff as `not-mine`.
    #[test]
    fn parse_base_checks_keeps_only_concluded_failures() {
        let json = r#"{"check_runs":[
            {"name":"clippy","status":"completed","conclusion":"failure"},
            {"name":"fmt","status":"completed","conclusion":"success"},
            {"name":"slow","status":"in_progress","conclusion":null}
        ]}"#;
        assert_eq!(parse_base_checks(json), vec!["clippy".to_string()]);
    }
}
