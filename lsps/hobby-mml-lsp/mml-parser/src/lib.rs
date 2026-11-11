//! HMML v3 (`.hmml`) parser: the review delta of one pull request, parsed into
//! an AST with byte spans and recoverable diagnostics.
//!
//! Entry points:
//! - [`parse`] — text → [`Document`] (parse-time diagnostics in
//!   [`Document::diagnostics`]).
//! - [`validate`] — semantic diagnostics over a [`Document`].
//! - [`analyze`] — both, merged and sorted by position.
//! - [`git::check_links`] — the diff-link checks against the repo checkout
//!   (grammar §8), run by `--lint` and the editor.

pub mod ast;
pub mod check;
pub mod diagnostic;
pub mod git;
pub mod parser;
pub mod span;

pub use ast::Document;
pub use check::validate;
pub use diagnostic::{Diagnostic, Severity};
pub use parser::parse;
pub use span::{LineIndex, Position, Span};

/// Parse and validate `src`, returning the [`Document`] together with all
/// parse and semantic diagnostics sorted by start position.
pub fn analyze(src: &str) -> (Document, Vec<Diagnostic>) {
    let doc = parse(src);
    let mut all = doc.diagnostics.clone();
    all.extend(validate(&doc));
    all.sort_by_key(|d| (d.span.start, d.span.end));
    (doc, all)
}
