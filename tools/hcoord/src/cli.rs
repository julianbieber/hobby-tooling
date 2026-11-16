//! The `hcoord` command line: the arguments read into one subcommand, the subcommand
//! run, and its result written where the calling skill reads it.

use std::io;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::output::Output;
use crate::{capture, ci, goal, merged, model, seed, tui, wait};

const USAGE: &str = "\
usage:
  hcoord tui <coordinator notes dir>            show tasks.json, redrawn as it changes
  hcoord wait <task notes dir> [--timeout <s>] [--after <phase>]
                                                block until build-task.json needs a human or is done;
                                                --after keeps waiting while the phase is still <phase>
  hcoord merged <pr url> [--timeout <s>] [--ready]
                                                block until the pull request is merged or something
                                                blocks it; --ready also returns once it could be merged
  hcoord ci <pr url> [--spent <n>] [--bound <n>] [--timeout <s>]
                                                classify the pr's failing checks: fix, rerun, or
                                                blocked naming which of the three cases it is
  hcoord status <coordinator notes dir>         the counts, then each unfinished task's
                                                phase and how long it has sat there
  hcoord goal lint <goal.md> [--tasks <tasks.json>]
                                                check a goal file, and with --tasks that every
                                                scenario is owned by exactly one task
  hcoord goal show <goal.md> <id>...            print the named scenarios verbatim
  hcoord capture <task notes dir> <scenario> --window <name> | --text <file>
                                                write screens/<scenario>.png for a hand scenario:
                                                an X11 window, or a file (- for stdin) rendered as text
  hcoord seed <coordinator notes dir> <task> [--force]
                                                write the task's build-task.json and
                                                business-context.md from tasks.json and its goal

`wait` prints one JSON line {phase, outcome}, plus {acceptance} when the run recorded one
and {phase_since, written, unchanged_for} when the state file recorded them, and exits 0
when a state was reached, 2 on timeout, 1 when the file never became readable. `merged`
prints one JSON line {pr, outcome, state, checks} with the same exit codes, 1 when `gh`
fails. `ci` prints one JSON line {pr, outcome, spent, bound}, plus {checks} when green
and {case, reason, check, run, log} when the checks are failing, with the same exit
codes as `merged`.";

/// One subcommand with its arguments read and checked.
#[derive(Debug)]
pub enum Command {
    Tui {
        dir: PathBuf,
    },
    Wait {
        dir: PathBuf,
        timeout_secs: u64,
        after: Option<String>,
    },
    Merged {
        pr: String,
        timeout_secs: u64,
        ready: bool,
    },
    Ci {
        pr: String,
        spent: u32,
        bound: u32,
        timeout_secs: u64,
    },
    Status {
        dir: PathBuf,
    },
    GoalLint {
        goal: PathBuf,
        tasks: Option<PathBuf>,
    },
    GoalShow {
        goal: PathBuf,
        ids: Vec<String>,
    },
    Capture {
        dir: PathBuf,
        scenario: String,
        source: capture::Source,
    },
    Seed {
        dir: PathBuf,
        task: String,
        force: bool,
    },
}

/// A flag after a subcommand's positional arguments that could not be read.
#[derive(Debug, thiserror::Error)]
pub enum FlagError {
    #[error("{0} wants a value")]
    MissingValue(String),
    #[error("unexpected argument: {0}")]
    Unexpected(String),
    #[error("{flag} wants {unit}, got {value:?}")]
    NotANumber {
        flag: &'static str,
        unit: &'static str,
        value: String,
        #[source]
        source: ParseIntError,
    },
    #[error("give one of --window or --text, not both")]
    TwoSources,
    #[error("give --window <name> or --text <file>")]
    NoSource,
}

