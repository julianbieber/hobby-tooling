//! Validates that every built-in graph query compiles against the actually
//! installed tree-sitter grammar. tree-sitter node names drift between grammar
//! versions, so this guards against a query silently breaking (the extractor
//! warns + skips on a bad query, which would silently drop all edges for that
//! language).
//!
//! Skips gracefully when the helix runtime / grammars are unavailable (e.g. a
//! CI box without `HELIX_RUNTIME`), so it never produces false failures.

use hobby_tree_extract::builtin_queries::builtin_graph_query;
use treesitter_class_lsp::grammars::GrammarCache;
use treesitter_class_lsp::languages::LanguageRegistry;

/// Representative file extensions whose resolved language has a built-in query.
const EXTS: &[&str] = &["rs", "py", "go", "java", "ts", "tsx", "js", "jsx", "scala"];

#[test]
fn builtin_graph_queries_compile_against_grammars() {
    let Ok(registry) = LanguageRegistry::load() else {
        eprintln!("skipping: helix language registry unavailable");
        return;
    };
    let grammars = GrammarCache::new();

    let mut checked = 0usize;
    let mut failures = Vec::new();

    for ext in EXTS {
        let fake = std::path::PathBuf::from(format!("probe.{ext}"));
        let Some(lang) = registry.resolve(&fake) else {
            eprintln!("skipping .{ext}: no language registered");
            continue;
        };
        let Some(src) = builtin_graph_query(&lang.name) else {
            eprintln!("skipping .{ext} ({}): no built-in graph query", lang.name);
            continue;
        };
        let grammar = match grammars.get_or_load(&registry, lang) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("skipping .{ext} ({}): grammar unavailable: {e}", lang.name);
                continue;
            }
        };
        match tree_sitter::Query::new(&grammar.language, src) {
            Ok(_) => {
                checked += 1;
                eprintln!("ok: {} ({ext})", lang.name);
            }
            Err(e) => failures.push(format!("{} (.{ext}): {e}", lang.name)),
        }
    }

    assert!(
        failures.is_empty(),
        "built-in graph queries failed to compile:\n{}",
        failures.join("\n")
    );
    eprintln!("compiled {checked} built-in graph queries");
}
