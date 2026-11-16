//! Finding `claude:` comments in a file's lines and turning one into a request
//! with the code around it.

use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::Range;

/// A claude instruction found in a comment.
#[derive(Debug, Clone)]
pub struct ClaudeInstruction {
    /// The instruction text (after `claude:` prefix).
    pub text: String,
    /// Line number where the comment was found.
    pub line: u32,
}

/// A structured request to send to Claude Code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeRequest {
    /// Absolute file path.
    pub file: String,
    /// The instruction from the comment.
    pub instruction: String,
    /// Start line of the relevant code range (1-indexed for display).
    pub start_line: u32,
    /// End line of the relevant code range (1-indexed).
    pub end_line: u32,
    /// The code context (lines around the selection).
    pub context: String,
}

/// Return the claude comment prefixes that are syntactically valid for a file's language.
///
/// Keeps detection strict: a `# claude:` in a Rust file should not match, since it isn't a
/// valid Rust comment.
pub fn claude_prefixes_for_path(path: &str) -> &'static [&'static str] {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "py" | "pyi" | "rb" | "sh" | "bash" | "zsh" | "nu" | "toml" | "yaml" | "yml" | "nix"
        | "r" | "pl" | "pm" => &["# claude:"],
        "sql" | "hs" | "lua" => &["-- claude:"],
        _ => &["// claude:", "/* claude:"],
    }
}

/// The primary prefix to use when *inserting* a new claude comment for a file.
pub fn comment_prefix_for_file(path: &str) -> &'static str {
    match claude_prefixes_for_path(path) {
        ["# claude:", ..] => "# claude: ",
        ["-- claude:", ..] => "-- claude: ",
        _ => "// claude: ",
    }
}

/// The claude instructions on the lines of `range`.
///
/// Checks the selection itself plus a few lines above it, since the typical
/// pattern is writing a comment above the code you want edited.
pub fn find_claude_comments(lines: &[&str], range: &Range, path: &str) -> Vec<ClaudeInstruction> {
    let prefixes = claude_prefixes_for_path(path);
    numbered(lines, range.start.line.saturating_sub(3), range.end.line)
        .filter_map(|(line, raw)| {
            extract_claude_comment(raw.trim(), prefixes).map(|text| ClaudeInstruction {
                text: text.to_string(),
                line,
            })
        })
        .collect()
}

fn numbered<'a>(
    lines: &'a [&'a str],
    first: u32,
    last: u32,
) -> impl Iterator<Item = (u32, &'a str)> {
    (0u32..)
        .zip(lines.iter().copied())
        .skip_while(move |(i, _)| *i < first)
        .take_while(move |(i, _)| *i <= last)
}

/// Length in bytes of line `line` of `lines`, 0 past the end.
pub fn line_len(lines: &[&str], line: u32) -> u32 {
    usize::try_from(line)
        .ok()
        .and_then(|i| lines.get(i))
        .map_or(0, |l| u32::try_from(l.len()).unwrap_or(u32::MAX))
}

/// Extract the instruction text from a claude comment, if the line matches one of `prefixes`.
fn extract_claude_comment<'a>(line: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for prefix in prefixes {
        if let Some(rest) = line_after_prefix_ci(line, prefix) {
            let rest = rest.trim_end_matches("*/").trim();
            if !rest.is_empty() {
                return Some(rest);
            }
        }
    }
    None
}

/// Case-insensitive prefix match, returns the remainder after the prefix.
fn line_after_prefix_ci<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, rest) = line.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then_some(rest)
}

/// Build a structured request from the detected instruction and code context.
pub fn build_request(
    file_path: &str,
    lines: &[&str],
    range: &Range,
    instruction: &ClaudeInstruction,
) -> ClaudeRequest {
    let context_lines: Vec<String> = numbered(
        lines,
        range.start.line.saturating_sub(5),
        range.end.line.saturating_add(5),
    )
    .map(|(i, line)| format!("{:>4} | {}", u64::from(i) + 1, line))
    .collect();

    ClaudeRequest {
        file: file_path.to_string(),
        instruction: instruction.text.clone(),
        start_line: range.start.line + 1,
        end_line: range.end.line + 1,
        context: context_lines.join("\n"),
    }
}

