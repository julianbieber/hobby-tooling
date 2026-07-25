//! Byte spans and byte-offset → (line, column) conversion.
//!
//! Every AST node and diagnostic carries a [`Span`] of byte offsets into the
//! source. The LSP server (task 07) needs LSP `Position`s (0-based line +
//! UTF-16 character), so [`LineIndex`] converts an offset into `(line, col)`.

use serde::Serialize;

/// A half-open byte range `[start, end)` into the source text. `Default` is the
/// empty span at offset 0, so span-carrying AST nodes can derive `Default`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// An empty span at a single offset (used for "missing here" diagnostics).
    pub fn at(offset: usize) -> Span {
        Span {
            start: offset,
            end: offset,
        }
    }
}

/// A value paired with the source span it was parsed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Spanned<T> {
        Spanned { value, span }
    }
}

/// A 0-based line/column position. `character` is a UTF-16 code-unit offset
/// within the line, matching the LSP default position encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// Precomputed line-start byte offsets, so any byte offset can be mapped to a
/// [`Position`] in `O(log n)`. Built once per document.
pub struct LineIndex {
    /// Byte offset of the start of each line. `line_starts[0]` is always 0.
    line_starts: Vec<usize>,
    /// The full source, needed to count UTF-16 units within the target line.
    text: String,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            line_starts,
            text: text.to_owned(),
        }
    }

    /// Convert a byte offset into a 0-based [`Position`]. An offset past the end
    /// of the text clamps to the end.
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        // Largest line whose start is <= offset.
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next - 1,
        };
        let line_start = self.line_starts[line];
        let character = self.text[line_start..offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>();
        Position {
            line: line as u32,
            character: character as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_across_lines() {
        let src = "ab\ncde\n\nf";
        let idx = LineIndex::new(src);
        assert_eq!(
            idx.position(0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            idx.position(2),
            Position {
                line: 0,
                character: 2
            }
        ); // end of "ab"
        assert_eq!(
            idx.position(3),
            Position {
                line: 1,
                character: 0
            }
        ); // start of "cde"
        assert_eq!(
            idx.position(5),
            Position {
                line: 1,
                character: 2
            }
        );
        assert_eq!(
            idx.position(7),
            Position {
                line: 2,
                character: 0
            }
        ); // blank line
        assert_eq!(
            idx.position(8),
            Position {
                line: 3,
                character: 0
            }
        ); // "f"
    }

    #[test]
    fn utf16_characters() {
        // "é" is 2 bytes UTF-8 but 1 UTF-16 unit; "𝄞" is 4 bytes / 2 UTF-16 units.
        let src = "é𝄞x";
        let idx = LineIndex::new(src);
        let after = src.len();
        assert_eq!(
            idx.position(after),
            Position {
                line: 0,
                character: 4
            }
        );
    }

    #[test]
    fn offset_past_end_clamps() {
        let idx = LineIndex::new("ab");
        assert_eq!(
            idx.position(999),
            Position {
                line: 0,
                character: 2
            }
        );
    }
}
