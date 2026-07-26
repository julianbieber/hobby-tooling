//! Extracting the slash-command / skill reference token under the cursor.
//!
//! Three reference forms appear in prompt buffers and skill bodies:
//! `/name`, `` `name` `` (backtick-wrapped), and a bare `name`. Namespaced
//! names contain a `:` (e.g. `julian:mml-review`), so `:` is part of the word.

use tower_lsp::lsp_types::{Position, Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    /// `/name` — the leading slash is included in the replace range.
    Slash,
    /// `` `name` `` — the run sits just after a backtick.
    Backtick,
    /// A bare `name` with no sigil.
    Bare,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefToken {
    pub sigil: Sigil,
    /// The reference name as typed (without the sigil), e.g. `julian:mml-r`.
    pub query: String,
    /// The range a completion should replace: the name run only, never the
    /// leading sigil. Keeping the sigil outside the edit lets the client filter
    /// on the typed name and preserves a `/` the user already wrote.
    pub replace: Range,
}

/// Characters that make up a reference name. `:` is included so namespaced
/// names complete as one token; `/` is deliberately excluded — it is only the
/// leading sigil, never a mid-name character.
fn is_ref_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | ':')
}

/// Find the reference token whose run of name-characters ends at (or contains)
/// the cursor. Returns `None` when there is nothing referencable there — a bare
/// run with no sigil and an empty query yields `None`, but a lone `/` (just
/// typed) yields a `Slash` token with an empty query so completion can open.
pub fn ref_at(text: &str, pos: Position) -> Option<RefToken> {
    let line = text.lines().nth(pos.line as usize).unwrap_or("");
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let cursor = (pos.character as usize).min(len);

    // Run of name-chars ending at the cursor, extended rightwards through the
    // rest of the same token.
    let mut start = cursor;
    while start > 0 && is_ref_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = cursor;
    while end < len && is_ref_char(chars[end]) {
        end += 1;
    }

    let query: String = chars[start..end].iter().collect();

    // Classify by the character immediately left of the run. The replace range
    // always covers just the run (`start..end`), never the sigil.
    let sigil = match start.checked_sub(1).map(|i| chars[i]) {
        Some('/') => Sigil::Slash,
        Some('`') => Sigil::Backtick,
        _ => Sigil::Bare,
    };

    if sigil == Sigil::Bare && query.is_empty() {
        return None;
    }

    Some(RefToken {
        sigil,
        query,
        replace: Range {
            start: Position {
                line: pos.line,
                character: start as u32,
            },
            end: Position {
                line: pos.line,
                character: end as u32,
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, line: u32, character: u32) -> Option<RefToken> {
        ref_at(text, Position { line, character })
    }

    #[test]
    fn slash_form_mid_typing() {
        // "/mml" with cursor after the last char.
        let t = at("/mml", 0, 4).unwrap();
        assert_eq!(t.sigil, Sigil::Slash);
        assert_eq!(t.query, "mml");
        assert_eq!(t.replace.start.character, 1); // run only, excludes the slash
        assert_eq!(t.replace.end.character, 4);
    }

    #[test]
    fn lone_slash_opens_with_empty_query() {
        let t = at("/", 0, 1).unwrap();
        assert_eq!(t.sigil, Sigil::Slash);
        assert_eq!(t.query, "");
        // Empty run at the cursor (just after the slash): zero-width insert point.
        assert_eq!(t.replace.start.character, 1);
        assert_eq!(t.replace.end.character, 1);
    }

    #[test]
    fn namespaced_colon_kept_in_query() {
        // "run julian:mml-r" cursor at end.
        let t = at("run julian:mml-r", 0, 16).unwrap();
        assert_eq!(t.sigil, Sigil::Bare);
        assert_eq!(t.query, "julian:mml-r");
    }

    #[test]
    fn slash_namespaced() {
        let t = at("/julian:mml-review", 0, 18).unwrap();
        assert_eq!(t.sigil, Sigil::Slash);
        assert_eq!(t.query, "julian:mml-review");
        assert_eq!(t.replace.start.character, 1); // run only, excludes the slash
    }

    #[test]
    fn backtick_form() {
        let t = at("use `mml-extract`", 0, 16).unwrap();
        assert_eq!(t.sigil, Sigil::Backtick);
        assert_eq!(t.query, "mml-extract");
        assert_eq!(t.replace.start.character, 5); // after the backtick
    }

    #[test]
    fn cursor_inside_token_captures_whole() {
        // Cursor between "mml" and "-extract".
        let t = at("/mml-extract", 0, 4).unwrap();
        assert_eq!(t.query, "mml-extract");
        assert_eq!(t.replace.end.character, 12);
    }

    #[test]
    fn whitespace_yields_none() {
        // Cursor flanked by spaces (col 2 in "a  b"): empty run, no sigil.
        assert_eq!(at("a  b", 0, 2), None);
    }

    #[test]
    fn path_segment_reads_as_slash_ref() {
        // Ambiguous with a unix path; extraction still yields a Slash token —
        // the resolver, not this function, guards against treating /home as a ref.
        let t = at("/home/user", 0, 5).unwrap();
        assert_eq!(t.sigil, Sigil::Slash);
        assert_eq!(t.query, "home");
    }

    #[test]
    fn out_of_range_line_is_none() {
        assert_eq!(at("only one line", 5, 0), None);
    }
}
