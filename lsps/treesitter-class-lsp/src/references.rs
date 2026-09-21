//! Finding where a name is used across a workspace's files, and ordering the
//! places found.

use std::path::PathBuf;

use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Tree};

/// The most locations one references answer holds.
pub const REFERENCE_LIMIT: usize = 1000;

/// One place a name occurs: the file, the language that file was parsed as, and
/// the name's own range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub file: PathBuf,
    pub language: String,
    pub range: Range,
}

/// The range of every place `name` occurs in `tree`, in source order.
///
/// An occurrence is a named leaf node whose text is exactly `name`, so a
/// mention inside a comment or a string literal is not one: the leaf there is
/// the whole comment or the whole run of string text, not the name. A leaf whose
/// kind says `comment` or `string` is skipped as well, which covers the one case
/// text equality does not — a string whose entire content is the name. The rule
/// is the same for every grammar, so it cannot tell two unrelated things that
/// share a name apart.
pub fn occurrences_in(tree: &Tree, source: &[u8], name: &str) -> Vec<Range> {
    let mut found = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if is_occurrence(node, source, name) {
            found.push(range_of(node));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}

/// `refs` without every reference that sits exactly on one of `declarations`
/// (a file and a range).
pub fn without_declarations(
    refs: Vec<Reference>,
    declarations: &[(PathBuf, Range)],
) -> Vec<Reference> {
    refs.into_iter()
        .filter(|r| !declarations.iter().any(|(file, range)| *file == r.file && *range == r.range))
        .collect()
}

/// `refs` in a stable order, cut to [`REFERENCE_LIMIT`].
///
/// References in `language` (the language of the buffer asking) come first;
/// within each group the order is file, then position. The limit applies after
/// that order, so it never drops a reference in `language` for one in another. A
/// `language` of `None` puts every reference in the second group.
pub fn order(mut refs: Vec<Reference>, language: Option<&str>) -> Vec<Reference> {
    refs.sort_by(|a, b| {
        let key = |r: &Reference| {
            (
                Some(r.language.as_str()) != language,
                r.file.clone(),
                r.range.start.line,
                r.range.start.character,
            )
        };
        key(a).cmp(&key(b))
    });
    refs.truncate(REFERENCE_LIMIT);
    refs
}

fn is_occurrence(node: Node, source: &[u8], name: &str) -> bool {
    if !node.is_named() || node.child_count() != 0 {
        return false;
    }
    let kind = node.kind();
    if kind.contains("comment") || kind.contains("string") {
        return false;
    }
    &source[node.start_byte()..node.end_byte()] == name.as_bytes()
}

fn range_of(node: Node) -> Range {
    let start = node.start_position();
    let end = node.end_position();
    Range {
        start: Position {
            line: start.row as u32,
            character: start.column as u32,
        },
        end: Position {
            line: end.row as u32,
            character: end.column as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, character: u32) -> Range {
        Range {
            start: Position { line, character },
            end: Position {
                line,
                character: character + 1,
            },
        }
    }

    fn reference(language: &str, file: &str, line: u32, character: u32) -> Reference {
        Reference {
            file: PathBuf::from(file),
            language: language.to_string(),
            range: at(line, character),
        }
    }

    // Pins that the buffer's language wins over file order, so a `.py` buffer sees
    // Python uses before a `.proto` file that sorts first.
    #[test]
    fn order_puts_the_buffers_language_first() {
        let ordered = order(
            vec![
                reference("protobuf", "a.proto", 0, 0),
                reference("python", "z.py", 0, 0),
            ],
            Some("python"),
        );
        let files: Vec<_> = ordered.iter().map(|r| r.file.to_str().unwrap()).collect();
        assert_eq!(files, ["z.py", "a.proto"]);
    }

    // Pins the tie-break inside one language: file, then line, then column.
    #[test]
    fn order_sorts_by_file_then_position_within_a_language() {
        let ordered = order(
            vec![
                reference("python", "b.py", 1, 0),
                reference("python", "a.py", 9, 4),
                reference("python", "a.py", 9, 2),
                reference("python", "a.py", 3, 8),
            ],
            Some("python"),
        );
        let places: Vec<_> = ordered
            .iter()
            .map(|r| (r.file.to_str().unwrap(), r.range.start.line, r.range.start.character))
            .collect();
        assert_eq!(places, [("a.py", 3, 8), ("a.py", 9, 2), ("a.py", 9, 4), ("b.py", 1, 0)]);
    }

    // Pins that the cap cuts after ordering, so a reference in the buffer's language
    // survives however many other-language ones sort before it by file name.
    #[test]
    fn order_applies_the_limit_after_the_ordering() {
        let mut refs: Vec<Reference> = (0..REFERENCE_LIMIT as u32)
            .map(|line| reference("protobuf", "a.proto", line, 0))
            .collect();
        refs.push(reference("python", "z.py", 0, 0));
        let ordered = order(refs, Some("python"));
        assert_eq!(ordered.len(), REFERENCE_LIMIT);
        assert_eq!(ordered[0].file, PathBuf::from("z.py"));
    }

    // Pins that only an exact file-and-range match is a declaration: a use in the same
    // file at another range stays.
    #[test]
    fn without_declarations_drops_an_exact_match_only() {
        let kept = without_declarations(
            vec![
                reference("python", "lib.py", 0, 6),
                reference("python", "lib.py", 5, 11),
            ],
            &[(PathBuf::from("lib.py"), at(0, 6))],
        );
        assert_eq!(kept, [reference("python", "lib.py", 5, 11)]);
    }
}
