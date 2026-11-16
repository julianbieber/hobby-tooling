//! Finding the link under a cursor in a research file.

const SCHEMES: [&str; 2] = ["https://", "http://"];

/// The URL `gd` at `line`/`character` should open: the link under the cursor,
/// or, on a `[k]` ref, the URL of source `[k]`. `None` on anything else, and on
/// a ref the file lists no source for.
pub fn target_at(text: &str, line: u32, character: u32) -> Option<String> {
    url_at(text, line, character).or_else(|| {
        let line = nth_line(text, line)?;
        let k = ref_at(line, byte_offset(line, character))?;
        source_url(text, k)
    })
}

fn nth_line(text: &str, line: u32) -> Option<&str> {
    text.lines().nth(usize::try_from(line).ok()?)
}

fn url_at(text: &str, line: u32, character: u32) -> Option<String> {
    let line = nth_line(text, line)?;
    let cursor = byte_offset(line, character);
    SCHEMES
        .iter()
        .flat_map(|scheme| line.match_indices(scheme))
        .filter_map(|(start, _)| Some((start, url_prefix(line.get(start..)?))))
        .find(|(start, url)| *start <= cursor && cursor < start + url.len())
        .map(|(_, url)| url.to_string())
}

fn ref_at(line: &str, cursor: usize) -> Option<&str> {
    line.match_indices('[').find_map(|(open, _)| {
        let (k, _) = line.get(open + 1..)?.split_once(']')?;
        let close = open + 1 + k.len();
        let is_ref = !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit());
        (is_ref && open <= cursor && cursor <= close).then_some(k)
    })
}

fn source_url(text: &str, k: &str) -> Option<String> {
    let label = format!("[{k}]");
    text.lines().find_map(|line| {
        let rest = line.trim_start_matches(['│', ' ']);
        let rest = rest.strip_prefix("source").unwrap_or(rest).trim_start();
        let rest = rest.strip_prefix(&label)?;
        let url = rest.trim_start();
        let is_url = rest.starts_with(' ') && SCHEMES.iter().any(|s| url.starts_with(s));
        is_url.then(|| url_prefix(url).to_string())
    })
}

fn url_prefix(s: &str) -> &str {
    s.split(|c: char| c.is_whitespace() || c == '│')
        .next()
        .unwrap_or(s)
}

fn byte_offset(line: &str, character: u32) -> usize {
    let character = usize::try_from(character).unwrap_or(usize::MAX);
    let mut units = 0;
    for (i, c) in line.char_indices() {
        if units >= character {
            return i;
        }
        units += c.len_utf16();
    }
    line.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "\
│ source   [1] https://docs.rs/bevy/latest/bevy/app/struct.App.html     │
│          ↳ https://github.com/b/e/blob/main/x.rs#L42-L49 [2]   │
│ how      1. Change main's signature [2]                                │";

    fn col(line: &str, needle: &str) -> u32 {
        let byte = line.find(needle).unwrap();
        u32::try_from(line.get(..byte).unwrap().encode_utf16().count()).unwrap()
    }

    // The cursor anywhere inside a source line's URL yields the whole URL,
    // stopping at the padding before the frame.
    #[test]
    fn cursor_inside_url_yields_whole_url() {
        let line = CARD.lines().next().unwrap();
        let c = col(line, "latest");
        assert_eq!(
            url_at(CARD, 0, c).as_deref(),
            Some("https://docs.rs/bevy/latest/bevy/app/struct.App.html")
        );
    }

    // Columns are UTF-16: the `│` and `↳` before a snippet link must not shift
    // the cursor off the URL, and a `#L` anchor stays part of it.
    #[test]
    fn column_after_multibyte_frame_resolves() {
        let line = CARD.lines().nth(1).unwrap();
        assert_eq!(
            url_at(CARD, 1, col(line, "https")).as_deref(),
            Some("https://github.com/b/e/blob/main/x.rs#L42-L49")
        );
    }

    // A cursor off any link, or on the ref after one, opens nothing.
    #[test]
    fn cursor_off_url_yields_none() {
        let line0 = CARD.lines().next().unwrap();
        assert_eq!(url_at(CARD, 0, col(line0, "[1]")), None);
        assert_eq!(url_at(CARD, 2, 12), None);
        let line1 = CARD.lines().nth(1).unwrap();
        assert_eq!(url_at(CARD, 1, col(line1, "[2]")), None);
    }

    // On a card, `[k]` anywhere in a slot resolves to the `source` slot's
    // entry `[k]`, whether it is the slot's first line or a continuation.
    #[test]
    fn card_ref_opens_its_source() {
        let card = format!("{CARD}\n│          [2] https://github.com/b/e/blob/main/x.rs      │");
        let how = card.lines().nth(2).unwrap();
        assert_eq!(
            target_at(&card, 2, col(how, "[2]") + 1).as_deref(),
            Some("https://github.com/b/e/blob/main/x.rs")
        );
        let source = card.lines().next().unwrap();
        assert_eq!(
            target_at(&card, 0, col(source, "[1]")).as_deref(),
            Some("https://docs.rs/bevy/latest/bevy/app/struct.App.html")
        );
    }

    // A matrix or figure lists its sources flush left after the body; a ref in
    // the body resolves there, and one the list omits opens nothing.
    #[test]
    fn sheet_ref_opens_its_source_or_nothing() {
        let sheet = "\
question: which techniques exist

looks like | pooling water [6] | smoke [2] | [unsourced]

[6] https://github.com/luciopaiva/water";
        let body = sheet.lines().nth(2).unwrap();
        assert_eq!(
            target_at(sheet, 2, col(body, "6]")).as_deref(),
            Some("https://github.com/luciopaiva/water")
        );
        assert_eq!(target_at(sheet, 2, col(body, "[2]")), None);
        assert_eq!(target_at(sheet, 2, col(body, "[unsourced]") + 1), None);
    }

    // A `[k]` that is only mentioned, not listed with a URL, is no source.
    #[test]
    fn ref_without_url_is_not_a_source() {
        let text = "│ verdict  see [3]  │\n│          [3] not a link │";
        let line = text.lines().next().unwrap();
        assert_eq!(target_at(text, 0, col(line, "[3]")), None);
    }

    // A URL with no frame after it (a matrix or figure line) runs to line end.
    #[test]
    fn url_at_end_of_line_runs_to_end() {
        let text = "  [3] http://example.com/a";
        assert_eq!(url_at(text, 0, 10).as_deref(), Some("http://example.com/a"));
    }
}