/// Arguments that name no subcommand, or name one with flags it does not take.
#[derive(Debug, thiserror::Error)]
pub enum ArgsError {
    #[error("{usage}", usage = USAGE)]
    Usage,
    #[error("{0}\n\n{usage}", usage = USAGE)]
    GoalFlags(#[source] FlagError),
    #[error("hcoord {command}: {source}")]
    Flags {
        command: &'static str,
        #[source]
        source: FlagError,
    },
}

/// A subcommand that ran and failed, prefixed with the subcommand it came from.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("hcoord tui: {0}")]
    Tui(#[source] io::Error),
    #[error("hcoord wait: {0}")]
    Wait(#[source] model::LoadError),
    #[error("hcoord merged: {0}")]
    Merged(#[source] merged::ViewError),
    #[error("hcoord ci: {0}")]
    Ci(#[source] merged::ViewError),
    #[error("hcoord status: {0}")]
    Status(#[source] model::LoadError),
    #[error(transparent)]
    Goal(goal::RunError),
    #[error("hcoord capture: {0}")]
    Capture(#[source] capture::CaptureError),
    #[error("hcoord seed: {0}")]
    Seed(#[source] seed::RunError),
}

impl Command {
    /// Read the arguments after the program name.
    pub fn parse(args: &[String]) -> Result<Self, ArgsError> {
        let flags =
            |command: &'static str| move |source: FlagError| ArgsError::Flags { command, source };
        match args {
            [cmd, dir, ..] if cmd == "tui" => Ok(Self::Tui {
                dir: PathBuf::from(dir),
            }),
            [cmd, dir, rest @ ..] if cmd == "wait" => {
                let (timeout_secs, after) = parse_wait_flags(rest).map_err(flags("wait"))?;
                Ok(Self::Wait {
                    dir: PathBuf::from(dir),
                    timeout_secs,
                    after,
                })
            }
            [cmd, pr, rest @ ..] if cmd == "merged" => {
                let (timeout_secs, ready) = parse_merged_flags(rest).map_err(flags("merged"))?;
                Ok(Self::Merged {
                    pr: pr.clone(),
                    timeout_secs,
                    ready,
                })
            }
            [cmd, pr, rest @ ..] if cmd == "ci" => {
                let (spent, bound, timeout_secs) = parse_ci_flags(rest).map_err(flags("ci"))?;
                Ok(Self::Ci {
                    pr: pr.clone(),
                    spent,
                    bound,
                    timeout_secs,
                })
            }
            [cmd, dir, ..] if cmd == "status" => Ok(Self::Status {
                dir: PathBuf::from(dir),
            }),
            [cmd, sub, path, rest @ ..] if cmd == "goal" && sub == "lint" => Ok(Self::GoalLint {
                goal: PathBuf::from(path),
                tasks: parse_goal_flags(rest).map_err(ArgsError::GoalFlags)?,
            }),
            [cmd, sub, path, ids @ ..] if cmd == "goal" && sub == "show" && !ids.is_empty() => {
                Ok(Self::GoalShow {
                    goal: PathBuf::from(path),
                    ids: ids.to_vec(),
                })
            }
            [cmd, dir, scenario, rest @ ..] if cmd == "capture" => Ok(Self::Capture {
                dir: PathBuf::from(dir),
                scenario: scenario.clone(),
                source: parse_capture_flags(rest).map_err(flags("capture"))?,
            }),
            [cmd, dir, task, rest @ ..] if cmd == "seed" => Ok(Self::Seed {
                dir: PathBuf::from(dir),
                task: task.clone(),
                force: parse_seed_flags(rest).map_err(flags("seed"))?,
            }),
            _ => Err(ArgsError::Usage),
        }
    }

    /// Run the subcommand to its result. The polling subcommands block here until
    /// their state resolves or their timeout passes.
    pub fn run(self) -> Result<Output, RunError> {
        match self {
            Self::Tui { dir } => {
                tui::run(&dir).map_err(RunError::Tui)?;
                Ok(Output {
                    stdout: String::new(),
                    code: ExitCode::SUCCESS,
                })
            }
            Self::Wait {
                dir,
                timeout_secs,
                after,
            } => wait::run(&dir, timeout_secs, after.as_deref()).map_err(RunError::Wait),
            Self::Merged {
                pr,
                timeout_secs,
                ready,
            } => merged::run(&pr, timeout_secs, ready).map_err(RunError::Merged),
            Self::Ci {
                pr,
                spent,
                bound,
                timeout_secs,
            } => ci::run(&pr, spent, bound, timeout_secs).map_err(RunError::Ci),
            Self::Status { dir } => status(&dir).map_err(RunError::Status),
            Self::GoalLint { goal, tasks } => {
                goal::run_lint(&goal, tasks.as_deref()).map_err(RunError::Goal)
            }
            Self::GoalShow { goal, ids } => goal::run_show(&goal, &ids).map_err(RunError::Goal),
            Self::Capture {
                dir,
                scenario,
                source,
            } => capture::run(&dir, &scenario, source).map_err(RunError::Capture),
            Self::Seed { dir, task, force } => {
                seed::run(&dir, &task, force).map_err(RunError::Seed)
            }
        }
    }
}

/// Run the subcommand `args` name and write its result: its text to standard output,
/// and its failure, or a usage message, as an error event to the installed `tracing`
/// subscriber.
pub fn run(args: &[String]) -> ExitCode {
    let command = match Command::parse(args) {
        Ok(command) => command,
        Err(e) => return fail(&e, "cannot read the arguments"),
    };
    match command.run() {
        Ok(output) => {
            print!("{}", output.stdout);
            output.code
        }
        Err(e) => fail(&e, "subcommand failed"),
    }
}

fn status(dir: &std::path::Path) -> Result<Output, model::LoadError> {
    let coordinator = model::Coordinator::load(dir)?;
    let mut stdout = format!("{}\n", coordinator.counts().summary());
    for line in coordinator.phase_lines(dir) {
        stdout.push_str(&line);
        stdout.push('\n');
    }
    Ok(Output {
        stdout,
        code: ExitCode::SUCCESS,
    })
}

fn fail(error: &(dyn std::error::Error + 'static), what: &str) -> ExitCode {
    tracing::error!(error, "{what}");
    ExitCode::from(1)
}

fn parse_goal_flags(rest: &[String]) -> Result<Option<PathBuf>, FlagError> {
    let mut tasks = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--tasks" => {
                let value = it
                    .next()
                    .ok_or_else(|| FlagError::MissingValue(flag.clone()))?;
                tasks = Some(PathBuf::from(value));
            }
            other => return Err(FlagError::Unexpected(other.to_owned())),
        }
    }
    Ok(tasks)
}

fn parse_capture_flags(rest: &[String]) -> Result<capture::Source, FlagError> {
    let mut source = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| FlagError::MissingValue(flag.clone()))?;
        let next = match flag.as_str() {
            "--window" => capture::Source::Window(value.clone()),
            "--text" => capture::Source::Text(PathBuf::from(value)),
            other => return Err(FlagError::Unexpected(other.to_owned())),
        };
        if source.replace(next).is_some() {
            return Err(FlagError::TwoSources);
        }
    }
    source.ok_or(FlagError::NoSource)
}

fn parse_seed_flags(rest: &[String]) -> Result<bool, FlagError> {
    let mut force = false;
    for flag in rest {
        match flag.as_str() {
            "--force" => force = true,
            other => return Err(FlagError::Unexpected(other.to_owned())),
        }
    }
    Ok(force)
}

fn number<T: std::str::FromStr<Err = ParseIntError>>(
    flag: &'static str,
    unit: &'static str,
    value: &str,
) -> Result<T, FlagError> {
    value.parse().map_err(|source| FlagError::NotANumber {
        flag,
        unit,
        value: value.to_owned(),
        source,
    })
}

fn parse_wait_flags(rest: &[String]) -> Result<(u64, Option<String>), FlagError> {
    let mut timeout = wait::DEFAULT_TIMEOUT_SECS;
    let mut after = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| FlagError::MissingValue(flag.clone()))?;
        match flag.as_str() {
            "--timeout" => timeout = number("--timeout", "seconds", value)?,
            "--after" => after = Some(value.clone()),
            other => return Err(FlagError::Unexpected(other.to_owned())),
        }
    }
    Ok((timeout, after))
}

fn parse_merged_flags(rest: &[String]) -> Result<(u64, bool), FlagError> {
    let mut timeout = merged::DEFAULT_TIMEOUT_SECS;
    let mut ready = false;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--ready" => ready = true,
            "--timeout" => {
                let value = it
                    .next()
                    .ok_or_else(|| FlagError::MissingValue(flag.clone()))?;
                timeout = number("--timeout", "seconds", value)?;
            }
            other => return Err(FlagError::Unexpected(other.to_owned())),
        }
    }
    Ok((timeout, ready))
}

