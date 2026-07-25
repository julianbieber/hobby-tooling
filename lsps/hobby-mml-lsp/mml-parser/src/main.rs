//! `mml-parser` CLI — validate a `.mml` file and report diagnostics.
//!
//! Usage:
//!   mml-parser <file>           # same as --check
//!   mml-parser <file> --check   # human-readable diagnostics (file:line:col)
//!   mml-parser <file> --lint    # --check + filesystem checks (source paths)
//!   mml-parser <file> --json    # { "ast": ..., "diagnostics": [...] } as JSON
//!
//! All modes run the filesystem-aware reference checks from `mml_parser::lint`
//! (`@ spec` refs, `use` imports); `--lint` adds the source-linkage check —
//! every header `source.paths` entry and module `@ path` must resolve to a
//! real file, using the same search order as the LSP's goto-definition.
//!
//! Exit status: 0 = ok, 1 = at least one error-severity diagnostic (under
//! `--lint`: any diagnostic at all — the linter is a gate), 2 = usage / I/O
//! error. The MML skills shell out to `--lint` after writing a spec.

use mml_parser::{LineIndex, Severity, analyze, lint};
use serde_json::json;
use std::path::Path;
use std::process::ExitCode;

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
    // Filesystem checks resolve against the file's real location; a relative
    // CLI argument is canonicalized so worktree/`~/projects` lookups work.
    if let Ok(abs) = Path::new(&path).canonicalize() {
        diagnostics.extend(lint::unresolved_spec_refs(&doc, &abs, None));
        diagnostics.extend(lint::unresolved_imports(&doc, &abs, None));
        if matches!(mode, Mode::Lint) {
            diagnostics.extend(lint::unresolved_source_paths(&doc, &abs, None));
        }
        diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    }
    let index = LineIndex::new(&src);
    let has_error = diagnostics.iter().any(|d| d.severity == Severity::Error);
    let failed = match mode {
        Mode::Lint => !diagnostics.is_empty(),
        Mode::Check | Mode::Json => has_error,
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
    eprintln!("usage: mml-parser <file> [--check | --lint | --json]");
}
