//! Nesting of a real file's definitions into document symbols, checked over a
//! real grammar. Skipped silently when the helix runtime isn't available.

use std::path::PathBuf;

use treesitter_class_lsp::document_symbol;
use treesitter_class_lsp::indexer::Indexer;
use treesitter_class_lsp::languages::LanguageRegistry;

// Pins that a Python class holds its method as a child while a module-level def is a second root.
#[test]
fn python_method_nests_under_its_class_and_module_function_stays_a_root() {
    let registry = match LanguageRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: helix runtime not available: {e}");
            return;
        }
    };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.py");
    let Some(lang) = registry.resolve(&path) else {
        eprintln!("skipping: no python language configured");
        return;
    };
    let lang = lang.clone();
    let indexer = Indexer::new(registry);

    let entries = indexer.index_file(&path, &lang).unwrap();
    let roots = document_symbol::tree(&entries);

    let names: Vec<&str> = roots.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Greeter", "shout"]);
    let children = roots[0].children.as_ref().expect("Greeter has children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "greet");
    assert!(roots[1].children.is_none());
}
