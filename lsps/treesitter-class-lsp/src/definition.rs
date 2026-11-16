//! Resolving the name under a cursor to the index entries that define it.

use std::path::Path;

use regex::Regex;
use tower_lsp::lsp_types::{Position, Range};

use crate::indexer::{IndexEntry, can_contain_methods};

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
    let line = line_at(text, pos.line)?;
    let (start, end) = identifier_span(line, pos.character)?;
    line.get(start..end).map(str::to_string)
}

/// The `line`th line of `text`, counted from zero.
pub fn line_at(text: &str, line: u32) -> Option<&str> {
    text.lines().nth(usize::try_from(line).ok()?)
}

/// Byte offset in `line` of the character at LSP column `character`; `None`
/// past the end of the line.
pub fn byte_col_at(line: &str, character: u32) -> Option<usize> {
    char_col_to_byte_col(line, usize::try_from(character).ok()?)
}

fn identifier_span(line: &str, character: u32) -> Option<(usize, usize)> {
    let byte_col = byte_col_at(line, character)?;
    let (head, tail) = line.split_at_checked(byte_col)?;
    let (head, tail) = if tail.chars().next().is_some_and(is_identifier_char) {
        (head, tail)
    } else {
        let previous = head
            .chars()
            .next_back()
            .filter(|c| is_identifier_char(*c))?;
        line.split_at_checked(head.len() - previous.len_utf8())?
    };
    let start = head.trim_end_matches(is_identifier_char).len();
    let end = head.len() + tail.find(|c| !is_identifier_char(c)).unwrap_or(tail.len());
    Some((start, end))
}

/// What stands left of the name under the cursor in `recv.name`, `recv->name`
/// or `recv::name`.
#[derive(Debug, PartialEq, Eq)]
pub enum Receiver {
    /// `self` or `this`: the class whose body holds the cursor.
    SelfLike,
    /// Any other identifier, spelled as written.
    Named(String),
}

/// The receiver of the member access the cursor is on.
///
/// The separator has to touch both the name and the receiver, with no
/// whitespace either side. `None` when the name is not preceded by `.`, `->` or
/// `::`, when nothing identifier-like sits before the separator, or when the
/// cursor is not on an identifier.
pub fn receiver_at(text: &str, pos: Position) -> Option<Receiver> {
    let line = line_at(text, pos.line)?;
    let (name_start, _) = identifier_span(line, pos.character)?;
    let before = line.get(..name_start)?;
    let before = before
        .strip_suffix("->")
        .or_else(|| before.strip_suffix("::"))
        .or_else(|| before.strip_suffix('.'))?;
    let receiver = before.get(before.trim_end_matches(is_identifier_char).len()..)?;
    match receiver {
        "" => None,
        "self" | "this" => Some(Receiver::SelfLike),
        other => Some(Receiver::Named(other.to_string())),
    }
}

/// The name of the class a `receiver` at `pos` stands for, when the buffer or
/// the index says so.
///
/// `SelfLike` is the innermost class-like definition of `file` containing `pos`.
/// A `Named` receiver is, in order: itself, when the index defines a class-like
/// entry of that name; else the type of a `name: Type` annotation; else the
/// callee of a `name = Type(...)` or `name = new Type(...)` assignment. The
/// last dotted segment of the type is used, so `pkg.Type` yields `Type`. Among
/// several annotations or assignments the nearest at or above `pos` wins, then
/// the nearest below. `None` when nothing of these applies.
pub fn receiver_type(
    text: &str,
    pos: Position,
    receiver: &Receiver,
    file: Option<&Path>,
    entries: &[IndexEntry],
) -> Option<String> {
    match receiver {
        Receiver::SelfLike => {
            let file = file?;
            entries
                .iter()
                .filter(|e| e.file == file && can_contain_methods(e.kind))
                .filter(|e| range_holds(&e.range, pos))
                .max_by_key(|e| (e.range.start.line, e.range.start.character))
                .map(|e| e.name.clone())
        }
        Receiver::Named(var) => {
            if entries
                .iter()
                .any(|e| e.name == *var && can_contain_methods(e.kind))
            {
                return Some(var.clone());
            }
            let escaped = regex::escape(var);
            let ty = r"[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*";
            let annotation = Regex::new(&format!(r"\b{escaped}\s*:\s*({ty})")).ok()?;
            let assignment =
                Regex::new(&format!(r"\b{escaped}\s*=\s*(?:new\s+)?({ty})\s*\(")).ok()?;
            declared_type(text, pos.line, &annotation)
                .or_else(|| declared_type(text, pos.line, &assignment))
        }
    }
}

