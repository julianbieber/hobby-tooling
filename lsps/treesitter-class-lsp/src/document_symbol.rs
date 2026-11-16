//! Turning one file's flat list of definitions into the nested shape a
//! document symbol request answers with.

use tower_lsp::lsp_types::{DocumentSymbol, Range};

use crate::indexer::{IndexEntry, range_contains};

/// Nests `entries` — the definitions of a single file — by range containment:
/// a definition sits under the innermost definition whose range contains it,
/// and one no other contains is a root. Siblings and roots are in document
/// order. An entry's `name_range` becomes its selection range unless it lies
/// outside the entry's `range`, in which case `range` stands in for it.
pub fn tree(entries: &[IndexEntry]) -> Vec<DocumentSymbol> {
    let mut sorted: Vec<&IndexEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| {
        (
            e.range.start.line,
            e.range.start.character,
            std::cmp::Reverse(e.range.end.line),
            std::cmp::Reverse(e.range.end.character),
        )
    });

    let mut roots: Vec<DocumentSymbol> = Vec::new();
    let mut open: Vec<DocumentSymbol> = Vec::new();
    for entry in sorted {
        while open
            .last()
            .is_some_and(|parent| !range_contains(&parent.range, &entry.range))
        {
            close_innermost(&mut open, &mut roots);
        }
        open.push(node(entry));
    }
    while !open.is_empty() {
        close_innermost(&mut open, &mut roots);
    }
    roots
}

fn close_innermost(open: &mut Vec<DocumentSymbol>, roots: &mut Vec<DocumentSymbol>) {
    let Some(done) = open.pop() else { return };
    match open.last_mut() {
        Some(parent) => parent.children.get_or_insert_with(Vec::new).push(done),
        None => roots.push(done),
    }
}

fn node(entry: &IndexEntry) -> DocumentSymbol {
    let selection_range = if within(&entry.range, &entry.name_range) {
        entry.name_range
    } else {
        entry.range
    };
    #[expect(
        deprecated,
        reason = "the protocol's deprecated field still has to be set"
    )]
    let symbol = DocumentSymbol {
        name: entry.name.clone(),
        detail: None,
        kind: entry.kind,
        tags: None,
        deprecated: None,
        range: entry.range,
        selection_range,
        children: None,
    };
    symbol
}

fn within(outer: &Range, inner: &Range) -> bool {
    outer == inner || range_contains(outer, inner)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tower_lsp::lsp_types::{Position, SymbolKind};

    use super::*;

    fn range(start: (u32, u32), end: (u32, u32)) -> Range {
        Range {
            start: Position {
                line: start.0,
                character: start.1,
            },
            end: Position {
                line: end.0,
                character: end.1,
            },
        }
    }

    fn entry(name: &str, kind: SymbolKind, range: Range, name_range: Range) -> IndexEntry {
        IndexEntry {
            name: name.to_string(),
            language: "python".to_string(),
            kind,
            file: PathBuf::from("a.py"),
            range,
            name_range,
            container: None,
        }
    }

    fn at(name: &str, kind: SymbolKind, start: u32, end: u32) -> IndexEntry {
        let r = range((start, 0), (end, 0));
        entry(name, kind, r, range((start, 4), (start, 8)))
    }

    // Pins that a method inside a class's range becomes that class's child rather than a second root.
    #[test]
    fn method_inside_a_class_nests_under_it() {
        let entries = [
            at("greet", SymbolKind::METHOD, 2, 4),
            at("Greeter", SymbolKind::CLASS, 0, 5),
        ];

        let roots = tree(&entries);

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name, "Greeter");
        let children = roots[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "greet");
    }

    // Pins that a definition outside every other definition stays at the top level, in document order.
    #[test]
    fn definition_outside_every_class_stays_a_root() {
        let entries = [
            at("shout", SymbolKind::FUNCTION, 7, 9),
            at("Greeter", SymbolKind::CLASS, 0, 5),
        ];

        let roots = tree(&entries);

        let names: Vec<&str> = roots.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Greeter", "shout"]);
        assert!(roots.iter().all(|r| r.children.is_none()));
    }

    // Pins that nesting continues below the class level, so a def inside a method is the method's child.
    #[test]
    fn definition_inside_a_method_nests_two_deep() {
        let entries = [
            at("Greeter", SymbolKind::CLASS, 0, 9),
            at("greet", SymbolKind::METHOD, 1, 8),
            at("inner", SymbolKind::FUNCTION, 3, 5),
            at("farewell", SymbolKind::METHOD, 8, 9),
        ];

        let roots = tree(&entries);

        assert_eq!(roots.len(), 1);
        let methods = roots[0].children.as_ref().unwrap();
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["greet", "farewell"]);
        let inner = methods[0].children.as_ref().unwrap();
        assert_eq!(inner[0].name, "inner");
    }

    // Pins that a name range outside its definition is replaced by the definition's range, since the protocol requires the selection range to lie inside the range.
    #[test]
    fn name_range_outside_the_range_falls_back_to_the_range() {
        let r = range((2, 0), (4, 0));
        let stray = entry("greet", SymbolKind::METHOD, r, range((9, 0), (9, 5)));

        let roots = tree(&[stray]);

        assert_eq!(roots[0].selection_range, r);
    }

    // Pins that a name range inside the definition is passed through as the selection range.
    #[test]
    fn name_range_inside_the_range_is_the_selection_range() {
        let e = at("greet", SymbolKind::METHOD, 2, 4);

        let roots = tree(std::slice::from_ref(&e));

        assert_eq!(roots[0].selection_range, e.name_range);
    }
}
