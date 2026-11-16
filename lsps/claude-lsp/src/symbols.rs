//! Listing `claude:` comments as document and workspace symbols.

use std::path::Path;

use ignore::WalkBuilder;
use tower_lsp::lsp_types::{
    DocumentSymbol, Location, Position, Range, SymbolInformation, SymbolKind, Url,
};

use crate::actions;

/// Hard cap on how many workspace-symbol matches we return per query,
/// so a huge repo cannot produce an unbounded response.
const WORKSPACE_SYMBOL_LIMIT: usize = 500;

/// Build document symbols for every claude comment in a file.
pub fn document_symbols(lines: &[&str], path: &str) -> Vec<DocumentSymbol> {
    actions::find_all_claude_comments(lines, path)
        .into_iter()
        .map(|(line_idx, text)| {
            let line_len = actions::line_len(lines, line_idx);
            let range = Range {
                start: Position {
                    line: line_idx,
                    character: 0,
                },
                end: Position {
                    line: line_idx,
                    character: line_len,
                },
            };
            #[expect(
                deprecated,
                reason = "the protocol's deprecated field still has to be set"
            )]
            let symbol = DocumentSymbol {
                name: text,
                detail: Some("claude".to_string()),
                kind: SymbolKind::STRING,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: None,
            };
            symbol
        })
        .collect()
}

/// Walk `root` and return claude comments across all files, optionally filtered by `query`.
/// Respects `.gitignore` (via the `ignore` crate). Caps results at `WORKSPACE_SYMBOL_LIMIT`.
pub fn workspace_symbols(root: &Path, query: &str) -> Vec<SymbolInformation> {
    let query_lc = query.to_ascii_lowercase();
    let mut out = Vec::new();

    for entry in WalkBuilder::new(root).build() {
        if out.len() >= WORKSPACE_SYMBOL_LIMIT {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(%err, "workspace symbol walk error");
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let path_str = path.to_string_lossy();
        let lines: Vec<&str> = content.lines().collect();

        for (line_idx, text) in actions::find_all_claude_comments(&lines, &path_str) {
            if !query_lc.is_empty() && !text.to_ascii_lowercase().contains(&query_lc) {
                continue;
            }
            let Ok(uri) = Url::from_file_path(path) else {
                continue;
            };
            let line_len = actions::line_len(&lines, line_idx);
            #[expect(
                deprecated,
                reason = "the protocol's deprecated field still has to be set"
            )]
            let symbol = SymbolInformation {
                name: text,
                kind: SymbolKind::STRING,
                tags: None,
                deprecated: None,
                location: Location {
                    uri,
                    range: Range {
                        start: Position {
                            line: line_idx,
                            character: 0,
                        },
                        end: Position {
                            line: line_idx,
                            character: line_len,
                        },
                    },
                },
                container_name: None,
            };
            out.push(symbol);
            if out.len() >= WORKSPACE_SYMBOL_LIMIT {
                break;
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn document_symbols_lists_each_comment() {
        let lines = vec![
            "fn main() {",
            "    // claude: first",
            "    let x = 1;",
            "    // claude: second thing",
            "}",
        ];
        let syms = document_symbols(&lines, "src/main.rs");
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "first");
        assert_eq!(syms[0].range.start.line, 1);
        assert_eq!(syms[1].name, "second thing");
        assert_eq!(syms[1].range.start.line, 3);
    }

    #[test]
    fn document_symbols_respects_language() {
        // In a Python file, only `# claude:` should match
        let lines = vec!["# claude: py-one", "// claude: not-py"];
        let syms = document_symbols(&lines, "foo.py");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "py-one");
    }

    #[test]
    fn workspace_symbols_walks_and_filters() {
        let tmp = std::env::temp_dir().join(format!("claude-lsp-ws-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let rs_path = tmp.join("a.rs");
        let py_path = tmp.join("b.py");
        let mut rs = fs::File::create(&rs_path).unwrap();
        writeln!(rs, "fn foo() {{}}").unwrap();
        writeln!(rs, "// claude: refactor the foo").unwrap();
        let mut py = fs::File::create(&py_path).unwrap();
        writeln!(py, "# claude: add tests").unwrap();
        writeln!(py, "def bar(): pass").unwrap();

        // Empty query returns everything
        let all = workspace_symbols(&tmp, "");
        assert_eq!(all.len(), 2);

        // Filter by substring
        let filtered = workspace_symbols(&tmp, "tests");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "add tests");

        // Case-insensitive
        let ci = workspace_symbols(&tmp, "REFACTOR");
        assert_eq!(ci.len(), 1);
        assert_eq!(ci[0].name, "refactor the foo");

        fs::remove_dir_all(&tmp).unwrap();
    }
}
