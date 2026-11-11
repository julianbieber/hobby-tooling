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

use mml_parser::git::{GitCache, check_links, home_dir};
use mml_parser::{LineIndex, Severity, analyze};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(PartialEq)]
enum Mode {
    Check,
    Lint,
    Json,
}

fn main() -> ExitCode {
    let mut path: Option<String> = None;
    let mut mode = Mode::Check;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--check" => mode = Mode::Check,
            "--lint" => mode = Mode::Lint,
            "--json" => mode = Mode::Json,
            "-V" | "--version" => {
                println!("hobby-mml-parser {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag `{other}`");
                print_usage();
                return ExitCode::from(2);
            }
            other => {
                if path.is_some() {
                    eprintln!("error: multiple input files given");
                    return ExitCode::from(2);
                }
                path = Some(other.to_owned());
            }
        }
    }

    let Some(path) = path else {
        eprintln!("error: no input file");
        print_usage();
        return ExitCode::from(2);
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

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
        Mode::Json => {
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
            let out = json!({ "ast": doc, "diagnostics": diags });
            println!("{}", serde_json::to_string_pretty(&out).expect("serialize"));
        }
        Mode::Check | Mode::Lint => {
            for d in &diagnostics {
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
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_usage() {
    eprintln!("usage: hobby-mml-parser <file> [--check | --lint | --json]");
}
