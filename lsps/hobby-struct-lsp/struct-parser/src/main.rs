//! The `hobby-struct-parser` command line: read a repo's standards, count the
//! code against them, and report.

use std::path::{Path, PathBuf};
use struct_parser::ast::Verification;
use struct_parser::span::LineIndex;
use struct_parser::{Severity, check, has_errors, parse, ratchet};

fn main() -> std::process::ExitCode {
    // Rust masks SIGPIPE, so a report piped into `head` panics on the first
    // write past the reader's exit instead of stopping. Reports are long and
    // piping one is normal use.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let (lint, detail, do_ratchet, json) = (
        flag("--lint"),
        flag("--detail"),
        flag("--ratchet"),
        flag("--json"),
    );
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    let Some(target) = positional.first() else {
        eprintln!("usage: hobby-struct-parser [--lint|--detail|--ratchet|--json] <repo-or-file>");
        return std::process::ExitCode::from(2);
    };
    let Some((spec_path, repo_root)) = resolve(Path::new(target)) else {
        eprintln!("no {} found at {target}", struct_parser::SPEC_FILE);
        return std::process::ExitCode::from(2);
    };
    let src = match std::fs::read_to_string(&spec_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", spec_path.display());
            return std::process::ExitCode::from(2);
        }
    };

    let parsed = parse(&src);
    let checked = check(&parsed.spec, &repo_root);
    let mut diagnostics = parsed.diagnostics;
    diagnostics.extend(checked.diagnostics);
    diagnostics.sort_by_key(|d| (d.span.start, d.severity));

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&diagnostics).expect("diagnostics serialize")
        );
        return exit(&diagnostics, lint);
    }

    if do_ratchet {
        let (tightened, n) = ratchet(&src, &checked.measured);
        if n > 0 && std::fs::write(&spec_path, &tightened).is_ok() {
            println!("tightened {n} budget(s) in {}", spec_path.display());
            return std::process::ExitCode::SUCCESS;
        }
        println!("no budget could be tightened");
        return std::process::ExitCode::SUCCESS;
    }

    for m in &checked.measured {
        let verdict = match m.actual.cmp(&m.budget) {
            std::cmp::Ordering::Greater => "OVER ",
            std::cmp::Ordering::Less => "under",
            std::cmp::Ordering::Equal => "at   ",
        };
        println!(
            "{verdict} {:>4}/{:<4}  {}  ({})",
            m.actual,
            m.budget,
            m.standard,
            m.metric.render()
        );
        if detail {
            for offender in &m.offenders {
                println!("            {offender}");
            }
        }
    }

    let reviewed: Vec<&struct_parser::Standard> = parsed
        .spec
        .standards
        .iter()
        .filter(|s| matches!(s.verification, Some(Verification::Reviewed { .. })))
        .collect();
    if !reviewed.is_empty() {
        println!("\nReviewed, not counted:");
        for s in reviewed {
            let audited = match &s.verification {
                Some(Verification::Reviewed { audited, .. }) => audited
                    .as_ref()
                    .map(|a| a.value.as_str())
                    .unwrap_or("never"),
                _ => "never",
            };
            println!("  {}  (audited {audited})", s.name.value);
        }
    }

    let index = LineIndex::new(&src);
    if !diagnostics.is_empty() {
        println!();
    }
    for d in &diagnostics {
        let at = index.position(d.span.start);
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        println!(
            "{}:{}:{}: {severity}[{}] {}",
            spec_path.display(),
            at.line + 1,
            at.character + 1,
            d.code,
            d.message
        );
    }
    exit(&diagnostics, lint)
}

fn exit(diagnostics: &[struct_parser::Diagnostic], lint: bool) -> std::process::ExitCode {
    let fails = if lint {
        !diagnostics.is_empty()
    } else {
        has_errors(diagnostics)
    };
    if fails {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn resolve(target: &Path) -> Option<(PathBuf, PathBuf)> {
    if target.is_file() {
        return Some((
            target.to_path_buf(),
            target.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ));
    }
    let candidate = target.join(struct_parser::SPEC_FILE);
    candidate
        .is_file()
        .then(|| (candidate, target.to_path_buf()))
}
