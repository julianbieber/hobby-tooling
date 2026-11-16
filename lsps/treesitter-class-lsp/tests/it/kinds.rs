//! Kind mapping of indexed definitions, checked over a real grammar. Skipped
//! silently when the helix runtime isn't available.

use std::path::PathBuf;

use tower_lsp::lsp_types::SymbolKind;
use treesitter_class_lsp::indexer::Indexer;
use treesitter_class_lsp::languages::LanguageRegistry;

// Pins that a `def` inside a class is reported as METHOD and a module-level `def` as FUNCTION.
#[test]
fn classes_functions_and_methods_get_their_own_kinds() {
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
    let kind_of = |name: &str| entries.iter().find(|e| e.name == name).map(|e| e.kind);

    assert_eq!(kind_of("Greeter"), Some(SymbolKind::CLASS));
    assert_eq!(kind_of("greet"), Some(SymbolKind::METHOD));
    assert_eq!(kind_of("shout"), Some(SymbolKind::FUNCTION));
}
