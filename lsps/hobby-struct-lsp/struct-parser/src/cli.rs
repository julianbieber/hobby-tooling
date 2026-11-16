//! The `hobby-struct-parser` command line: its arguments, and the report it
//! writes for one repo.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::ast::Verification;
use crate::span::LineIndex;
use crate::{
    Diagnostic, Measured, SPEC_FILE, Severity, Standard, check, has_errors, parse, ratchet,
};

/// The one-line usage a human is shown when the arguments do not parse.
pub const USAGE: &str =
    "usage: hobby-struct-parser [--lint|--detail|--ratchet|--json] <repo-or-file>";

/// What a run writes. `--json` wins over `--ratchet`, which wins over the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Every counted standard against its budget, then the reviewed ones and
    /// the diagnostics; `detail` also lists each count's offenders.
    Report {
        detail: bool,
    },
    Json,
    /// Rewrite the budgets the code has moved below, in place.
    Ratchet,
}

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// A repo or crate root holding [`SPEC_FILE`], or the file itself.
    pub target: PathBuf,
    pub output: Output,
    /// Fail on warnings as well as errors. A ratchet never fails.
    pub lint: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UsageError {
    #[error("unknown flag `{0}`")]
    UnknownFlag(String),
    #[error("no repo or file given")]
    MissingTarget,
    #[error("more than one repo or file given: `{0}`")]
    ExtraTarget(String),
}

impl Args {
    /// Parse the arguments after the program name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, UsageError> {
        let (mut lint, mut detail, mut do_ratchet, mut json) = (false, false, false, false);
        let mut target = None;
        for arg in args {
            match arg.as_str() {
                "--lint" => lint = true,
                "--detail" => detail = true,
                "--ratchet" => do_ratchet = true,
                "--json" => json = true,
                flag if flag.starts_with("--") => return Err(UsageError::UnknownFlag(arg)),
                _ if target.is_some() => return Err(UsageError::ExtraTarget(arg)),
                _ => target = Some(PathBuf::from(arg)),
            }
        }
        let output = if json {
            Output::Json
        } else if do_ratchet {
            Output::Ratchet
        } else {
            Output::Report { detail }
        };
        Ok(Args {
            target: target.ok_or(UsageError::MissingTarget)?,
            output,
            lint,
        })
    }
}

/// Whether the repo met its standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
}

