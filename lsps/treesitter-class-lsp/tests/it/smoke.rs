//! End-to-end smoke test: load a real helix grammar and index the current
//! crate's own `src/` directory. Skipped silently when the helix runtime isn't
//! available.

use std::path::PathBuf;

use treesitter_class_lsp::indexer::Indexer;
use treesitter_class_lsp::languages::LanguageRegistry;

#[test]
fn indexes_this_crate_and_finds_known_definitions() {
    let registry = match LanguageRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: helix runtime not available: {e}");
            return;
        }
    };
    let indexer = Indexer::new(registry);

    let this_crate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let index = indexer.index_root(&this_crate);

    let names: Vec<&str> = index.entries().iter().map(|e| e.name.as_str()).collect();
    // `Backend` is defined in src/server.rs; `Index` and `Indexer` in
    // src/indexer.rs; `LoadedGrammar` in src/grammars.rs.
    for expected in [
        "Backend",
        "Index",
        "Indexer",
        "LoadedGrammar",
        "LanguageRegistry",
    ] {
        assert!(
            names.contains(&expected),
            "expected {expected} in index; got {} entries: {:?}",
            names.len(),
            names,
        );
    }
}
