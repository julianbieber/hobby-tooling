//! The budget and ratchet mechanics.
//!
//! These pin the property the format exists for: a standard is durable, its
//! budget only moves down, and ordinary edits to the code do not force an edit
//! to the file.

use std::path::Path;
use struct_parser::diagnostic::code;
use struct_parser::{Severity, check, parse, ratchet};
use tempfile::TempDir;

fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    for (path, body) in files {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
        std::fs::write(&full, body).expect("write");
    }
    dir
}

fn codes(spec: &str, root: &Path) -> Vec<(&'static str, Severity, String)> {
    let parsed = parse(spec);
    let checked = check(&parsed.spec, root);
    parsed
        .diagnostics
        .into_iter()
        .chain(checked.diagnostics)
        .map(|d| (d.code, d.severity, d.message))
        .collect()
}

fn spec_with(budget: usize) -> String {
    format!(
        "\
repo: t
root: src

## A file fits in one head

check: files over 10 lines
budget: {budget}
"
    )
}

/// Two files over the threshold, one under.
fn three_files() -> TempDir {
    repo(&[
        ("src/big.rs", &"// line\n".repeat(20)),
        ("src/bigger.rs", &"// line\n".repeat(30)),
        ("src/small.rs", "// line\n"),
    ])
}

// The budget is a ceiling: at it is clean, over it is an error. This is the
// whole enforcement model, replacing a waiver per offending file.
#[test]
fn a_budget_is_a_ceiling() {
    let dir = three_files();
    assert!(
        codes(&spec_with(2), dir.path())
            .iter()
            .all(|(c, _, _)| *c != code::BUDGET_EXCEEDED),
        "two offenders against a budget of two is clean"
    );
    let over = codes(&spec_with(1), dir.path());
    assert!(
        over.iter()
            .any(|(c, s, _)| *c == code::BUDGET_EXCEEDED && *s == Severity::Error)
    );
}

// Slack is a nudge rather than a failure: improving the code should never be
// what breaks the check.
#[test]
fn slack_is_a_warning_naming_the_tighter_number() {
    let dir = three_files();
    let found = codes(&spec_with(5), dir.path());
    let slack = found
        .iter()
        .find(|(c, _, _)| *c == code::BUDGET_SLACK)
        .expect("two offenders against a budget of five");
    assert_eq!(slack.1, Severity::Warning);
    assert!(slack.2.contains("tighten it to 2"), "got {}", slack.2);
}

// The ratchet only moves down. A budget that rose would record a regression as
// the new normal, which is the one thing it must never do.
#[test]
fn the_ratchet_only_tightens() {
    let dir = three_files();
    let loose = spec_with(5);
    let measured = check(&parse(&loose).spec, dir.path()).measured;
    let (tightened, n) = ratchet(&loose, &measured);
    assert_eq!(n, 1);
    assert!(tightened.contains("budget: 2"), "{tightened}");

    let tight = spec_with(1);
    let measured = check(&parse(&tight).spec, dir.path()).measured;
    let (unchanged, n) = ratchet(&tight, &measured);
    assert_eq!(n, 0, "an exceeded budget must not be raised to fit");
    assert_eq!(unchanged, tight);
}

// The point of the format. A file already over the limit growing further, or a
// module being renamed, changes no count and so forces no edit here. The
// previous design stored `? growth.rs 2758 lines` and broke on both.
#[test]
fn ordinary_edits_do_not_move_the_count() {
    let before = repo(&[
        ("src/big.rs", &"// line\n".repeat(20)),
        ("src/small.rs", "// line\n"),
    ]);
    let after = repo(&[
        ("src/renamed.rs", &"// line\n".repeat(400)),
        ("src/small.rs", "// line\n"),
    ]);
    let spec = spec_with(1);
    assert!(
        codes(&spec, before.path())
            .iter()
            .all(|(c, _, _)| *c != code::BUDGET_EXCEEDED && *c != code::BUDGET_SLACK)
    );
    assert!(
        codes(&spec, after.path())
            .iter()
            .all(|(c, _, _)| *c != code::BUDGET_EXCEEDED && *c != code::BUDGET_SLACK),
        "growing a file that was already over, and renaming it, must not touch the file"
    );
}

