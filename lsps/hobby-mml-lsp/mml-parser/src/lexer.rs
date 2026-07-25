//! Line scanner: source text → a vector of classified [`Line`]s.
//!
//! MML is line-oriented (grammar §1), so the parser works over whole lines. The
//! lexer records, per line: byte offsets, indentation (spaces only — a tab in
//! the leading whitespace is flagged), and a coarse [`LineKind`]. Comment and
//! blank lines are kept (not discarded) so the parser can compute spans and skip
//! them where appropriate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Only whitespace.
    Blank,
    /// First non-space char is `#`, and it is not a column-0 `##` section header.
    Comment,
    /// A column-0 `##` header (e.g. `## modules`). Name may be unknown.
    Section,
    /// Anything else — actual content the parser interprets.
    Content,
}

#[derive(Debug, Clone)]
pub struct Line<'a> {
    /// 0-based line number.
    pub line_no: usize,
    /// Byte offset of the start of the line (including indentation).
    pub start: usize,
    /// Byte offset of the first non-whitespace character (== end for blank lines).
    pub content_start: usize,
    /// Number of leading whitespace characters (each space or tab counts as one).
    pub indent: usize,
    /// A tab appeared in the leading whitespace (grammar §1: spaces only).
    pub has_tab_indent: bool,
    /// The line with leading and trailing whitespace removed.
    pub content: &'a str,
    pub kind: LineKind,
}

impl<'a> Line<'a> {
    /// Byte span of `content` within the source.
    pub fn content_span(&self) -> (usize, usize) {
        (self.content_start, self.content_start + self.content.len())
    }
}

/// Split `src` into classified lines. Every source line produces exactly one
/// [`Line`] (including trailing empty line handling matches Rust's `lines()`:
/// a final newline does not yield an extra empty line).
pub fn lex(src: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for (line_no, raw) in src.split_inclusive('\n').enumerate() {
        // Strip the trailing '\n' and an optional '\r' for content purposes,
        // but keep `offset` advancing over the full raw slice.
        let without_nl = raw.strip_suffix('\n').unwrap_or(raw);
        let text = without_nl.strip_suffix('\r').unwrap_or(without_nl);

        let mut indent = 0;
        let mut has_tab_indent = false;
        let mut ws_bytes = 0;
        for c in text.chars() {
            match c {
                ' ' => {
                    indent += 1;
                    ws_bytes += 1;
                }
                '\t' => {
                    indent += 1;
                    has_tab_indent = true;
                    ws_bytes += 1;
                }
                _ => break,
            }
        }

        let content_start = offset + ws_bytes;
        let content = text[ws_bytes..].trim_end();

        let kind = if content.is_empty() {
            LineKind::Blank
        } else if indent == 0 && content.starts_with("##") {
            LineKind::Section
        } else if content.starts_with('#') {
            LineKind::Comment
        } else {
            LineKind::Content
        };

        lines.push(Line {
            line_no,
            start: offset,
            content_start,
            indent,
            has_tab_indent,
            content,
            kind,
        });

        offset += raw.len();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_kinds() {
        let src = "system: x\n\n# a comment\n## modules\n  - indented\n";
        let lines = lex(src);
        assert_eq!(lines[0].kind, LineKind::Content);
        assert_eq!(lines[1].kind, LineKind::Blank);
        assert_eq!(lines[2].kind, LineKind::Comment);
        assert_eq!(lines[3].kind, LineKind::Section);
        assert_eq!(lines[4].kind, LineKind::Content);
        assert_eq!(lines[4].indent, 2);
    }

    #[test]
    fn indented_double_hash_is_comment_not_section() {
        let lines = lex("  ## not a section\n");
        assert_eq!(lines[0].kind, LineKind::Comment);
    }

    #[test]
    fn hash_inside_value_is_not_a_comment() {
        let lines = lex("- respond with #hashtag\n");
        assert_eq!(lines[0].kind, LineKind::Content);
    }

    #[test]
    fn tab_indent_flagged() {
        let lines = lex("\t- step\n");
        assert!(lines[0].has_tab_indent);
    }

    #[test]
    fn content_span_is_accurate() {
        let src = "  hello\n";
        let lines = lex(src);
        let (s, e) = lines[0].content_span();
        assert_eq!(&src[s..e], "hello");
    }
}
