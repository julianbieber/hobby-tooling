use std::path::{Path, PathBuf};
use std::process::Command;

use mml_parser::git::{GitCache, check_links};
use mml_parser::{Diagnostic, Severity, parse};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn write(root: &Path, rel: &str, lines: usize) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let body: String = (1..=lines).map(|i| format!("line {i}\n")).collect();
    std::fs::write(p, body).unwrap();
}

struct World {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    notes: PathBuf,
    base: String,
    head: String,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let repo = home.join("fun_worktrees/demo/t/task");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["checkout", "-q", "-b", "feat"]);
    write(&repo, "src/a/Run.rs", 5);
    write(&repo, "src/a/Util.rs", 3);
    write(&repo, "src/b/Util.rs", 3);
    write(&repo, "src/Old.rs", 4);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    write(&repo, "src/a/Run.rs", 10);
    write(&repo, "src/New.rs", 2);
    std::fs::remove_file(repo.join("src/Old.rs")).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "head"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    write(&repo, "src/Later.rs", 1);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "later"]);
    let notes = tmp.path().join("notes/demo/t/task/aspects/main");
    std::fs::create_dir_all(&notes).unwrap();
    World {
        _tmp: tmp,
        home,
        notes,
        base,
        head,
    }
}

fn delta(repo: &str, base: &str, head: &str) -> String {
    format!(
        "repo: {repo}
branch: feat
base: {base}
head: {head}

## intent
  - runs

## unit run — the runner @ a/Run.rs:1
  - start
+ - resolves @ a/Run.rs:8
+ - ambiguous suffix @ Util.rs:1
+ - out of range @ a/Run.rs:99
- - removed step, base side @ Old.rs:4
- - added file is not at base @ New.rs:1
+ - unique among the diff files @ Run.rs:10
"
    )
}

fn lint(w: &World, src: &str) -> Vec<Diagnostic> {
    let path = w.notes.join("delta-feat.hmml");
    std::fs::write(&path, src).unwrap();
    let doc = parse(src);
    let mut cache = GitCache::new();
    check_links(&mut cache, &doc, &path, &w.home)
}

fn on_line<'a>(src: &str, diags: &'a [Diagnostic], needle: &str) -> Vec<&'a Diagnostic> {
    let start = src.find(needle).expect("needle");
    let end = src
        .get(start..)
        .and_then(|rest| rest.find('\n'))
        .map_or(src.len(), |i| start + i);
    diags
        .iter()
        .filter(|d| d.span.start >= start && d.span.start <= end)
        .collect()
}

#[test]
fn links_resolve_against_the_task_checkout() {
    let w = world();
    let src = delta("demo", w.base.get(..10).unwrap(), w.head.get(..10).unwrap());
    let diags = lint(&w, &src);

    assert!(
        on_line(&src, &diags, "## unit run").is_empty(),
        "{diags:#?}"
    );
    assert!(on_line(&src, &diags, "resolves @").is_empty(), "{diags:#?}");
    assert!(
        on_line(&src, &diags, "unique among").is_empty(),
        "{diags:#?}"
    );
    assert!(
        on_line(&src, &diags, "removed step, base side").is_empty(),
        "{diags:#?}"
    );

    let amb = on_line(&src, &diags, "ambiguous suffix");
    assert_eq!(amb.len(), 1);
    assert_eq!(amb[0].code, "unresolved-diff-link");
    assert_eq!(amb[0].severity, Severity::Warning);
    assert!(amb[0].message.contains("src/a/Util.rs") && amb[0].message.contains("src/b/Util.rs"));

    let range = on_line(&src, &diags, "out of range");
    assert_eq!(range.len(), 1);
    assert!(range[0].message.contains("line 99"), "{}", range[0].message);

    let base_side = on_line(&src, &diags, "added file is not at base");
    assert_eq!(base_side.len(), 1);
    assert!(
        base_side[0].message.contains("base"),
        "{}",
        base_side[0].message
    );

    let stale: Vec<_> = diags.iter().filter(|d| d.code == "stale-delta").collect();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].severity, Severity::Info);

    assert!(!diags.iter().any(|d| d.code == "checkout-not-found"));
}

