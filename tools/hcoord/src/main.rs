//! `hcoord` — what a coordinator session needs from outside the Claude pane: a live
//! view of its `tasks.json` in the pane next to it, and a way to block on one task's
//! pipeline state instead of polling it from a prompt.

mod model;
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
  hcoord status <coordinator notes dir>         one line of counts, for scripts

`wait` prints one JSON line {phase, outcome} and exits 0 when a state was reached,
2 on timeout, 1 on a missing or unreadable file.";

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
        "status" => match args.get(1) {
            Some(dir) => match model::Coordinator::load(&PathBuf::from(dir)) {
                Ok(c) => {
                    println!("{}", c.counts().summary());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("hcoord status: {e}");
                    ExitCode::from(1)
                }
            },
            None => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(1)
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
}
