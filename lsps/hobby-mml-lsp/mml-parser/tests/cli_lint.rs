//! End-to-end tests for the CLI's `--lint` mode: exit codes and the
//! `unresolved-source-path` check, run against the real binary.

use std::fs;
use std::path::Path;
use std::process::Command;

/// A minimal valid spec whose source references point at `supervisor.py` (via
/// `source.paths`) and `child.py` (via a module `@ path`), both relative.
const SPEC: &str = "system: sys\nunit: one\nkind: workflow\nsource:\n  repo: nonexistent-repo\n  paths:\n    - supervisor.py\n\n## modules\nWorker — runs @ child.py\n\n## data\nin:  Queue<String>\nout: Stream<String>\n\n## behavior\n- run\n\n## invariants\n";

fn run_lint(spec_path: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg(spec_path)
        .arg("--lint")
        .output()
        .expect("run mml-parser");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn lint_clean_spec_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    // Both referenced files exist next to the spec (the spec-dir fallback).
    fs::write(dir.path().join("supervisor.py"), "x").unwrap();
    fs::write(dir.path().join("child.py"), "x").unwrap();
    let spec = dir.path().join("one.hmml");
    fs::write(&spec, SPEC).unwrap();

    let (ok, stdout) = run_lint(&spec);
    assert!(ok, "expected exit 0, got output:\n{stdout}");
    assert!(stdout.contains("ok (no diagnostics)"), "{stdout}");
}

#[test]
fn lint_missing_source_paths_exit_one_with_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    // Neither supervisor.py nor child.py exists anywhere.
    let spec = dir.path().join("one.hmml");
    fs::write(&spec, SPEC).unwrap();

    let (ok, stdout) = run_lint(&spec);
    assert!(!ok, "expected exit 1, got output:\n{stdout}");
    assert!(stdout.contains("unresolved-source-path"), "{stdout}");
    assert!(stdout.contains("supervisor.py"), "{stdout}");
    assert!(stdout.contains("child.py"), "{stdout}");
}

#[test]
fn check_mode_ignores_source_paths() {
    let dir = tempfile::tempdir().unwrap();
    // Same broken spec: plain --check stays a structural pass and exits 0.
    let spec = dir.path().join("one.hmml");
    fs::write(&spec, SPEC).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg(&spec)
        .arg("--check")
        .output()
        .expect("run mml-parser");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(!stdout.contains("unresolved-source-path"), "{stdout}");
}