// A standard that does not say how it is held to reads as enforced and may not
// be. That refusal is the one thing carried over from the first design.
#[test]
fn a_standard_must_say_how_it_is_verified() {
    let dir = repo(&[("src/a.rs", "")]);
    let found = codes(
        "repo: t\nroot: src\n\n## Something worth doing\n\nProse and nothing else.\n",
        dir.path(),
    );
    assert!(
        found
            .iter()
            .any(|(c, s, _)| *c == code::UNVERIFIED_STANDARD && *s == Severity::Error)
    );
}

// A `check:` with no ceiling counts and enforces nothing, which is the same
// failure wearing a different hat.
#[test]
fn a_check_without_a_budget_is_refused() {
    let dir = repo(&[("src/a.rs", "")]);
    let found = codes(
        "repo: t\nroot: src\n\n## Files\n\ncheck: files over 10 lines\n",
        dir.path(),
    );
    assert!(found.iter().any(|(c, _, _)| *c == code::MISSING_BUDGET));
}

// The metric vocabulary is closed: a typo must not read as a standard that
// happens to have nothing to say.
#[test]
fn an_unknown_metric_is_refused() {
    let dir = repo(&[("src/a.rs", "")]);
    let found = codes(
        "repo: t\nroot: src\n\n## Files\n\ncheck: files that are too big\nbudget: 0\n",
        dir.path(),
    );
    assert!(found.iter().any(|(c, _, _)| *c == code::UNKNOWN_METRIC));
}

// A reviewed standard is honest about not being mechanical, but an undated one
// cannot say whether anybody ever looked.
#[test]
fn a_reviewed_standard_wants_a_date() {
    let dir = repo(&[("src/a.rs", "")]);
    let undated = codes(
        "repo: t\nroot: src\n\n## Torn down by tag\n\nreview: every spawn carries the tag\n",
        dir.path(),
    );
    assert!(undated.iter().any(|(c, _, _)| *c == code::MISSING_AUDIT));

    let dated = codes(
        "repo: t\nroot: src\n\n## Torn down by tag\n\nreview: every spawn carries the tag\naudited: 2026-09-01\n",
        dir.path(),
    );
    assert!(dated.iter().all(|(c, _, _)| *c != code::MISSING_AUDIT));
}

// Import cycles are counted as mutual pairs, and the count must not depend on
// which file the walk happened to start from.
#[test]
fn cycles_are_counted_as_pairs() {
    let dir = repo(&[
        ("src/a.rs", "use crate::b;\n"),
        ("src/b.rs", "use crate::a;\n"),
        ("src/c.rs", "use crate::a;\n"),
    ]);
    let spec = "repo: t\nroot: src\n\n## DAG\n\ncheck: import cycles\nbudget: 1\n";
    assert!(
        codes(spec, dir.path())
            .iter()
            .all(|(c, _, _)| *c != code::BUDGET_EXCEEDED && *c != code::BUDGET_SLACK),
        "exactly one pair"
    );
}

// Write-ownership needs no configuration in the new model: the metric is the
// number of types with more than one writer, so no type is ever named.
#[test]
fn multi_writer_types_need_no_configuration() {
    let dir = repo(&[
        (
            "src/a.rs",
            "fn s(mut m: ResMut<Shared>, mut o: ResMut<Own>) {}\n",
        ),
        ("src/b.rs", "fn s(mut m: ResMut<Shared>) {}\n"),
    ]);
    let spec = "repo: t\nroot: src\n\n## One owner\n\ncheck: multi-writer types\nbudget: 1\n";
    assert!(
        codes(spec, dir.path())
            .iter()
            .all(|(c, _, _)| *c != code::BUDGET_EXCEEDED && *c != code::BUDGET_SLACK),
        "Shared has two writers, Own has one"
    );
}
