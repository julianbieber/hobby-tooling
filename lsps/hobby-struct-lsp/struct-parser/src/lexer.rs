//! Splitting source text into the significant lines the parser works over.
//!
//! `.hstruct` is line-oriented, so this is the whole lexical layer: it strips
//! comments and blank lines, records each line's indent and byte offset, and
//! classifies the leading marker that decides how the parser reads it.

use crate::span::Span;

/// What the first non-space character of a line says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// `##` at column 0: a section header.
    Section,
    /// `!`: an assertion.
    Assertion,
    /// `?`: a waiver.
    Waiver,
    /// Anything else: prose, a header key, or a module record or field.
    Plain,
}

/// One significant line: its marker, its text with the marker stripped, its
/// indent in spaces, and the span of the text within the source.
#[derive(Debug, Clone)]
pub struct Line {
    pub marker: Marker,
    pub text: String,
    pub indent: usize,
    pub span: Span,
    /// Whether the line's indentation used a tab, which the grammar forbids.
    pub tab_indent: bool,
}

/// Split source into significant lines, dropping blanks and comments.
///
/// A `#` at the first non-space column starts a comment, except `##` at column
/// 0, which is always a section header and never a comment.
pub fn lines(src: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for raw in src.split('\n') {
        let start = offset;
        offset += raw.len() + 1;

        let trimmed_end = raw.trim_end();
        if trimmed_end.is_empty() {
            continue;
        }

        let body = trimmed_end.trim_start_matches([' ', '\t']);
        let lead = trimmed_end.strip_suffix(body).unwrap_or_default();
        let tab_indent = lead.contains('\t');
        let indent = lead.chars().filter(|c| *c == ' ').count();

        let is_section = indent == 0 && !tab_indent && body.starts_with("##");
        if !is_section && body.starts_with('#') {
            continue;
        }

        let (marker, rest) = if is_section {
            (Marker::Section, body.trim_start_matches('#').trim())
        } else if let Some(rest) = body.strip_prefix('!') {
            (Marker::Assertion, rest.trim())
        } else if let Some(rest) = body.strip_prefix('?') {
            (Marker::Waiver, rest.trim())
        } else {
            (Marker::Plain, body.trim())
        };

        let text_start = start + lead.len() + (body.len() - rest.len());
        out.push(Line {
            marker,
            text: rest.to_owned(),
            indent,
            span: Span::new(text_start, text_start + rest.len()),
            tab_indent,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Blank lines and comments carry nothing, so the parser must never see them.
    #[test]
    fn blanks_and_comments_are_dropped() {
        let ls = lines("repo: wusel\n\n# a comment\n  # indented comment\nroot: src\n");
        assert_eq!(ls.len(), 2);
        assert_eq!(ls[0].text, "repo: wusel");
        assert_eq!(ls[1].text, "root: src");
    }

    // `##` at column 0 is a section header, which is the one `#` that is not a
    // comment; getting this backwards would swallow every section in the file.
    #[test]
    fn section_header_is_not_a_comment() {
        let ls = lines("## layers\n");
        assert_eq!(ls.len(), 1);
        assert_eq!(ls[0].marker, Marker::Section);
        assert_eq!(ls[0].text, "layers");
    }

    // The two markers are what the whole language rests on, and both may be
    // indented, since a waiver often sits under the assertion it waives.
    #[test]
    fn markers_are_stripped_and_classified() {
        let ls = lines("! no import cycles\n  ? a -> b\nplain prose\n");
        assert_eq!(ls[0].marker, Marker::Assertion);
        assert_eq!(ls[0].text, "no import cycles");
        assert_eq!(ls[1].marker, Marker::Waiver);
        assert_eq!(ls[1].text, "a -> b");
        assert_eq!(ls[1].indent, 2);
        assert_eq!(ls[2].marker, Marker::Plain);
    }

    // Spans point at the text after the marker, so a diagnostic underlines the
    // claim rather than the punctuation introducing it.
    #[test]
    fn span_covers_the_text_after_the_marker() {
        let src = "! no import cycles\n";
        let ls = lines(src);
        assert_eq!(
            src.get(ls[0].span.start..ls[0].span.end),
            Some("no import cycles")
        );
    }

    // Tabs are illegal indentation and the parser reports them, so the lexer has
    // to record their use rather than silently normalising it away.
    #[test]
    fn tab_indentation_is_recorded() {
        let ls = lines("\towns  a thing\n");
        assert!(ls[0].tab_indent);
    }
}