#[test]
fn no_stale_delta_at_the_branch_tip() {
    let w = world();
    let repo = w.home.join("fun_worktrees/demo/t/task");
    git(&repo, &["reset", "-q", "--hard", &w.head]);
    let src = delta("demo", &w.base, &w.head);
    assert!(!lint(&w, &src).iter().any(|d| d.code == "stale-delta"));
}

#[test]
fn checkout_not_found_once() {
    let w = world();
    let src = delta("elsewhere", &w.base, &w.head);
    let diags = lint(&w, &src);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, "checkout-not-found");
    assert_eq!(
        src.get(diags[0].span.start..diags[0].span.end),
        Some("elsewhere")
    );

    let unknown_head = delta("demo", &w.base, "abcdef0123");
    let diags = lint(&w, &unknown_head);
    assert_eq!(
        diags
            .iter()
            .filter(|d| d.code == "checkout-not-found")
            .count(),
        1
    );
}

#[test]
fn aspect_checkout_comes_first() {
    let w = world();
    let task_repo = w.home.join("fun_worktrees/demo/t/task");
    let aspect = w.home.join("fun_aspects/demo/t/task/main");
    std::fs::create_dir_all(aspect.parent().unwrap()).unwrap();
    git(
        &task_repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            aspect.to_str().unwrap(),
            &w.head,
        ],
    );
    let src = delta("demo", &w.base, &w.head);
    let path = w.notes.join("delta-feat.hmml");
    std::fs::write(&path, &src).unwrap();
    let doc = parse(&src);
    let mut cache = GitCache::new();
    let t = mml_parser::git::target(&mut cache, &doc, &path, &w.home).unwrap();
    assert_eq!(t.checkout, aspect);
}

#[test]
fn cli_lint_reports_through_home() {
    let w = world();
    let src = delta("demo", &w.base, &w.head);
    let path = w.notes.join("delta-feat.hmml");
    std::fs::write(&path, &src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hobby-mml-parser"))
        .arg(&path)
        .arg("--lint")
        .env("HOME", &w.home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.matches("[unresolved-diff-link]").count(),
        3,
        "{stdout}"
    );
    assert_eq!(stdout.matches("info [stale-delta]").count(), 1, "{stdout}");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn worktree_head_reads_uncommitted_and_untracked_files() {
    let w = world();
    let repo = w.home.join("fun_worktrees/demo/t/task");
    git(&repo, &["reset", "-q", "--hard", &w.base]);
    write(&repo, "src/a/Run.rs", 8);
    write(&repo, "src/Fresh.rs", 2);
    let src = format!(
        "repo: demo
branch: feat
base: {base}
head: worktree

## intent
  - runs

## unit run — the runner @ a/Run.rs:1
  - start
+ - edited, not committed @ a/Run.rs:8
+ - in an untracked file @ Fresh.rs:2
+ - past the edited file @ a/Run.rs:9
- - removed step, base side @ Old.rs:4
",
        base = w.base.get(..10).unwrap()
    );
    let diags = lint(&w, &src);
    assert!(
        on_line(&src, &diags, "not committed").is_empty(),
        "{diags:#?}"
    );
    assert!(
        on_line(&src, &diags, "untracked file").is_empty(),
        "{diags:#?}"
    );
    assert!(on_line(&src, &diags, "base side").is_empty(), "{diags:#?}");
    let past = on_line(&src, &diags, "past the edited");
    assert_eq!(past.len(), 1, "{diags:#?}");
    assert_eq!(past[0].code, "unresolved-diff-link");
    assert!(!diags.iter().any(|d| d.code == "stale-delta"));
    assert_eq!(diags.len(), 1, "{diags:#?}");
}

#[test]
fn worktree_head_needs_a_checkout_holding_base() {
    let w = world();
    let src = "repo: demo\nbranch: feat\nbase: abcdef0123\nhead: worktree\n\n## intent\n  - runs\n\n## unit run — r\n+ - x @ Run.rs:1\n";
    let diags = lint(&w, src);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, "checkout-not-found");
    assert!(diags[0].message.contains("the base commit"));
}