/// A run that could not produce its verdict.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("no {SPEC_FILE} found at {}", .0.display())]
    NoSpec(PathBuf),
    #[error("could not read {}", path.display())]
    ReadSpec {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not write the tightened budgets to {}", path.display())]
    WriteSpec {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not serialize the diagnostics")]
    Serialize(#[source] serde_json::Error),
    #[error("could not write the report")]
    Report(#[source] io::Error),
}

/// Check the repo `args` names and write its report to `out`.
pub fn run(args: &Args, out: &mut impl Write) -> Result<Verdict, RunError> {
    let (spec_path, repo_root) =
        resolve(&args.target).ok_or_else(|| RunError::NoSpec(args.target.clone()))?;
    let src = std::fs::read_to_string(&spec_path).map_err(|source| RunError::ReadSpec {
        path: spec_path.clone(),
        source,
    })?;

    let parsed = parse(&src);
    let checked = check(&parsed.spec, &repo_root);
    let mut diagnostics = parsed.diagnostics;
    diagnostics.extend(checked.diagnostics);
    diagnostics.sort_by_key(|d| (d.span.start, d.severity));

    let verdict = verdict(&diagnostics, args.lint);
    let written = match args.output {
        Output::Json => {
            let json = serde_json::to_string_pretty(&diagnostics).map_err(RunError::Serialize)?;
            writeln!(out, "{json}")
        }
        Output::Ratchet => {
            let (tightened, n) = ratchet(&src, &checked.measured);
            if n == 0 {
                writeln!(out, "no budget could be tightened")
            } else {
                std::fs::write(&spec_path, &tightened).map_err(|source| RunError::WriteSpec {
                    path: spec_path.clone(),
                    source,
                })?;
                writeln!(out, "tightened {n} budget(s) in {}", spec_path.display())
            }
        }
        Output::Report { detail } => write_report(
            out,
            &spec_path,
            &src,
            &parsed.spec.standards,
            &checked.measured,
            &diagnostics,
            detail,
        ),
    };
    written.map_err(RunError::Report)?;
    if args.output == Output::Ratchet {
        Ok(Verdict::Pass)
    } else {
        Ok(verdict)
    }
}

fn write_report(
    out: &mut impl Write,
    spec_path: &Path,
    src: &str,
    standards: &[Standard],
    measured: &[Measured],
    diagnostics: &[Diagnostic],
    detail: bool,
) -> io::Result<()> {
    for m in measured {
        let verdict = match m.actual.cmp(&m.budget) {
            std::cmp::Ordering::Greater => "OVER ",
            std::cmp::Ordering::Less => "under",
            std::cmp::Ordering::Equal => "at   ",
        };
        writeln!(
            out,
            "{verdict} {:>4}/{:<4}  {}  ({})",
            m.actual,
            m.budget,
            m.standard,
            m.metric.render()
        )?;
        if detail {
            for offender in &m.offenders {
                writeln!(out, "            {offender}")?;
            }
        }
    }

    let reviewed: Vec<(&str, &str)> = standards
        .iter()
        .filter_map(|s| match &s.verification {
            Some(Verification::Reviewed { audited, .. }) => Some((
                s.name.value.as_str(),
                audited.as_ref().map_or("never", |a| a.value.as_str()),
            )),
            _ => None,
        })
        .collect();
    if !reviewed.is_empty() {
        writeln!(out, "\nReviewed, not counted:")?;
        for (name, audited) in reviewed {
            writeln!(out, "  {name}  (audited {audited})")?;
        }
    }

    let index = LineIndex::new(src);
    if !diagnostics.is_empty() {
        writeln!(out)?;
    }
    for d in diagnostics {
        let at = index.position(d.span.start);
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        writeln!(
            out,
            "{}:{}:{}: {severity}[{}] {}",
            spec_path.display(),
            at.line + 1,
            at.character + 1,
            d.code,
            d.message
        )?;
    }
    Ok(())
}

fn verdict(diagnostics: &[Diagnostic], lint: bool) -> Verdict {
    let fails = if lint {
        !diagnostics.is_empty()
    } else {
        has_errors(diagnostics)
    };
    if fails { Verdict::Fail } else { Verdict::Pass }
}

fn resolve(target: &Path) -> Option<(PathBuf, PathBuf)> {
    if target.is_file() {
        return Some((
            target.to_path_buf(),
            target.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ));
    }
    let candidate = target.join(SPEC_FILE);
    candidate
        .is_file()
        .then(|| (candidate, target.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, UsageError> {
        Args::parse(list.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn json_wins_over_ratchet_and_report() {
        let parsed = args(&["--ratchet", "--json", "--lint", "repo"]).unwrap();
        assert_eq!(parsed.output, Output::Json);
        assert!(parsed.lint);
        assert_eq!(parsed.target, PathBuf::from("repo"));
    }

    #[test]
    fn unknown_flag_and_missing_target_are_usage_errors() {
        assert_eq!(
            args(&["--frobnicate", "repo"]),
            Err(UsageError::UnknownFlag("--frobnicate".to_owned()))
        );
        assert_eq!(args(&["--detail"]), Err(UsageError::MissingTarget));
    }

    #[test]
    fn a_failing_writer_is_a_report_error() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SPEC_FILE), "repo: t\n").unwrap();
        let parsed = args(&[dir.path().to_str().unwrap()]).unwrap();
        assert!(matches!(
            run(&parsed, &mut Closed),
            Err(RunError::Report(_))
        ));
    }
}
