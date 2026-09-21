//! `hcoord` — what a coordinator session needs from outside the Claude pane: a live
//! view of its `tasks.json` in the pane next to it, a way to block on one task's
//! pipeline state and on its pull request instead of polling them from a prompt, and
//! the seeding of a task's brief from the goal file its list was split from, the
//! capture of the screen a `hand` scenario names, and the judgement of a finished run
//! against that goal.

mod capture;
mod ci;
mod clock;
mod goal;
mod judge;
mod merged;
mod model;
mod seed;
mod tui;
mod wait;

use std::path::PathBuf;
use std::process::ExitCode;

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
  hcoord judge list <coordinator notes dir>     the scenarios to judge, hand ones first
  hcoord judge show <coordinator notes dir> <scenario>
                                                one scenario verbatim, its screens and its capture
  hcoord judge record <coordinator notes dir> <scenario> <ok|wrong|tune>
            [--words <file|->] [--follow-up <text>]
                                                write the verdict into judgement.md

`wait` prints one JSON line {phase, outcome}, plus {acceptance} when the run recorded one
and {phase_since, written, unchanged_for} when the state file recorded them, and exits 0
when a state was reached, 2 on timeout, 1 when the file never became readable. `merged`
prints one JSON line {pr, outcome, state, checks} with the same exit codes, 1 when `gh`
fails. `ci` prints one JSON line {pr, outcome, spent, bound}, plus {checks} when green
and {case, reason, check, run, log} when the checks are failing, with the same exit
codes as `merged`.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        eprintln!("{USAGE}");
        return ExitCode::from(1);
    };
    match cmd {
        "tui" => match args.get(1) {
            Some(dir) => match tui::run(&PathBuf::from(dir)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("hcoord tui: {e}");
                    ExitCode::from(1)
                }
            },
            None => usage(),
        },
        "wait" => match args.get(1) {
            Some(dir) => match parse_wait_flags(&args[2..]) {
                Ok((timeout, after)) => wait::run(&PathBuf::from(dir), timeout, after.as_deref()),
                Err(e) => {
                    eprintln!("hcoord wait: {e}");
                    ExitCode::from(1)
                }
            },
            None => usage(),
        },
        "merged" => match args.get(1) {
            Some(pr) => match parse_merged_flags(&args[2..]) {
                Ok((timeout, ready)) => merged::run(pr, timeout, ready),
                Err(e) => {
                    eprintln!("hcoord merged: {e}");
                    ExitCode::from(1)
                }
            },
            None => usage(),
        },
        "ci" => match args.get(1) {
            Some(pr) => match parse_ci_flags(&args[2..]) {
                Ok((spent, bound, timeout)) => ci::run(pr, spent, bound, timeout),
                Err(e) => {
                    eprintln!("hcoord ci: {e}");
                    ExitCode::from(1)
                }
            },
            None => usage(),
        },
        "status" => match args.get(1) {
            Some(dir) => {
                let dir = PathBuf::from(dir);
                match model::Coordinator::load(&dir) {
                    Ok(c) => {
                        println!("{}", c.counts().summary());
                        for line in c.phase_lines(&dir) {
                            println!("{line}");
                        }
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("hcoord status: {e}");
                        ExitCode::from(1)
                    }
                }
            }
            None => usage(),
        },
        "goal" => match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("lint"), Some(path)) => match parse_goal_flags(&args[3..]) {
                Ok(tasks) => goal::run_lint(&PathBuf::from(path), tasks.as_deref()),
                Err(e) => {
                    eprintln!("{e}\n\n{USAGE}");
                    ExitCode::from(1)
                }
            },
            (Some("show"), Some(path)) if args.len() > 3 => {
                goal::run_show(&PathBuf::from(path), &args[3..])
            }
            _ => usage(),
        },
        "capture" => match (args.get(1), args.get(2)) {
            (Some(dir), Some(scenario)) => match parse_capture_flags(&args[3..]) {
                Ok(source) => capture::run(&PathBuf::from(dir), scenario, source),
                Err(e) => {
                    eprintln!("hcoord capture: {e}");
                    ExitCode::from(1)
                }
            },
            _ => usage(),
        },
        "seed" => match (args.get(1), args.get(2)) {
            (Some(dir), Some(task)) => match parse_seed_flags(&args[3..]) {
                Ok(force) => seed::run(&PathBuf::from(dir), task, force),
                Err(e) => {
                    eprintln!("hcoord seed: {e}");
                    ExitCode::from(1)
                }
            },
            _ => usage(),
        },
        "judge" => match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("list"), Some(dir)) if args.len() == 3 => judge::run_list(&PathBuf::from(dir)),
            (Some("show"), Some(dir)) if args.len() == 4 => {
                judge::run_show(&PathBuf::from(dir), &args[3])
            }
            (Some("record"), Some(dir)) if args.len() > 4 => match parse_judge_flags(&args[5..]) {
                Ok((words, follow_up)) => judge::run_record(
                    &PathBuf::from(dir),
                    &args[3],
                    &args[4],
                    words.as_deref(),
                    follow_up.as_deref(),
                ),
                Err(e) => {
                    eprintln!("hcoord judge: {e}");
                    ExitCode::from(1)
                }
            },
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(1)
}

