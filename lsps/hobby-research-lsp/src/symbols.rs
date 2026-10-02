//! Turning `hresearch symbols` output into the workspace symbols an editor lists.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;
use tower_lsp::lsp_types::{Location, Position, Range, SymbolInformation, SymbolKind, Url};

#[derive(Deserialize)]
struct Entry {
    name: String,
    kind: String,
    path: String,
    line: u32,
    container: String,
}

/// The research-card symbols matching `query`, as `hresearch symbols` lists
/// them when run from `root`. Empty when `hresearch` is missing from `PATH`,
/// exits non-zero or prints anything but its JSON array; never an error.
pub fn query(root: Option<&Path>, query: &str) -> Vec<SymbolInformation> {
    let mut command = Command::new("hresearch");
    command
        .args(["symbols", query])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(root) = root {
        command.current_dir(root);
    }
    match command.output() {
        Ok(out) if out.status.success() => parse(&String::from_utf8_lossy(&out.stdout)),
        _ => Vec::new(),
    }
}

/// The symbols in one `hresearch symbols` JSON array: each located at the
/// start of its line in the card file, with the container carrying the tag.
/// Empty on anything that is not that array; an entry whose path is not
/// absolute is dropped.
pub fn parse(json: &str) -> Vec<SymbolInformation> {
    let entries: Vec<Entry> = serde_json::from_str(json).unwrap_or_default();
    entries.into_iter().filter_map(symbol).collect()
}

#[allow(deprecated)]
fn symbol(entry: Entry) -> Option<SymbolInformation> {
    let position = Position::new(entry.line, 0);
    Some(SymbolInformation {
        name: entry.name,
        kind: kind(&entry.kind),
        tags: None,
        deprecated: None,
        location: Location::new(
            Url::from_file_path(&entry.path).ok()?,
            Range::new(position, position),
        ),
        container_name: Some(entry.container),
    })
}

fn kind(name: &str) -> SymbolKind {
    match name {
        "file" => SymbolKind::FILE,
        "struct" => SymbolKind::STRUCT,
        "enum" => SymbolKind::ENUM,
        "interface" => SymbolKind::INTERFACE,
        "function" => SymbolKind::FUNCTION,
        "package" => SymbolKind::PACKAGE,
        _ => SymbolKind::CONSTANT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(kind: &str, line: u32) -> String {
        format!(
            r#"[{{"name":"X","kind":"{kind}","path":"/tmp/card.hresearch","line":{line},"container":"p/t bevy@0.19.1 current"}}]"#
        )
    }

    // Each kind string `hresearch` emits must reach the LSP kind the picker shows an icon for.
    #[test]
    fn kinds_map() {
        for (name, want) in [
            ("file", SymbolKind::FILE),
            ("struct", SymbolKind::STRUCT),
            ("enum", SymbolKind::ENUM),
            ("interface", SymbolKind::INTERFACE),
            ("function", SymbolKind::FUNCTION),
            ("constant", SymbolKind::CONSTANT),
            ("package", SymbolKind::PACKAGE),
            ("unheard-of", SymbolKind::CONSTANT),
        ] {
            assert_eq!(parse(&one(name, 0))[0].kind, want, "{name}");
        }
    }

    // Enter jumps to the line the identifier is on, so the line must land in the range start.
    #[test]
    fn line_lands_in_range() {
        let s = &parse(&one("struct", 7))[0];
        assert_eq!(s.location.range.start, Position::new(7, 0));
        assert_eq!(s.location.uri.path(), "/tmp/card.hresearch");
    }

    // The picker shows the container as the only place the version tag can be read.
    #[test]
    fn container_is_kept() {
        let s = &parse(&one("struct", 0))[0];
        assert_eq!(s.container_name.as_deref(), Some("p/t bevy@0.19.1 current"));
    }

    // A missing or broken `hresearch` must not surface as a protocol error on every keystroke.
    #[test]
    fn garbage_is_empty() {
        assert!(parse("not json").is_empty());
        assert!(parse("").is_empty());
        assert!(parse("[]").is_empty());
    }
}
