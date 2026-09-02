//! Cursor-position logic for the language server, as pure functions.

use struct_parser::ast::Spec;
use struct_parser::span::Span;

/// What may be completed at a point, decided from the line alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContext {
    /// After `check: ` — one of the metrics the checker counts.
    Metric,
    /// At the start of a line inside a standard — a verification key.
    VerificationKey,
    None,
}

/// Every metric spelling a `check:` line accepts, matching grammar.md §4.
pub const METRICS: &[&str] = &[
    "import cycles",
    "files over ${1:900} lines",
    "modules importing more than ${1:6}",
    "multi-writer types",
    "unfiltered queries",
    "allocating systems",
];

/// The keys a standard's body may carry.
pub const VERIFICATION_KEYS: &[&str] = &["check: ", "budget: ", "review: ", "audited: "];

/// The standard whose section `offset` falls in, with the span of its heading.
pub fn standard_at(text: &str, offset: usize, spec: &Spec) -> Option<(String, Span)> {
    let _ = text;
    spec.standards
        .iter()
        .rfind(|s| s.span.start <= offset)
        .map(|s| (s.name.value.clone(), s.name.span))
}

/// What can be completed at `offset`, from the shape of the line up to it.
pub fn completion_context(text: &str, offset: usize) -> CompletionContext {
    let line = line_containing(text, offset);
    let before = &text[line.start..offset];
    let trimmed = before.trim_start();

    if trimmed.starts_with("check:") {
        return CompletionContext::Metric;
    }
    if !trimmed.contains(char::is_whitespace) && !trimmed.starts_with('#') {
        return CompletionContext::VerificationKey;
    }
    CompletionContext::None
}

fn line_containing(text: &str, offset: usize) -> Span {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(text.len());
    Span::new(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use struct_parser::parse;

    const SPEC: &str = "\
repo: t
root: src

## Modules form a DAG

A cycle means neither module can be read alone.

check: import cycles
budget: 3

## A file fits in one head

check: files over 900 lines
budget: 10
";

    // Hover and diagnostics both need to know which standard a line belongs to,
    // and a standard's body runs to the next heading rather than being marked.
    #[test]
    fn a_line_belongs_to_the_standard_above_it() {
        let spec = parse(SPEC).spec;
        let at = SPEC.find("check: import cycles").expect("line");
        let (name, _) = standard_at(SPEC, at, &spec).expect("inside the first standard");
        assert_eq!(name, "Modules form a DAG");

        let at = SPEC.find("budget: 10").expect("line");
        let (name, _) = standard_at(SPEC, at, &spec).expect("inside the second");
        assert_eq!(name, "A file fits in one head");
    }

    // The metric vocabulary is closed and the checker refuses anything outside
    // it, so guessing is a dead end and completion is the way in.
    #[test]
    fn a_check_line_offers_the_metrics() {
        let at = SPEC.find("check: import cycles").expect("line") + 7;
        assert_eq!(completion_context(SPEC, at), CompletionContext::Metric);
    }

    // Prose is the bulk of a file and must never be interrupted by completion.
    #[test]
    fn prose_offers_nothing() {
        let at = SPEC.find("A cycle means neither").expect("line") + 10;
        assert_eq!(completion_context(SPEC, at), CompletionContext::None);
    }
}
