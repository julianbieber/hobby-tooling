//! A minimal YAML-frontmatter reader for skill/command files.
//!
//! Only the `name:` and `description:` keys are extracted, and only single-line
//! scalar values are supported (folded/multiline `>-` and `|` blocks are not
//! parsed — the first line is taken as-is). This avoids a heavyweight YAML
//! dependency for the two fields the LSP needs.

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Parse the leading `---`…`---` frontmatter block of `md`. Tolerates CRLF and
/// a missing closing fence (in which case the rest of the file is scanned).
pub fn parse(md: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();

    // Normalise line endings for scanning without allocating per line.
    let trimmed = md.strip_prefix('\u{feff}').unwrap_or(md);
    let mut lines = trimmed.lines();

    // The file must open with a `---` fence (allowing trailing whitespace).
    match lines.next() {
        Some(first) if first.trim_end() == "---" => {}
        _ => return fm,
    }

    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.trim_end() == "---" {
            break;
        }
        if let Some(rest) = strip_key(line, "name")
            && fm.name.is_none()
        {
            fm.name = clean_scalar(rest);
        } else if let Some(rest) = strip_key(line, "description")
            && fm.description.is_none()
        {
            fm.description = clean_scalar(rest);
        }
    }

    fm
}

/// If `line` is `<key>:<value>` (no leading indentation), return the raw value.
fn strip_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    // Reject indented keys — those are nested under another mapping, not
    // top-level frontmatter fields.
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = line.strip_prefix(key)?;
    rest.strip_prefix(':')
}

/// Trim a scalar value and drop a single layer of matching quotes. A folded /
/// literal block indicator (`>`, `|`, with optional chomping) yields no
/// single-line value.
fn clean_scalar(raw: &str) -> Option<String> {
    let v = raw.trim();
    if v.is_empty() {
        return None;
    }
    if v.starts_with('>') || v.starts_with('|') {
        // Multiline scalar; the value continues on following indented lines,
        // which we do not stitch together. Treat as absent.
        return None;
    }
    let unquoted = strip_quotes(v);
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

fn strip_quotes(v: &str) -> &str {
    let bytes = v.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &v[1..v.len() - 1];
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_name_and_description() {
        let md = "---\nname: principal-review\ndescription: Do a review\n---\nbody\n";
        let fm = parse(md);
        assert_eq!(fm.name.as_deref(), Some("principal-review"));
        assert_eq!(fm.description.as_deref(), Some("Do a review"));
    }

    #[test]
    fn description_only_command_style() {
        let md = "---\ndescription: Analyze one unit of code\nallowed-tools: Read, Grep\n---\n";
        let fm = parse(md);
        assert_eq!(fm.name, None);
        assert_eq!(fm.description.as_deref(), Some("Analyze one unit of code"));
    }

    #[test]
    fn no_frontmatter_is_empty() {
        let fm = parse("# Just a heading\nsome text\n");
        assert_eq!(fm, Frontmatter::default());
    }

    #[test]
    fn crlf_is_tolerated() {
        let md = "---\r\nname: foo\r\ndescription: bar\r\n---\r\n";
        let fm = parse(md);
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description.as_deref(), Some("bar"));
    }

    #[test]
    fn missing_closing_fence_still_reads_fields() {
        let md = "---\nname: foo\ndescription: bar\n";
        let fm = parse(md);
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description.as_deref(), Some("bar"));
    }

    #[test]
    fn quoted_values_are_unwrapped() {
        let md = "---\nname: \"foo\"\ndescription: 'bar baz'\n---\n";
        let fm = parse(md);
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description.as_deref(), Some("bar baz"));
    }

    #[test]
    fn folded_scalar_description_is_absent() {
        // A `>-` folded block's value is on following indented lines; we don't
        // stitch it, so description is treated as absent rather than ">-".
        let md = "---\nname: foo\ndescription: >-\n  long text here\n  more text\n---\n";
        let fm = parse(md);
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description, None);
    }

    #[test]
    fn indented_keys_are_ignored() {
        let md = "---\nmeta:\n  name: nested\nname: top\n---\n";
        let fm = parse(md);
        assert_eq!(fm.name.as_deref(), Some("top"));
    }
}
