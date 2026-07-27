//! Supplementary tree-sitter queries that fill the gaps helix's `tags.scm`
//! leaves for code-graph extraction.
//!
//! Helix tags give us definitions everywhere and call references for some
//! languages (Python, Go, JavaScript), but never imports, and never calls or
//! inheritance for Rust/Java/Scala/TypeScript. These queries are run as a
//! *second* cursor over the same parse tree; the resulting references/imports
//! are merged with whatever the tags query produced. Overlap is harmless —
//! graph edges are de-duplicated.
//!
//! Capture conventions (matched by `extractor::{ref_kind_from_capture}` and the
//! `import.module` / `import.symbol` / `name` capture names):
//!   - `@name`                     the referenced symbol's identifier
//!   - `@reference.call`           anchor node for a call
//!   - `@reference.class`          anchor node for an instantiation (`new X`)
//!   - `@reference.implementation` anchor node for an inheritance clause
//!   - `@import.module`            the imported module path/string
//!   - `@import.symbol`            (optional) the specific imported item
//!
//! Node names here are validated against the installed helix grammars by
//! `tests`/`builtin_queries_compile`. A query that fails to compile against a
//! grammar degrades gracefully (warn + skip) rather than crashing.

/// The supplementary graph query for a language, if we ship one.
pub fn builtin_graph_query(lang: &str) -> Option<&'static str> {
    match lang {
        "rust" => Some(include_str!("builtin_queries/rust_graph.scm")),
        "python" => Some(include_str!("builtin_queries/python_graph.scm")),
        "go" => Some(include_str!("builtin_queries/go_graph.scm")),
        "java" => Some(include_str!("builtin_queries/java_graph.scm")),
        "typescript" | "tsx" => Some(include_str!("builtin_queries/typescript_graph.scm")),
        "javascript" | "jsx" => Some(include_str!("builtin_queries/javascript_graph.scm")),
        "scala" => Some(include_str!("builtin_queries/scala_graph.scm")),
        _ => None,
    }
}

/// Languages for which we ship a built-in graph query (used by tests).
pub const GRAPH_QUERY_LANGS: &[&str] = &[
    "rust",
    "python",
    "go",
    "java",
    "typescript",
    "tsx",
    "javascript",
    "jsx",
    "scala",
];
