//! Shared line-range containment logic.
//!
//! Both the markdown renderer (which nests methods under their class) and the
//! graph builder (which attributes a call site to its enclosing function and
//! emits `Contains` edges) need one consistent notion of "symbol A is nested
//! inside symbol B". That notion lives here so the two consumers can never
//! drift apart.
//!
//! Containment is purely range-based (same file, inner span within outer
//! span). It is a heuristic: a symbol whose extracted line range is wrong
//! (macro-generated code, unusual formatting) can be mis-nested. This is the
//! same heuristic the renderer has always shipped.

use std::path::Path;

use crate::extractor::Symbol;

/// True if `inner`'s line range sits strictly within `outer`'s, in the same
/// file. Mirrors the renderer's original nesting rule exactly: the inner symbol
/// must start *after* the outer one and end at or before it.
pub fn is_inside(inner: &Symbol, outer: &Symbol) -> bool {
    inner.file == outer.file
        && inner.line_start > outer.line_start
        && inner.line_end <= outer.line_end
}

/// The innermost symbol in `defs` whose `[line_start..=line_end]` range
/// contains `line` in `file`. "Innermost" = smallest line span, so a reference
/// inside a method resolves to the method rather than its enclosing class.
/// Returns `None` when nothing encloses the point (a top-level / file-scope
/// reference).
pub fn enclosing_symbol<'a>(file: &Path, line: u32, defs: &'a [Symbol]) -> Option<&'a Symbol> {
    defs.iter()
        .filter(|s| s.file == file && s.line_start <= line && line <= s.line_end)
        .min_by_key(|s| s.line_end.saturating_sub(s.line_start))
}

/// For each def, the index of its innermost strictly-enclosing def (its parent
/// container), or `None` if it is top-level. Used to emit `Contains` edges.
///
/// O(n²) with a cheap same-file short-circuit; callers with very large symbol
/// counts should bucket by file first, but for a single repo (hundreds–low
/// thousands of symbols) this is negligible.
pub fn parent_of_each(defs: &[Symbol]) -> Vec<Option<usize>> {
    defs.iter()
        .enumerate()
        .map(|(i, s)| {
            defs.iter()
                .enumerate()
                .filter(|(j, c)| *j != i && is_inside(s, c))
                .min_by_key(|(_, c)| c.line_end.saturating_sub(c.line_start))
                .map(|(j, _)| j)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::SymbolKind;
    use std::path::PathBuf;

    fn sym(name: &str, kind: SymbolKind, file: &str, start: u32, end: u32) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file: PathBuf::from(file),
            line_start: start,
            line_end: end,
            docstring: None,
        }
    }

    #[test]
    fn is_inside_respects_file_and_range() {
        let outer = sym("C", SymbolKind::Class, "a.rs", 1, 20);
        let inner = sym("m", SymbolKind::Method, "a.rs", 5, 8);
        let other_file = sym("m", SymbolKind::Method, "b.rs", 5, 8);
        let same_line = sym("C", SymbolKind::Class, "a.rs", 1, 20);
        assert!(is_inside(&inner, &outer));
        assert!(!is_inside(&other_file, &outer));
        // identical range is not "strictly inside"
        assert!(!is_inside(&same_line, &outer));
    }

    #[test]
    fn enclosing_picks_innermost() {
        let defs = vec![
            sym("Outer", SymbolKind::Class, "a.rs", 1, 50),
            sym("method", SymbolKind::Method, "a.rs", 10, 20),
            sym("Other", SymbolKind::Class, "b.rs", 1, 50),
        ];
        let f = PathBuf::from("a.rs");
        // line 15 sits inside both Outer and method → innermost = method
        let e = enclosing_symbol(&f, 15, &defs).unwrap();
        assert_eq!(e.name, "method");
        // line 5 sits inside Outer only
        let e = enclosing_symbol(&f, 5, &defs).unwrap();
        assert_eq!(e.name, "Outer");
        // line 99 sits inside nothing
        assert!(enclosing_symbol(&f, 99, &defs).is_none());
    }

    #[test]
    fn parent_of_each_finds_innermost_container() {
        let defs = vec![
            sym("Outer", SymbolKind::Class, "a.rs", 1, 50),
            sym("method", SymbolKind::Method, "a.rs", 10, 20),
            sym("free", SymbolKind::Function, "a.rs", 60, 70),
        ];
        let parents = parent_of_each(&defs);
        assert_eq!(parents[0], None); // Outer is top-level
        assert_eq!(parents[1], Some(0)); // method ∈ Outer
        assert_eq!(parents[2], None); // free fn is top-level
    }
}
