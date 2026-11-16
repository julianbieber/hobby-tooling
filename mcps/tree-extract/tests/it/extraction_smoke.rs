//! End-to-end extraction smoke test on a small polyglot fixture: confirms the
//! extractor surfaces calls, imports, and inheritance from real source via the
//! installed grammars. Skips gracefully if the helix runtime is unavailable.

use hobby_tree_extract::extractor::{Extractor, RefKind};
use hobby_tree_extract::graph::{EdgeKind, build_graph};

fn write(dir: &std::path::Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

#[test]
fn extracts_calls_imports_inheritance() {
    let Ok(extractor) = Extractor::new() else {
        eprintln!("skipping: helix registry unavailable");
        return;
    };
    let dir = tempfile::tempdir().unwrap();

    write(
        dir.path(),
        "a.rs",
        r#"
use crate::helpers::greet;

pub struct Greeter;

impl Greeter {
    pub fn run(&self) {
        greet();
        helper();
    }
}

fn helper() {}
"#,
    );
    write(
        dir.path(),
        "b.py",
        r#"
from app.utils import load

class Animal:
    pass

class Dog(Animal):
    def speak(self):
        load()
"#,
    );

    let r = extractor.extract_repo(dir.path());

    // Symbols from both languages.
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    for expect in ["Greeter", "helper", "Animal", "Dog"] {
        assert!(
            names.contains(&expect),
            "missing symbol {expect}: {names:?}"
        );
    }

    // Rust calls.
    let calls: Vec<&str> = r
        .references
        .iter()
        .filter(|x| x.ref_kind == RefKind::Call)
        .map(|x| x.name.as_str())
        .collect();
    assert!(
        calls.contains(&"greet"),
        "rust call greet missing: {calls:?}"
    );
    assert!(
        calls.contains(&"helper"),
        "rust call helper missing: {calls:?}"
    );
    assert!(calls.contains(&"load"), "py call load missing: {calls:?}");

    // Python inheritance: Dog(Animal) → Implementation reference to Animal.
    let impls: Vec<&str> = r
        .references
        .iter()
        .filter(|x| x.ref_kind == RefKind::Implementation)
        .map(|x| x.name.as_str())
        .collect();
    assert!(
        impls.contains(&"Animal"),
        "py inheritance Animal missing: {impls:?}"
    );

    // Imports from both languages.
    let modules: Vec<&str> = r.imports.iter().map(|x| x.module.as_str()).collect();
    assert!(
        modules.iter().any(|m| m.contains("helpers")),
        "rust import missing: {modules:?}"
    );
    assert!(
        modules.iter().any(|m| m.contains("app.utils")),
        "py import missing: {modules:?}"
    );
}

/// Regression: helix's Rust/Java tags put `@definition.*` on the name
/// identifier, so the captured node is a single line. The extractor must climb
/// to the enclosing declaration for `line_end`, otherwise every symbol collapses
/// to one line and call-site attribution / containment nesting break.
#[test]
fn rust_symbol_ranges_span_body_and_calls_attribute_to_method() {
    let Ok(extractor) = Extractor::new() else {
        eprintln!("skipping: helix registry unavailable");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.rs",
        r#"
pub struct Greeter;

impl Greeter {
    pub fn run(&self) {
        helper();
    }
}

fn helper() {}
"#,
    );
    let r = extractor.extract_repo(dir.path());

    let run = r
        .symbols
        .iter()
        .find(|s| s.name == "run")
        .expect("run method extracted");
    assert!(
        run.line_end > run.line_start,
        "run's range collapsed to one line: {}..={}",
        run.line_start,
        run.line_end
    );

    // The call to `helper` inside `run` must be attributed to `run`, not the
    // file node.
    let graph = build_graph(&r, dir.path(), None);
    let helper_id = graph
        .nodes
        .iter()
        .find(|n| n.name == "helper")
        .map(|n| n.id.clone())
        .expect("helper node");
    let run_id = format!("a.rs:L{}", run.line_start);
    let edge = graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Calls && e.to == helper_id)
        .expect("calls edge to helper");
    assert_eq!(
        edge.from, run_id,
        "helper() call attributed to {} instead of the enclosing method {run_id}",
        edge.from
    );
}

/// Regression: Scala `def` methods were not captured by the vendored tags
/// query, so they were absent from the symbol map and graph entirely.
#[test]
fn scala_def_methods_are_extracted() {
    let Ok(extractor) = Extractor::new() else {
        eprintln!("skipping: helix registry unavailable");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "S.scala",
        r#"
class Service {
  def handle(x: Int): Int = {
    compute(x)
  }

  def compute(x: Int): Int = x + 1
}
"#,
    );
    let r = extractor.extract_repo(dir.path());
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    for expect in ["Service", "handle", "compute"] {
        assert!(
            names.contains(&expect),
            "missing scala symbol {expect}: {names:?}"
        );
    }
}
