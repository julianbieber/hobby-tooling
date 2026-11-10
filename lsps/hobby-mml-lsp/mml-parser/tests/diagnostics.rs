use mml_parser::ast::{Gutter, Role, TestKind};
use mml_parser::parser::shared_prefix;
use mml_parser::{Severity, analyze, parse};

const BASE: &str = "\
repo: demo
branch: feat
base: 1111111
head: 2222222

## intent
  - do a thing

## data
+ struct Foo { a: String }

## unit run — the runner @ run.rs:1
  - start ^start
+ - do more @ run.rs:2 @ api Mod::go
  invariants:
  - holds

## api
  Mod — does stuff
+   fn go(x: String) -> bool @ m.rs:3

## tests
+ test go_works — it goes @ run_test.rs:1 @ guards ^start
";

fn codes(src: &str) -> Vec<&'static str> {
    analyze(src).1.into_iter().map(|d| d.code).collect()
}

fn with(from: &str, to: &str) -> String {
    assert!(BASE.contains(from), "BASE lacks {from:?}");
    BASE.replacen(from, to, 1)
}

fn assert_code(src: &str, code: &str) {
    let all = analyze(src).1;
    assert!(
        all.iter().any(|d| d.code == code),
        "expected `{code}`, got {:?}",
        all.iter()
            .map(|d| (d.code, d.message.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn base_is_clean() {
    assert_eq!(codes(BASE), Vec::<&str>::new());
}

#[test]
fn missing_header_key() {
    assert_code(&with("branch: feat\n", ""), "missing-header-key");
}

#[test]
fn unknown_header_key() {
    assert_code(
        &with("repo: demo\n", "repo: demo\nowner: me\n"),
        "unknown-header-key",
    );
}

#[test]
fn bad_header_value() {
    assert_code(&with("head: 2222222", "head: main"), "bad-header-value");
    assert_code(
        &with("branch: feat", "branch: two words"),
        "bad-header-value",
    );
}

#[test]
fn unknown_section() {
    assert_code(&format!("{BASE}\n## notes\n  - x\n"), "unknown-section");
}

#[test]
fn section_order() {
    assert_code(
        &format!("{BASE}\n## data\n+ struct Bar {{ b: String }}\n"),
        "section-order",
    );
    assert_code(&format!("{BASE}\n## api\n  Other — x\n"), "section-order");
}

#[test]
fn missing_intent() {
    assert_code(&with("## intent\n  - do a thing\n", ""), "missing-intent");
    assert_code(&with("  - do a thing\n", ""), "missing-intent");
}

#[test]
fn no_unit() {
    let src = with(
        "## unit run — the runner @ run.rs:1\n  - start ^start\n+ - do more @ run.rs:2 @ api Mod::go\n  invariants:\n  - holds\n",
        "",
    );
    assert_code(&src, "no-unit");
}

#[test]
fn bad_gutter() {
    assert_code(&with("  - start ^start", "* - start ^start"), "bad-gutter");
    assert_code(&with("  - start ^start", "+- start ^start"), "bad-gutter");
}

#[test]
fn bad_indent() {
    assert_code(
        &with("  invariants:", "   - odd\n  invariants:"),
        "bad-indent",
    );
    assert_code(
        &with("  - start ^start", "  - start ^start\n      - too deep"),
        "bad-indent",
    );
}

#[test]
fn nested_change_mismatch() {
    let src = with(
        "  invariants:",
        "+ - if new:\n    - context under an added line @ run.rs:4\n  invariants:",
    );
    assert_code(&src, "nested-change-mismatch");
}

#[test]
fn change_in_intent() {
    assert_code(
        &with("  - do a thing", "+ - do a thing"),
        "change-in-intent",
    );
}

#[test]
fn bad_tilde() {
    assert_code(
        &with(
            "  - start ^start",
            "~ - start without an arrow ^start @ run.rs:1",
        ),
        "bad-tilde",
    );
    assert_code(
        &with("  - start ^start", "~ - a → b → c ^start @ run.rs:1"),
        "bad-tilde",
    );
}

#[test]
fn bad_annotation() {
    assert_code(
        &with("@ run.rs:2 @ api Mod::go", "@ api Mod::go @ run.rs:2"),
        "bad-annotation",
    );
}

#[test]
fn duplicate_anchor() {
    assert_code(&with("  - holds", "  - holds ^start"), "duplicate-anchor");
}

#[test]
fn unknown_api_ref() {
    assert_code(&with("@ api Mod::go", "@ api Mod::stop"), "unknown-api-ref");
}

#[test]
fn unknown_anchor() {
    assert_code(
        &with("@ guards ^start", "@ guards ^start, ^nope"),
        "unknown-anchor",
    );
}

#[test]
fn bad_struct() {
    assert_code(
        &with(
            "+ struct Foo { a: String }",
            "+ struct Foo {\n+   a: String,",
        ),
        "bad-struct",
    );
    assert_code(
        &with("+ struct Foo { a: String }", "+ struct foo { a: String }"),
        "bad-struct",
    );
    assert_code(
        &with("+ struct Foo { a: String }", "+ struct Foo { a: string }"),
        "bad-struct",
    );
    assert_code(
        &with(
            "+ struct Foo { a: String }",
            "+ struct Foo {\n+   a: String,\n  }",
        ),
        "bad-struct",
    );
}

#[test]
fn duplicate_type() {
    assert_code(
        &with(
            "+ struct Foo { a: String }",
            "+ struct Foo { a: String }\n+ enum Foo { x, y }",
        ),
        "duplicate-type",
    );
}

#[test]
fn duplicate_unit() {
    assert_code(
        &with("## api", "## unit run — again\n  - x\n\n## api"),
        "duplicate-unit",
    );
}

#[test]
fn bad_unit_header() {
    assert_code(
        &with("## unit run — the runner", "## unit run the runner"),
        "bad-unit-header",
    );
    assert_code(
        &with("## unit run — the runner", "## unit Run — the runner"),
        "bad-unit-header",
    );
    assert_code(
        &with(
            "## unit run — the runner",
            "## unit run (gone) — the runner",
        ),
        "bad-unit-header",
    );
    assert_code(
        &with("## unit run — the runner", "## unit"),
        "bad-unit-header",
    );
}

#[test]
fn misplaced_invariants() {
    assert_code(
        &with("  - holds", "  - holds\n    - nested step"),
        "misplaced-invariants",
    );
    assert_code(
        &with("  - holds", "  - holds\n  invariants:\n  - again"),
        "misplaced-invariants",
    );
}

#[test]
fn bad_fn() {
    assert_code(
        &with("fn go(x: String) -> bool", "fn go(x String) -> bool"),
        "bad-fn",
    );
    assert_code(
        &with(
            "  Mod — does stuff",
            "+ fn loose() @ m.rs:1\n  Mod — does stuff",
        ),
        "bad-fn",
    );
    assert_code(
        &with("@ m.rs:3\n", "@ m.rs:3\n+     fn deep() @ m.rs:4\n"),
        "bad-fn",
    );
}

#[test]
fn duplicate_module() {
    assert_code(
        &with("## tests", "  Mod — again\n\n## tests"),
        "duplicate-module",
    );
}

#[test]
fn duplicate_fn() {
    assert_code(
        &with("@ m.rs:3\n", "@ m.rs:3\n+   fn go(y: i32) @ m.rs:9\n"),
        "duplicate-fn",
    );
}

#[test]
fn bad_test() {
    assert_code(
        &with("+ test go_works — it goes", "+ check go_works — it goes"),
        "bad-test",
    );
    assert_code(
        &with("+ test go_works — it goes", "+ test GoWorks — it goes"),
        "bad-test",
    );
    assert_code(
        &with("+ test go_works — it goes", "+ tests many — it goes"),
        "bad-test",
    );
}

#[test]
fn no_change() {
    let src = BASE.replace("\n+ ", "\n  ");
    let all = analyze(&src).1;
    assert!(
        all.iter()
            .any(|d| d.code == "no-change" && d.severity == Severity::Warning)
    );
}

#[test]
fn unlinked_change() {
    let all = analyze(&with(
        "+ - do more @ run.rs:2 @ api Mod::go",
        "+ - do more @ api Mod::go",
    ))
    .1;
    assert!(
        all.iter()
            .any(|d| d.code == "unlinked-change" && d.severity == Severity::Warning)
    );
    assert_code(
        &with(
            "+   fn go(x: String) -> bool @ m.rs:3",
            "+   fn go(x: String) -> bool",
        ),
        "unlinked-change",
    );
    assert_code(
        &with(
            "+ test go_works — it goes @ run_test.rs:1",
            "+ test go_works — it goes",
        ),
        "unlinked-change",
    );
}

#[test]
fn nesting_rule() {
    let ok = with(
        "  invariants:",
        "- - if gone: @ run.rs:5\n-   - removed body\n~ - check a → check b: @ run.rs:6\n+   - added under changed @ run.rs:7\n    - context under changed\n  - context parent:\n-   - removed under context @ run.rs:8\n  invariants:",
    );
    assert_eq!(codes(&ok), Vec::<&str>::new());
    let bad = with(
        "  invariants:",
        "- - if gone: @ run.rs:5\n+   - added under removed @ run.rs:6\n  invariants:",
    );
    assert_eq!(codes(&bad), vec!["nested-change-mismatch"]);
    let new_unit = with(
        "## api",
        "## unit fresh (new) — a new one\n+ - first @ x.rs:1\n  - context line\n\n## api",
    );
    assert_eq!(codes(&new_unit), vec!["nested-change-mismatch"]);
}

#[test]
fn tilde_lines_reconstruct_both_versions() {
    let src = with(
        "  - start ^start",
        "~ - spawn enemies from the table → from the wave table ^start @ run.rs:1",
    );
    let doc = parse(&src);
    assert_eq!(codes(&src), Vec::<&str>::new());
    let line = doc
        .lines
        .iter()
        .find(|l| l.gutter == Gutter::Changed)
        .unwrap();
    let t = line.tilde.as_ref().unwrap();
    assert_eq!(t.old, "spawn enemies from the table");
    assert_eq!(t.new, "spawn enemies from the wave table");
    assert_eq!(line.text, t.new);
    assert_eq!(&src[t.old_clause.start..t.old_clause.end], "from the table");
    assert_eq!(
        &src[t.new_clause.start..t.new_clause.end],
        "from the wave table"
    );
    assert_eq!(line.ann.anchor.as_ref().unwrap().value, "start");
}

#[test]
fn tilde_shared_prefix() {
    assert_eq!(
        shared_prefix("answer the spec itself", "answer a summary"),
        0
    );
    assert_eq!(
        shared_prefix("run it and take the rendered table", "take the rows"),
        "run it and ".len()
    );
    assert_eq!(
        shared_prefix("test x_y — rows passed as data", "a stored result"),
        "test x_y — ".len()
    );
    assert_eq!(
        shared_prefix(
            "the agent passes the rows verbatim",
            "the agent names a chart"
        ),
        0
    );
}

#[test]
fn tilde_test_and_field_lines() {
    let src = with(
        "+ test go_works — it goes @ run_test.rs:1 @ guards ^start",
        "~ test go_works — it went → it goes @ run_test.rs:1 @ guards ^start",
    )
    .replace(
        "+ struct Foo { a: String }",
        "  struct Foo {\n~   a: String → Vec<String>,\n  }",
    );
    assert_eq!(codes(&src), Vec::<&str>::new());
    let doc = parse(&src);
    assert!(matches!(&doc.tests[0].kind, TestKind::Single(n) if n == "go_works"));
    assert_eq!(doc.tests[0].description, "it goes");
    let f = &doc.data[0].fields[0];
    assert_eq!(f.ty.as_ref().unwrap().value, "Vec<String>");
    assert_eq!(
        &src[f.ty.as_ref().unwrap().span.start..f.ty.as_ref().unwrap().span.end],
        "Vec<String>"
    );
}

#[test]
fn annotation_order() {
    let all_four = with(
        "+ test go_works — it goes @ run_test.rs:1 @ guards ^start",
        "+ test go_works — it goes @ run_test.rs:1 @ guards ^start",
    )
    .replace(
        "+ - do more @ run.rs:2 @ api Mod::go",
        "+ - do more ^more @ run.rs:2 @ api Mod::go",
    );
    assert_eq!(codes(&all_four), Vec::<&str>::new());
    let doc = parse(&all_four);
    let line = doc
        .lines
        .iter()
        .find(|l| l.ann.anchor.as_ref().is_some_and(|a| a.value == "more"))
        .unwrap();
    assert_eq!(line.text, "do more");
    assert_eq!(
        line.ann.link.as_ref().unwrap().line.as_ref().unwrap().value,
        2
    );
    assert_eq!(line.ann.api[0].value, "Mod::go");

    for bad in [
        "+ - do more @ run.rs:2 ^more @ api Mod::go",
        "+ - do more @ run.rs:2 @ run.rs:3 @ api Mod::go",
        "+ - do more @ run.rs:2 @ api Mod::go @ guards ^start",
        "+ - do more @ run.rs:2 @ api",
    ] {
        let src = with("+ - do more @ run.rs:2 @ api Mod::go", bad);
        assert_code(&src, "bad-annotation");
    }
    assert_code(
        &with("  - holds", "  - holds @ api Mod::go"),
        "bad-annotation",
    );
    assert_code(
        &with("+ test go_works — it goes", "+ test go_works — it goes ^t"),
        "bad-annotation",
    );
    assert_code(
        &with("## unit run — the runner", "## unit run — the runner ^r"),
        "bad-annotation",
    );
}

#[test]
fn same_name_on_both_sides_is_not_a_duplicate() {
    let src = with(
        "+   fn go(x: String) -> bool @ m.rs:3",
        "-   fn go(x: String) @ m.rs:3\n+   fn go(x: String) -> bool @ m.rs:3",
    )
    .replace(
        "+ struct Foo { a: String }",
        "- struct Foo { a: i32 }\n+ struct Foo { a: String }",
    );
    assert_eq!(codes(&src), Vec::<&str>::new());
}

#[test]
fn change_block_link_covers_its_lines() {
    let src = with(
        "  invariants:",
        "+ - if new:\n+   - linked child @ run.rs:4\n- - removed parent: @ run.rs:5\n-   - unlinked child\n  invariants:",
    );
    assert_eq!(codes(&src), Vec::<&str>::new());
}

#[test]
fn roles_and_steps() {
    let doc = parse(BASE);
    let unit = &doc.units[0];
    assert_eq!(unit.name.value, "run");
    assert_eq!(unit.what, "the runner");
    assert_eq!(unit.link.as_ref().unwrap().file.value, "run.rs");
    assert_eq!(unit.steps.len(), 2);
    assert_eq!(unit.invariants.len(), 1);
    assert_eq!(doc.lines[unit.invariants[0]].role, Role::Invariant);
    assert_eq!(doc.api[0].fns[0].name.value, "go");
    assert_eq!(doc.resolve_api("Mod::go").unwrap().1.name.value, "go");
}

#[test]
fn recovers_after_errors() {
    let src = with("  - start ^start", "* broken\n  - start ^start")
        .replace("## api", "## bogus\n  junk\n\n## api");
    let (doc, all) = analyze(&src);
    let c: Vec<_> = all.iter().map(|d| d.code).collect();
    assert!(
        c.contains(&"bad-gutter") && c.contains(&"unknown-section"),
        "{c:?}"
    );
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.api.len(), 1);
    assert_eq!(doc.tests.len(), 1);
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let src = format!(
        "# a comment\n{}",
        BASE.replace("## data", "#\n# note\n\n## data")
    );
    assert_eq!(codes(&src), Vec::<&str>::new());
}

#[test]
fn api_takes_several_targets() {
    let src = "repo: r\nbranch: b\nbase: abcdef1\nhead: abcdef2\n\n## intent\n  - x\n\n## unit u — the unit\n+ - go @ F.rs:1 @ api Mod::go, Other::run\n\n## api\n+ Mod — m\n+   fn go() @ F.rs:2\n+ Other — o\n+   fn run() @ F.rs:3\n";
    let (doc, diags) = mml_parser::analyze(src);
    assert!(
        diags
            .iter()
            .all(|d| d.severity != mml_parser::Severity::Error),
        "{diags:?}"
    );
    let line = doc.lines.iter().find(|l| !l.ann.api.is_empty()).unwrap();
    assert_eq!(line.ann.api.len(), 2);
    let bad = src.replace("Other::run", "Other::nope");
    let (_, diags) = mml_parser::analyze(&bad);
    assert!(diags.iter().any(|d| d.code == "unknown-api-ref"));
}

#[test]
fn tilde_on_a_return_type_keeps_the_signature() {
    let old = "fn lease(contract_id: Uuid) -> Proxy";
    let cut = shared_prefix(old, "Option<Proxy>");
    assert_eq!(
        format!("{}{}", &old[..cut], "Option<Proxy>"),
        "fn lease(contract_id: Uuid) -> Option<Proxy>"
    );
}

#[test]
fn tilde_with_ellipsis_keeps_the_old_words_and_adds() {
    let src = "repo: r\nbranch: b\nbase: abcdef1\nhead: abcdef2\n\n## intent\n  - x\n\n## unit u — the unit\n~ - head each section \"### Query N\" → … adding \"(result id: r-x)\" @ F.rs:1\n";
    let (doc, diags) = analyze(src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:?}"
    );
    let t = doc.lines.iter().find_map(|l| l.tilde.as_ref()).unwrap();
    assert_eq!(t.old, "head each section \"### Query N\"");
    assert_eq!(
        t.new,
        "head each section \"### Query N\" adding \"(result id: r-x)\""
    );
}

#[test]
fn unit_trigger_schedule_and_observer() {
    for header in [
        "## unit run in Update — the runner @ run.rs:1",
        "## unit run in OnEnter(Screen::Gameplay) — the runner @ run.rs:1",
        "## unit run on WaveCleared — the runner @ run.rs:1",
        "## unit run on Add<Enemy> — the runner @ run.rs:1",
    ] {
        let src = with("## unit run — the runner @ run.rs:1", header);
        assert_eq!(codes(&src), Vec::<&str>::new(), "{header}");
    }
    let doc = parse(&with(
        "## unit run — the runner",
        "## unit run on WaveCleared — the runner",
    ));
    let t = doc.units[0].trigger.as_ref().unwrap();
    assert_eq!(t.keyword(), "on");
    assert_eq!(t.target().value, "WaveCleared");
}

#[test]
fn new_unit_takes_a_trigger_after_its_status() {
    let src = with(
        "\n## api",
        "\n## unit fresh (new) in Update — a new one\n+ - first @ x.rs:1\n\n## api",
    );
    assert_eq!(codes(&src), Vec::<&str>::new());
}

#[test]
fn bad_unit_trigger() {
    for header in [
        "## unit run in update — the runner",
        "## unit run in — the runner",
        "## unit run on wave_cleared — the runner",
        "## unit run in Update extra — the runner",
        "## unit run at Update — the runner",
    ] {
        assert_code(&with("## unit run — the runner", header), "bad-unit-header");
    }
}

#[test]
fn data_types_carry_an_ecs_role() {
    let src = with(
        "+ struct Foo { a: String }",
        "+ component struct Foo { a: String }\n+ resource struct Bar {\n+   b: u32,\n+ }\n+ state enum Screen { title, gameplay }",
    );
    let (doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "{diags:?}");
    let roles: Vec<_> = doc
        .data
        .iter()
        .map(|t| t.role.map(|r| r.keyword()))
        .collect();
    assert_eq!(roles, [Some("component"), Some("resource"), Some("state")]);
    assert_eq!(doc.data[1].fields.len(), 1);
}

#[test]
fn unknown_data_role() {
    assert_code(
        &with(
            "+ struct Foo { a: String }",
            "+ asset struct Foo { a: String }",
        ),
        "bad-struct",
    );
}