/// Find all claude-comment lines in the file, scoped to the language's valid prefixes.
/// Returns `(line_index_0_based, instruction_text)` pairs.
pub fn find_all_claude_comments(lines: &[&str], path: &str) -> Vec<(u32, String)> {
    let prefixes = claude_prefixes_for_path(path);
    (0u32..)
        .zip(lines)
        .filter_map(|(i, line)| {
            extract_claude_comment(line.trim(), prefixes).map(|t| (i, t.to_string()))
        })
        .collect()
}

/// Find all claude-comment line indices.
pub fn find_all_claude_comment_lines(lines: &[&str], path: &str) -> Vec<u32> {
    let prefixes = claude_prefixes_for_path(path);
    (0u32..)
        .zip(lines)
        .filter_map(|(i, line)| extract_claude_comment(line.trim(), prefixes).map(|_| i))
        .collect()
}

/// Truncate a string to `max_len` characters, the last three of them "..." if
/// truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{kept}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn range(start_line: u32, end_line: u32) -> Range {
        Range {
            start: Position {
                line: start_line,
                character: 0,
            },
            end: Position {
                line: end_line,
                character: 0,
            },
        }
    }

    #[test]
    fn finds_rust_style_claude_comment() {
        let lines = vec!["// claude: refactor this", "fn foo() {}"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "refactor this");
        assert_eq!(results[0].line, 0);
    }

    #[test]
    fn finds_hash_style_claude_comment() {
        let lines = vec!["# claude: fix the bug", "def foo():"];
        let results = find_claude_comments(&lines, &range(0, 1), "app.py");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "fix the bug");
    }

    #[test]
    fn finds_sql_style_claude_comment() {
        let lines = vec!["-- claude: optimize query", "SELECT * FROM t;"];
        let results = find_claude_comments(&lines, &range(0, 1), "query.sql");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "optimize query");
    }

    #[test]
    fn finds_block_comment_style() {
        let lines = vec!["/* claude: add error handling */", "let x = 1;"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "add error handling");
    }

    #[test]
    fn case_insensitive_prefix() {
        let lines = vec!["// Claude: do something", "code"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "do something");
    }

    #[test]
    fn no_match_on_plain_comments() {
        let lines = vec!["// just a normal comment", "fn bar() {}"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert!(results.is_empty());
    }

    #[test]
    fn scans_lines_above_selection() {
        let lines = vec![
            "// claude: fix this",
            "// some other comment",
            "// more stuff",
            "fn target() {}",
        ];
        // Selection is on line 3; the comment on line 0 is within the 3-line look-back
        let results = find_claude_comments(&lines, &range(3, 3), "src/main.rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "fix this");
        assert_eq!(results[0].line, 0);
    }

    #[test]
    fn does_not_scan_too_far_above() {
        let lines = vec!["// claude: too far away", "", "", "", "", "fn target() {}"];
        // Selection on line 5; look-back is 3 lines (lines 2..=5), so line 0 is out of range
        let results = find_claude_comments(&lines, &range(5, 5), "src/main.rs");
        assert!(results.is_empty());
    }

    #[test]
    fn multiple_instructions_in_range() {
        let lines = vec![
            "// claude: first instruction",
            "// claude: second instruction",
            "fn foo() {}",
        ];
        let results = find_claude_comments(&lines, &range(0, 2), "src/main.rs");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].text, "first instruction");
        assert_eq!(results[1].text, "second instruction");
    }

    #[test]
    fn ignores_empty_instruction() {
        let lines = vec!["// claude:", "fn foo() {}"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert!(results.is_empty());
    }

    #[test]
    fn handles_indented_comments() {
        let lines = vec!["    // claude: handle indent", "    let x = 1;"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "handle indent");
    }

    #[test]
    fn rust_file_ignores_hash_prefix() {
        // `# claude:` is not a Rust comment and must not match in .rs files
        let lines = vec!["# claude: wrong syntax", "fn foo() {}"];
        let results = find_claude_comments(&lines, &range(0, 1), "src/main.rs");
        assert!(results.is_empty());
    }

    #[test]
    fn python_file_ignores_slash_prefix() {
        let lines = vec!["// claude: wrong syntax", "def foo():"];
        let results = find_claude_comments(&lines, &range(0, 1), "app.py");
        assert!(results.is_empty());
    }

    #[test]
    fn build_request_basic() {
        let lines = vec![
            "fn main() {",
            "    // claude: refactor this",
            "    let x = 1;",
            "    let y = 2;",
            "}",
        ];
        let instr = ClaudeInstruction {
            text: "refactor this".to_string(),
            line: 1,
        };
        let req = build_request("/src/main.rs", &lines, &range(1, 3), &instr);

        assert_eq!(req.file, "/src/main.rs");
        assert_eq!(req.instruction, "refactor this");
        // range lines are 0-indexed; output is 1-indexed
        assert_eq!(req.start_line, 2);
        assert_eq!(req.end_line, 4);
        // Context should contain all 5 lines (no clamping needed)
        assert!(req.context.contains("fn main() {"));
        assert!(req.context.contains("let x = 1;"));
        assert!(req.context.contains("}"));
    }

    #[test]
    fn build_request_context_has_line_numbers() {
        let lines = vec!["line0", "line1", "line2"];
        let instr = ClaudeInstruction {
            text: "do stuff".to_string(),
            line: 1,
        };
        let req = build_request("/test.rs", &lines, &range(1, 1), &instr);

        // Each context line should be formatted as "   N | content"
        for ctx_line in req.context.lines() {
            assert!(ctx_line.contains(" | "), "expected '| ' in: {ctx_line}");
        }
        // Line numbers are 1-indexed in context display
        assert!(req.context.contains("   1 | line0"));
        assert!(req.context.contains("   2 | line1"));
        assert!(req.context.contains("   3 | line2"));
    }

    #[test]
    fn build_request_clamps_context_to_file_bounds() {
        let lines = vec!["only line"];
        let instr = ClaudeInstruction {
            text: "test".to_string(),
            line: 0,
        };
        let req = build_request("/f.rs", &lines, &range(0, 0), &instr);

        assert_eq!(req.start_line, 1);
        assert_eq!(req.end_line, 1);
        assert_eq!(req.context.lines().count(), 1);
        assert!(req.context.contains("only line"));
    }

    #[test]
    fn build_request_context_window_extends_around_selection() {
        // 15 lines total; selection at lines 7-8 (0-indexed)
        let lines: Vec<&str> = (0..15)
            .map(|i| match i {
                0 => "line_00",
                1 => "line_01",
                2 => "line_02",
                3 => "line_03",
                4 => "line_04",
                5 => "line_05",
                6 => "line_06",
                7 => "line_07",
                8 => "line_08",
                9 => "line_09",
                10 => "line_10",
                11 => "line_11",
                12 => "line_12",
                13 => "line_13",
                _ => "line_14",
            })
            .collect();
        let instr = ClaudeInstruction {
            text: "edit".to_string(),
            line: 7,
        };
        let req = build_request("/f.rs", &lines, &range(7, 8), &instr);

        // context_start = 7 - 5 = 2, context_end = min(8 + 6, 15) = 14
        // So context covers lines 2..14 (indices 2 through 13)
        assert!(req.context.contains("line_02"));
        assert!(req.context.contains("line_13"));
        assert!(!req.context.contains("line_01"));
        assert!(!req.context.contains("line_14"));
    }

    #[test]
    fn find_all_returns_line_and_text() {
        let lines = vec![
            "fn main() {",
            "    // claude: first",
            "    let x = 1;",
            "    // claude: second",
            "}",
        ];
        let results = find_all_claude_comments(&lines, "src/main.rs");
        assert_eq!(
            results,
            vec![(1, "first".to_string()), (3, "second".to_string())]
        );
    }

    #[test]
    fn prefix_selection_by_extension() {
        assert_eq!(claude_prefixes_for_path("foo.py"), &["# claude:"]);
        assert_eq!(claude_prefixes_for_path("foo.sql"), &["-- claude:"]);
        assert_eq!(
            claude_prefixes_for_path("foo.rs"),
            &["// claude:", "/* claude:"]
        );
        // unknown extension falls back to slash-style
        assert_eq!(
            claude_prefixes_for_path("README"),
            &["// claude:", "/* claude:"]
        );
    }

    #[test]
    fn insertion_prefix_matches_language() {
        assert_eq!(comment_prefix_for_file("app.py"), "# claude: ");
        assert_eq!(comment_prefix_for_file("query.sql"), "-- claude: ");
        assert_eq!(comment_prefix_for_file("src/main.rs"), "// claude: ");
    }
}
