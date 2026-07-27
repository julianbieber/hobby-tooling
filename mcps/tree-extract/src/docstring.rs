//! Best-effort docstring extraction.
//!
//! For comment-prefix languages we walk source lines immediately above the
//! symbol's start row and pull contiguous comment blocks. For Python we look
//! at the first statement of the symbol's body and extract a triple-quoted
//! string literal if present.

use tree_sitter::Node;

pub fn extract_docstring(source: &str, language: &str, def_node: Node) -> Option<String> {
    match language {
        "rust" => extract_above_prefix(source, def_node, &["///", "//!"]),
        "scala" | "java" | "javascript" | "jsx" | "typescript" | "tsx" | "kotlin" | "swift" => {
            extract_above_block(source, def_node)
        }
        "go" => extract_above_prefix(source, def_node, &["//"]),
        "python" => extract_python_docstring(source, def_node),
        _ => None,
    }
}

fn extract_above_prefix(source: &str, def_node: Node, prefixes: &[&str]) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let row = def_node.start_position().row;
    if row == 0 {
        return None;
    }
    let mut collected: Vec<String> = Vec::new();
    let mut i = row as i64 - 1;
    while i >= 0 {
        let line = lines.get(i as usize)?.trim_start();
        let mut matched = false;
        for p in prefixes {
            if let Some(rest) = line.strip_prefix(p) {
                collected.push(rest.trim().to_string());
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
        i -= 1;
    }
    if collected.is_empty() {
        None
    } else {
        collected.reverse();
        Some(collected.join(" "))
    }
}

fn extract_above_block(source: &str, def_node: Node) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let row = def_node.start_position().row;
    if row == 0 {
        return None;
    }
    // Walk upward over blank lines + annotation lines to find the line that
    // ends with `*/`.
    let mut end_row: Option<usize> = None;
    let mut i = row as i64 - 1;
    while i >= 0 {
        let trimmed = lines.get(i as usize)?.trim();
        if trimmed.is_empty() || trimmed.starts_with('@') {
            i -= 1;
            continue;
        }
        if trimmed.ends_with("*/") {
            end_row = Some(i as usize);
        }
        break;
    }
    let end_row = end_row?;
    // Walk up from end_row to find the matching `/**` (must start the block).
    let mut start_row: Option<usize> = None;
    let mut j = end_row as i64;
    while j >= 0 {
        let trimmed = lines.get(j as usize)?.trim_start();
        if trimmed.starts_with("/**") {
            start_row = Some(j as usize);
            break;
        }
        j -= 1;
    }
    let start_row = start_row?;
    let mut buf = String::new();
    for line in &lines[start_row..=end_row] {
        let cleaned = line
            .trim()
            .trim_start_matches("/**")
            .trim_start_matches("/*")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();
        if !cleaned.is_empty() {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(cleaned);
        }
    }
    if buf.is_empty() { None } else { Some(buf) }
}

fn extract_python_docstring(source: &str, def_node: Node) -> Option<String> {
    let mut child_cursor = def_node.walk();
    for child in def_node.named_children(&mut child_cursor) {
        if child.kind() == "block" || child.kind() == "function_definition_body" {
            let mut body_cursor = child.walk();
            // Only the first statement counts as a docstring.
            if let Some(stmt) = child.named_children(&mut body_cursor).next()
                && stmt.kind() == "expression_statement"
            {
                let mut s = stmt.walk();
                for expr in stmt.named_children(&mut s) {
                    if expr.kind() == "string" {
                        let raw = expr.utf8_text(source.as_bytes()).ok()?;
                        return Some(strip_python_quotes(raw));
                    }
                }
            }
            break;
        }
    }
    None
}

fn strip_python_quotes(s: &str) -> String {
    let trimmed = s.trim_start_matches('r').trim_start_matches('b');
    let core = trimmed
        .strip_prefix("\"\"\"")
        .and_then(|c| c.strip_suffix("\"\"\""))
        .or_else(|| {
            trimmed
                .strip_prefix("'''")
                .and_then(|c| c.strip_suffix("'''"))
        })
        .or_else(|| trimmed.strip_prefix('"').and_then(|c| c.strip_suffix('"')))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|c| c.strip_suffix('\''))
        })
        .unwrap_or(trimmed);
    core.trim().to_string()
}