fn parse_goal_flags(rest: &[String]) -> Result<Option<PathBuf>, String> {
    let mut tasks = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--tasks" => {
                let value = it.next().ok_or_else(|| format!("{flag} wants a value"))?;
                tasks = Some(PathBuf::from(value));
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok(tasks)
}

fn parse_capture_flags(rest: &[String]) -> Result<capture::Source, String> {
    let mut source = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} wants a value"))?;
        let next = match flag.as_str() {
            "--window" => capture::Source::Window(value.clone()),
            "--text" => capture::Source::Text(PathBuf::from(value)),
            other => return Err(format!("unexpected argument: {other}")),
        };
        if source.replace(next).is_some() {
            return Err("give one of --window or --text, not both".to_owned());
        }
    }
    source.ok_or_else(|| "give --window <name> or --text <file>".to_owned())
}

fn parse_judge_flags(rest: &[String]) -> Result<(Option<PathBuf>, Option<String>), String> {
    let mut words = None;
    let mut follow_up = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} wants a value"))?;
        let repeated = match flag.as_str() {
            "--words" => words.replace(PathBuf::from(value)).is_some(),
            "--follow-up" => follow_up.replace(value.clone()).is_some(),
            other => return Err(format!("unexpected argument: {other}")),
        };
        if repeated {
            return Err(format!("{flag} given twice"));
        }
    }
    Ok((words, follow_up))
}

fn parse_seed_flags(rest: &[String]) -> Result<bool, String> {
    let mut force = false;
    for flag in rest {
        match flag.as_str() {
            "--force" => force = true,
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok(force)
}

fn parse_wait_flags(rest: &[String]) -> Result<(u64, Option<String>), String> {
    let mut timeout = wait::DEFAULT_TIMEOUT_SECS;
    let mut after = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} wants a value"))?;
        match flag.as_str() {
            "--timeout" => {
                timeout = value
                    .parse()
                    .map_err(|_| format!("--timeout wants seconds, got {value:?}"))?;
            }
            "--after" => after = Some(value.clone()),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok((timeout, after))
}

fn parse_merged_flags(rest: &[String]) -> Result<(u64, bool), String> {
    let mut timeout = merged::DEFAULT_TIMEOUT_SECS;
    let mut ready = false;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--ready" => ready = true,
            "--timeout" => {
                let value = it.next().ok_or("--timeout wants a value")?;
                timeout = value
                    .parse()
                    .map_err(|_| format!("--timeout wants seconds, got {value:?}"))?;
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok((timeout, ready))
}

fn parse_ci_flags(rest: &[String]) -> Result<(u32, u32, u64), String> {
    let mut spent = 0u32;
    let mut bound = ci::DEFAULT_BOUND;
    let mut timeout = merged::DEFAULT_TIMEOUT_SECS;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} wants a value"))?;
        match flag.as_str() {
            "--spent" => {
                spent = value
                    .parse()
                    .map_err(|_| format!("--spent wants a number, got {value:?}"))?;
            }
            "--bound" => {
                bound = value
                    .parse()
                    .map_err(|_| format!("--bound wants a number, got {value:?}"))?;
            }
            "--timeout" => {
                timeout = value
                    .parse()
                    .map_err(|_| format!("--timeout wants seconds, got {value:?}"))?;
            }
            other => return Err(format!("unexpected argument: {other}")),
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

    // `--words -` is how an answer reaches the file without a shell in between, so a
    // flag that lost its value or was given twice must be an error, not a silent default.
    #[test]
    fn judge_flags_parse_words_and_follow_up() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_judge_flags(&args(&[])).unwrap(), (None, None));
        assert_eq!(
            parse_judge_flags(&args(&["--words", "-", "--follow-up", "add it"])).unwrap(),
            (Some(PathBuf::from("-")), Some("add it".to_owned()))
        );
        assert_eq!(
            parse_judge_flags(&args(&["--follow-up", "add it", "--words", "w.txt"])).unwrap(),
            (Some(PathBuf::from("w.txt")), Some("add it".to_owned()))
        );
        assert!(parse_judge_flags(&args(&["--words"])).is_err());
        assert!(parse_judge_flags(&args(&["--bogus", "x"])).is_err());
        assert!(parse_judge_flags(&args(&["--words", "a", "--words", "b"])).is_err());
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
