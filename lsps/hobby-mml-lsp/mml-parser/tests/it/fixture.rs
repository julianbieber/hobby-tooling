use std::process::Command;

use mml_parser::analyze;
use mml_parser::ast::{EcsRole, Side, Trigger, UnitStatus};

const FIXTURE: &str = include_str!("../fixtures/delta-wave-bounties.hmml");
const FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/delta-wave-bounties.hmml"
);

#[test]
fn fixture_parses_clean() {
    let (doc, diags) = analyze(FIXTURE);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(doc.header.value("repo"), Some("bevy_survivor"));
    assert_eq!(doc.intent.len(), 2);
    assert_eq!(doc.data.len(), 5);
    let roles: Vec<_> = doc.data.iter().map(|t| t.role).collect();
    assert_eq!(
        roles,
        [
            Some(EcsRole::Component),
            Some(EcsRole::Event),
            Some(EcsRole::Resource),
            Some(EcsRole::Message),
            Some(EcsRole::State)
        ]
    );
    let names: Vec<_> = doc.units.iter().map(|u| u.name.value.as_str()).collect();
    assert_eq!(names, ["spawn-wave", "pay-bounty", "buy-upgrade"]);
    let status: Vec<_> = doc.units.iter().map(|u| u.status).collect();
    assert_eq!(
        status,
        [UnitStatus::Present, UnitStatus::New, UnitStatus::Present]
    );
    assert!(
        matches!(&doc.units[0].trigger, Some(Trigger::Schedule(s)) if s.value == "FixedUpdate")
    );
    assert!(matches!(&doc.units[1].trigger, Some(Trigger::Observer(e)) if e.value == "EnemyDied"));
    assert_eq!(doc.api.len(), 3);
    assert_eq!(doc.tests.len(), 6);
    let base_links = doc.links().iter().filter(|(_, s)| *s == Side::Base).count();
    assert_eq!(base_links, 4);
}

#[test]
fn cli_check_is_clean() {
    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg(FIXTURE_PATH)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok (no diagnostics)"));
}

#[test]
fn cli_lint_without_a_checkout_reports_only_checkout_not_found() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg(FIXTURE_PATH)
        .arg("--lint")
        .env("HOME", home.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "{stdout}");
    assert!(
        lines[0].contains("warning [checkout-not-found]"),
        "{stdout}"
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn cli_json_and_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .args([FIXTURE_PATH, "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["diagnostics"].as_array().unwrap().len(), 0);
    assert_eq!(v["ast"]["units"].as_array().unwrap().len(), 3);

    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg("/nonexistent/x.hmml")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn cli_exit_codes_follow_severity() {
    let dir = tempfile::tempdir().unwrap();
    let warn_only = dir.path().join("warn.hmml");
    std::fs::write(&warn_only, FIXTURE.replace(" @ purse_test.rs:10", "")).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
            .args(args)
            .env("HOME", dir.path())
            .output()
            .unwrap()
            .status
            .code()
    };
    let p = warn_only.to_str().unwrap();
    assert_eq!(run(&[p]), Some(0));
    assert_eq!(run(&[p, "--lint"]), Some(1));
    let broken = dir.path().join("broken.hmml");
    std::fs::write(&broken, FIXTURE.replace("## intent", "## purpose")).unwrap();
    assert_eq!(run(&[broken.to_str().unwrap()]), Some(1));
}
