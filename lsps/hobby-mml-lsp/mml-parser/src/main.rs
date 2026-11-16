//! `hobby-mml-parser` CLI — check an HMML v3 delta and report diagnostics.
//!
//! Usage:
//!   hobby-mml-parser <file>           # same as --check
//!   hobby-mml-parser <file> --check   # grammar checks, `file:line:col: severity [code]: message`
//!   hobby-mml-parser <file> --lint    # --check + diff links against the checkout (grammar §8)
//!   hobby-mml-parser <file> --json    # { "ast": ..., "diagnostics": [...] }
//!
//! Exit status: 0 ok; 1 any error, or under `--lint` any error or warning;
//! 2 usage or I/O error. Info never gates.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mml_parser::git::{GitCache, check_links, home_dir};
use mml_parser::{Diagnostic, Document, LineIndex, Severity, analyze};
use serde_json::json;

const USAGE: &str = "usage: hobby-mml-parser <file> [--check | --lint | --json]";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Check,
    Lint,
    Json,
}

enum Command {
    Version,
    Help,
    Run { path: String, mode: Mode },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("unknown flag `{0}`")]
    UnknownFlag(String),
    #[error("multiple input files given")]
    MultipleInputs,
    #[error("no input file")]
    NoInput,
    #[error("cannot read {path}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("cannot serialize JSON output")]
    Json(#[from] serde_json::Error),
}

impl CliError {
    fn shows_usage(&self) -> bool {
        matches!(self, CliError::UnknownFlag(_) | CliError::NoInput)
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Command, CliError> {
    let mut path: Option<String> = None;
    let mut mode = Mode::Check;
    for arg in args {
        match arg.as_str() {
            "--check" => mode = Mode::Check,
            "--lint" => mode = Mode::Lint,
            "--json" => mode = Mode::Json,
            "-V" | "--version" => return Ok(Command::Version),
            "-h" | "--help" => return Ok(Command::Help),
            other if other.starts_with('-') => return Err(CliError::UnknownFlag(arg)),
            _ => {
                if path.is_some() {
                    return Err(CliError::MultipleInputs);
                }
                path = Some(arg);
            }
        }
    }
    let path = path.ok_or(CliError::NoInput)?;
    Ok(Command::Run { path, mode })
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
    let outcome = parse_args(std::env::args().skip(1)).and_then(run);
    match outcome {
        Ok(code) => code,
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "could not check the delta");
            if e.shows_usage() {
                tracing::info!("{USAGE}");
            }
            ExitCode::from(2)
        }
    }
}

fn run(command: Command) -> Result<ExitCode, CliError> {
    let (path, mode) = match command {
        Command::Version => {
            println!("hobby-mml-parser {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        Command::Help => {
            tracing::info!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        Command::Run { path, mode } => (path, mode),
    };

    let src = std::fs::read_to_string(&path).map_err(|source| CliError::Read {
        path: path.clone(),
        source,
    })?;

    let (doc, mut diagnostics) = analyze(&src);
    if mode == Mode::Lint {
        let abs = Path::new(&path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&path));
        let home = home_dir().unwrap_or_default();
        let mut cache = GitCache::new();
        diagnostics.extend(check_links(&mut cache, &doc, &abs, &home));
        diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    }
    let index = LineIndex::new(&src);
    let failed = match mode {
        Mode::Lint => diagnostics.iter().any(|d| d.severity != Severity::Info),
        Mode::Check | Mode::Json => diagnostics.iter().any(|d| d.severity == Severity::Error),
    };

    match mode {
        Mode::Json => print_json(&doc, &diagnostics, &index)?,
        Mode::Check | Mode::Lint => print_text(&path, &diagnostics, &index),
    }

    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn print_json(
    doc: &Document,
    diagnostics: &[Diagnostic],
    index: &LineIndex,
) -> Result<(), CliError> {
    let diags: Vec<_> = diagnostics
        .iter()
        .map(|d| {
            json!({
                "span": d.span,
                "severity": d.severity,
                "code": d.code,
                "message": d.message,
                "range": {
                    "start": index.position(d.span.start),
                    "end": index.position(d.span.end),
                }
            })
        })
        .collect();
    let value = json!({ "ast": doc, "diagnostics": diags });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn print_text(path: &str, diagnostics: &[Diagnostic], index: &LineIndex) {
    for d in diagnostics {
        let pos = index.position(d.span.start);
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        println!(
            "{}:{}:{}: {} [{}]: {}",
            path,
            pos.line + 1,
            pos.character + 1,
            severity,
            d.code,
            d.message
        );
    }
    if diagnostics.is_empty() {
        println!("{path}: ok (no diagnostics)");
    }
}