fn declared_type(text: &str, line: u32, pattern: &Regex) -> Option<String> {
    let mut above: Option<(u32, String)> = None;
    let mut below: Option<(u32, String)> = None;
    for (i, l) in (0u32..).zip(text.lines()) {
        let Some(declared) = pattern.captures(l).and_then(|caps| caps.get(1)) else {
            continue;
        };
        let declared = declared.as_str();
        let ty = declared.rsplit('.').next().unwrap_or(declared).to_string();
        if i <= line {
            above = Some((i, ty));
        } else if below.is_none() {
            below = Some((i, ty));
        }
    }
    above.or(below).map(|(_, ty)| ty)
}

fn range_holds(range: &Range, pos: Position) -> bool {
    (range.start.line, range.start.character) <= (pos.line, pos.character)
        && (pos.line, pos.character) <= (range.end.line, range.end.character)
}

/// Every entry named exactly `name`, in a stable order, cut to
/// [`DEFINITION_LIMIT`].
///
/// Entries in `language` (the language of the buffer asking) come first; within
/// each group the order is file, then position. The limit applies after that
/// order, so it never drops a definition in `language` for one in another. A
/// `language` of `None` puts every entry in the second group, leaving plain
/// file-then-position order.
///
/// With a `container`, only the entries defined inside that class come back —
/// unless there are none, in which case the answer is the one `container: None`
/// would have given.
pub fn lookup<'a>(
    entries: &'a [IndexEntry],
    name: &str,
    language: Option<&str>,
    container: Option<&str>,
) -> Vec<&'a IndexEntry> {
    let mut found: Vec<&IndexEntry> = entries.iter().filter(|e| e.name == name).collect();
    if let Some(class) = container {
        let inside: Vec<&IndexEntry> = found
            .iter()
            .copied()
            .filter(|e| e.container.as_deref() == Some(class))
            .collect();
        if !inside.is_empty() {
            found = inside;
        }
    }
    found.sort_by_key(|e| {
        (
            Some(e.language.as_str()) != language,
            &e.file,
            e.range.start.line,
            e.range.start.character,
        )
    });
    found.truncate(DEFINITION_LIMIT);
    found
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tower_lsp::lsp_types::{Range, SymbolKind};

    use super::*;

    fn entry(name: &str, language: &str, file: &str, line: u32) -> IndexEntry {
        let range = Range {
            start: Position { line, character: 0 },
            end: Position { line, character: 1 },
        };
        IndexEntry {
            name: name.to_string(),
            language: language.to_string(),
            kind: SymbolKind::CLASS,
            file: PathBuf::from(file),
            range,
            name_range: range,
            container: None,
        }
    }

    fn files<'a>(found: &[&'a IndexEntry]) -> Vec<&'a str> {
        found.iter().map(|e| e.file.to_str().unwrap()).collect()
    }

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
        assert_eq!(
            identifier_at("obj.draw()", at(0, 5)).as_deref(),
            Some("draw")
        );
        assert_eq!(
            identifier_at("obj.draw()", at(0, 1)).as_deref(),
            Some("obj")
        );
    }

    // Pins that columns count characters, so a multibyte character earlier in the line does not shift the name.
    #[test]
    fn multibyte_characters_before_the_name_do_not_shift_it() {
        assert_eq!(
            identifier_at("é = make()", at(0, 5)).as_deref(),
            Some("make")
        );
    }

    // Pins S4: a name defined in two languages lists the buffer's language first, though its file sorts later.
    #[test]
    fn buffers_language_is_listed_before_other_languages() {
        let entries = [
            entry("Shared", "protobuf", "a/service.proto", 3),
            entry("Shared", "python", "z/service.py", 8),
        ];
        let found = lookup(&entries, "Shared", Some("python"), None);
        assert_eq!(files(&found), ["z/service.py", "a/service.proto"]);
    }

    // Pins that ordering inside one language is still file then position, so the answer stays stable across runs.
    #[test]
    fn within_one_language_order_is_file_then_position() {
        let entries = [
            entry("Shared", "python", "b.py", 1),
            entry("Shared", "python", "a.py", 9),
            entry("Shared", "python", "a.py", 2),
        ];
        let found = lookup(&entries, "Shared", Some("python"), None);
        let lines: Vec<u32> = found.iter().map(|e| e.range.start.line).collect();
        assert_eq!(files(&found), ["a.py", "a.py", "b.py"]);
        assert_eq!(lines, [2, 9, 1]);
    }

    // Pins that a buffer no language claims gets today's file-then-position order, not an empty or reshuffled answer.
    #[test]
    fn unknown_buffer_language_keeps_file_order() {
        let entries = [
            entry("Shared", "python", "z/service.py", 8),
            entry("Shared", "protobuf", "a/service.proto", 3),
        ];
        let found = lookup(&entries, "Shared", None, None);
        assert_eq!(files(&found), ["a/service.proto", "z/service.py"]);
    }

    // Pins that the cap is applied after the language ordering, so a foreign definition cannot crowd out the buffer's own.
    #[test]
    fn limit_keeps_the_buffers_language_over_earlier_files() {
        let mut entries: Vec<IndexEntry> = (0..DEFINITION_LIMIT)
            .map(|i| entry("Shared", "protobuf", &format!("a/{i:03}.proto"), 0))
            .collect();
        entries.push(entry("Shared", "python", "z.py", 0));
        let found = lookup(&entries, "Shared", Some("python"), None);
        assert_eq!(found.len(), DEFINITION_LIMIT);
        assert_eq!(files(&found)[0], "z.py");
    }

    fn scoped(
        name: &str,
        kind: SymbolKind,
        file: &str,
        lines: (u32, u32),
        container: Option<&str>,
    ) -> IndexEntry {
        let mut e = entry(name, "python", file, lines.0);
        e.kind = kind;
        e.range.end = Position {
            line: lines.1,
            character: 0,
        };
        e.container = container.map(str::to_string);
        e
    }

    // Pins that `self.method` is recognised as a call on the enclosing class, whatever the method is called.
    #[test]
    fn self_receiver_is_self_like() {
        assert_eq!(
            receiver_at("    self.get_state()", at(0, 12)),
            Some(Receiver::SelfLike)
        );
        assert_eq!(
            receiver_at("this.get()", at(0, 6)),
            Some(Receiver::SelfLike)
        );
    }

    // Pins that any other identifier before the dot is carried through as the receiver's name.
    #[test]
    fn dotted_receiver_is_named() {
        assert_eq!(
            receiver_at("bot.run()", at(0, 5)),
            Some(Receiver::Named("bot".to_string()))
        );
    }

    // Pins that `::` and `->` separate a receiver from a member just as `.` does, for the languages that spell it so.
    #[test]
    fn scope_and_arrow_separators_name_a_receiver() {
        assert_eq!(
            receiver_at("Foo::bar()", at(0, 6)),
            Some(Receiver::Named("Foo".to_string()))
        );
        assert_eq!(
            receiver_at("p->bar()", at(0, 4)),
            Some(Receiver::Named("p".to_string()))
        );
    }

    // Pins that a bare name has no receiver, so it is looked up by name alone.
    #[test]
    fn bare_name_has_no_receiver() {
        assert_eq!(receiver_at("run()", at(0, 1)), None);
    }

    // Pins that whitespace between the separator and the name ends the access, so `a. b` is not read as a receiver.
    #[test]
    fn space_before_the_dot_has_no_receiver() {
        assert_eq!(receiver_at("bot .run()", at(0, 6)), None);
    }

    // Pins that `self` resolves to the innermost class holding the cursor, not an outer one.
    #[test]
    fn self_resolves_to_the_innermost_enclosing_class() {
        let entries = [
            scoped("Outer", SymbolKind::CLASS, "a.py", (0, 20), None),
            scoped("Inner", SymbolKind::CLASS, "a.py", (5, 10), Some("Outer")),
            scoped("Elsewhere", SymbolKind::CLASS, "b.py", (0, 20), None),
        ];
        let got = receiver_type(
            "",
            at(7, 4),
            &Receiver::SelfLike,
            Some(Path::new("a.py")),
            &entries,
        );
        assert_eq!(got.as_deref(), Some("Inner"));
    }

    // Pins that an annotated variable resolves to its annotation, where the name is the only clue the file gives.
    #[test]
    fn annotated_variable_resolves_to_its_annotation() {
        let got = receiver_type(
            "g: Gadget = make()\ng.run()\n",
            at(1, 3),
            &Receiver::Named("g".to_string()),
            None,
            &[],
        );
        assert_eq!(got.as_deref(), Some("Gadget"));
    }

    // Pins that a dotted constructor call resolves to its last segment, since the index knows the class by that name alone.
    #[test]
    fn constructor_call_resolves_to_the_last_dotted_segment() {
        let got = receiver_type(
            "bot = marble_client.MarbleClient(host)\nbot.run()\n",
            at(1, 5),
            &Receiver::Named("bot".to_string()),
            None,
            &[],
        );
        assert_eq!(got.as_deref(), Some("MarbleClient"));
    }

    // Pins that a receiver spelled as a class the index defines is that class, which is how `Foo::bar` and `Foo.bar` resolve.
    #[test]
    fn receiver_naming_an_indexed_class_is_that_class() {
        let entries = [scoped("Foo", SymbolKind::CLASS, "a.py", (0, 5), None)];
        let got = receiver_type(
            "Foo.bar()",
            at(0, 5),
            &Receiver::Named("Foo".to_string()),
            None,
            &entries,
        );
        assert_eq!(got.as_deref(), Some("Foo"));
    }

    // Pins that a variable the file never types resolves to nothing, so the caller falls back to name alone.
    #[test]
    fn untyped_variable_resolves_to_nothing() {
        let got = receiver_type(
            "x.run()",
            at(0, 3),
            &Receiver::Named("x".to_string()),
            None,
            &[],
        );
        assert_eq!(got, None);
    }

    // Pins that a container narrows the answer to that class's method though another class defines the same name.
    #[test]
    fn container_keeps_only_that_classes_method() {
        let entries = [
            scoped(
                "get_state",
                SymbolKind::METHOD,
                "a.py",
                (2, 3),
                Some("Widget"),
            ),
            scoped(
                "get_state",
                SymbolKind::METHOD,
                "a.py",
                (8, 9),
                Some("Gadget"),
            ),
        ];
        let found = lookup(&entries, "get_state", Some("python"), Some("Gadget"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start.line, 8);
    }

    // Pins that a container nothing matches gives the by-name answer, so an inherited or unindexed method still resolves.
    #[test]
    fn container_matching_nothing_falls_back_to_every_definition() {
        let entries = [
            scoped(
                "get_state",
                SymbolKind::METHOD,
                "a.py",
                (2, 3),
                Some("Widget"),
            ),
            scoped(
                "get_state",
                SymbolKind::METHOD,
                "a.py",
                (8, 9),
                Some("Gadget"),
            ),
        ];
        let found = lookup(&entries, "get_state", Some("python"), Some("Base"));
        assert_eq!(found.len(), 2);
    }
}