fn parse_ci_flags(rest: &[String]) -> Result<(u32, u32, u64), FlagError> {
    let mut spent = 0u32;
    let mut bound = ci::DEFAULT_BOUND;
    let mut timeout = merged::DEFAULT_TIMEOUT_SECS;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| FlagError::MissingValue(flag.clone()))?;
        match flag.as_str() {
            "--spent" => spent = number("--spent", "a number", value)?,
            "--bound" => bound = number("--bound", "a number", value)?,
            "--timeout" => timeout = number("--timeout", "seconds", value)?,
            other => return Err(FlagError::Unexpected(other.to_owned())),
        }
    }
    Ok((spent, bound, timeout))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The coordinator spells these flags from a prompt, so the accepted forms are a
    // contract with the skill rather than a convenience.
    #[test]
    fn wait_flags_parse_in_any_order() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_wait_flags(&args(&[])).unwrap(),
            (wait::DEFAULT_TIMEOUT_SECS, None)
        );
        assert_eq!(
            parse_wait_flags(&args(&["--after", "await-play", "--timeout", "30"])).unwrap(),
            (30, Some("await-play".into()))
        );
        assert!(parse_wait_flags(&args(&["--timeout"])).is_err());
        assert!(parse_wait_flags(&args(&["--bogus", "x"])).is_err());
    }

    // `--ready` is what decides whether the coordinator merges or waits for me to, so a
    // flag silently swallowed as a value would hand the merge to the wrong one of us.
    #[test]
    fn merged_flags_parse_ready_as_a_switch() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_merged_flags(&args(&[])).unwrap(),
            (merged::DEFAULT_TIMEOUT_SECS, false)
        );
        assert_eq!(
            parse_merged_flags(&args(&["--ready", "--timeout", "30"])).unwrap(),
            (30, true)
        );
        assert_eq!(
            parse_merged_flags(&args(&["--timeout", "30", "--ready"])).unwrap(),
            (30, true)
        );
        assert!(parse_merged_flags(&args(&["--timeout"])).is_err());
        assert!(parse_merged_flags(&args(&["--bogus"])).is_err());
    }

    // `--force` is the one way to overwrite a seeded task, so anything else after the
    // task must be refused rather than read as consent.
    #[test]
    fn seed_flags_accept_only_force() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(!parse_seed_flags(&args(&[])).unwrap());
        assert!(parse_seed_flags(&args(&["--force"])).unwrap());
        assert!(parse_seed_flags(&args(&["--frce"])).is_err());
    }

    // A capture from two sources at once, or from none, would write a picture of
    // something the caller did not name, so both must be usage errors.
    #[test]
    fn capture_flags_want_exactly_one_source() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(matches!(
            parse_capture_flags(&args(&["--window", "app"])),
            Ok(capture::Source::Window(name)) if name == "app"
        ));
        assert!(matches!(
            parse_capture_flags(&args(&["--text", "-"])),
            Ok(capture::Source::Text(path)) if path.as_os_str() == "-"
        ));
        assert!(parse_capture_flags(&args(&[])).is_err());
        assert!(parse_capture_flags(&args(&["--window", "a", "--text", "b"])).is_err());
        assert!(parse_capture_flags(&args(&["--window"])).is_err());
    }

    // The `fix-ci` phase spells `--spent`/`--bound` from `ci_result.rounds` on every
    // call, so these are a contract with the skill rather than a convenience.
    #[test]
    fn ci_flags_default_and_parse() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_ci_flags(&args(&[])).unwrap(),
            (0, ci::DEFAULT_BOUND, merged::DEFAULT_TIMEOUT_SECS)
        );
        assert_eq!(
            parse_ci_flags(&args(&["--spent", "2", "--bound", "3", "--timeout", "30"])).unwrap(),
            (2, 3, 30)
        );
        assert!(parse_ci_flags(&args(&["--spent"])).is_err());
        assert!(parse_ci_flags(&args(&["--bogus", "1"])).is_err());
    }
}
