//! Resolving the name under a cursor to the index entries that define it.

use tower_lsp::lsp_types::Position;

use crate::indexer::IndexEntry;

/// The most locations one definition request answers with.
pub const DEFINITION_LIMIT: usize = 100;

/// Byte offset in `line` of the `char_col`th character, counted in Unicode
/// scalar values. `char_col` equal to the character count is valid and yields
/// `line.len()`; anything past that is `None`.
pub fn char_col_to_byte_col(line: &str, char_col: usize) -> Option<usize> {
    let mut chars = 0;
    for (byte_idx, _) in line.char_indices() {
        if chars == char_col {
            return Some(byte_idx);
        }
        chars += 1;
    }
    if chars == char_col {
        Some(line.len())
    } else {
        None
    }
}

/// The identifier (alphanumerics and `_`) the cursor rests on in `text`.
///
/// A cursor just past the end of a name still resolves that name. `None` when
/// the buffer has no such line, the column is outside the line, or neither the
/// character under the cursor nor the one before it is an identifier character.
pub fn identifier_at(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let byte_col = char_col_to_byte_col(line, pos.character as usize)?;
    let (head, tail) = line.split_at(byte_col);
    let (head, tail) = if tail.chars().next().is_some_and(is_identifier_char) {
        (head, tail)
    } else {
        let previous = head.chars().next_back().filter(|c| is_identifier_char(*c))?;
        let cut = head.len() - previous.len_utf8();
        (&line[..cut], &line[cut..])
    };
    let start = head.trim_end_matches(is_identifier_char).len();
    let end = head.len() + tail.find(|c| !is_identifier_char(c)).unwrap_or(tail.len());
    Some(line[start..end].to_string())
}

/// Every entry named exactly `name`, ordered by file then position so the
/// answer is stable across runs, and cut to [`DEFINITION_LIMIT`].
pub fn lookup<'a>(entries: &'a [IndexEntry], name: &str) -> Vec<&'a IndexEntry> {
    let mut found: Vec<&IndexEntry> = entries.iter().filter(|e| e.name == name).collect();
    found.sort_by(|a, b| {
        let key = |e: &IndexEntry| (e.range.start.line, e.range.start.character);
        a.file.cmp(&b.file).then_with(|| key(a).cmp(&key(b)))
    });
    found.truncate(DEFINITION_LIMIT);
    found
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    // Pins that a cursor in the middle of a name resolves the whole name, not the prefix before it.
    #[test]
    fn cursor_mid_name_resolves_whole_name() {
        assert_eq!(
            identifier_at("client = MarbleClient(host)", at(0, 12)).as_deref(),
            Some("MarbleClient")
        );
    }

    // Pins the left edge: the first character of a name is on the name.
    #[test]
    fn cursor_on_first_character_resolves_the_name() {
        assert_eq!(
            identifier_at("client = MarbleClient(host)", at(0, 9)).as_deref(),
            Some("MarbleClient")
        );
    }

    // Pins that a cursor resting just past a name, as after typing it, still resolves it.
    #[test]
    fn cursor_one_past_last_character_resolves_the_name() {
        assert_eq!(
            identifier_at("run_client(args)", at(0, 10)).as_deref(),
            Some("run_client")
        );
    }

    // Pins that a cursor between two non-identifier characters names nothing.
    #[test]
    fn cursor_on_punctuation_resolves_nothing() {
        assert_eq!(identifier_at("f(a, b)", at(0, 4)), None);
    }

    // Pins that a position below the buffer's last line answers nothing instead of panicking.
    #[test]
    fn line_past_end_of_buffer_resolves_nothing() {
        assert_eq!(identifier_at("one\ntwo\n", at(5, 0)), None);
    }

    // Pins that a column past the end of the line answers nothing instead of panicking.
    #[test]
    fn column_past_end_of_line_resolves_nothing() {
        assert_eq!(identifier_at("name", at(0, 9)), None);
    }

    // Pins that in `a.b` the cursor's own segment is the name, since a method is looked up alone.
    #[test]
    fn dotted_access_resolves_the_segment_under_the_cursor() {
        assert_eq!(identifier_at("obj.draw()", at(0, 5)).as_deref(), Some("draw"));
        assert_eq!(identifier_at("obj.draw()", at(0, 1)).as_deref(), Some("obj"));
    }

    // Pins that columns count characters, so a multibyte character earlier in the line does not shift the name.
    #[test]
    fn multibyte_characters_before_the_name_do_not_shift_it() {
        assert_eq!(identifier_at("é = make()", at(0, 5)).as_deref(), Some("make"));
    }
}
