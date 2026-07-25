//! HMML (`.hmml`) parser: text → AST with byte/line spans and recoverable
//! diagnostics, plus a semantic validation pass.
//!
//! This crate is the shared core for the HMML LSP server (task 07) and a
//! command-line validator the HMML skills shell out to. Everything editor-facing
//! depends on the AST and [`Diagnostic`] shapes here, so they are kept
//! LSP-friendly (byte [`Span`]s convertible via [`LineIndex`], and diagnostics
//! carrying span + severity + stable code + message).
//!
//! Entry points:
//! - [`parse`] — text → [`Document`] (parse-time diagnostics in
//!   [`Document::diagnostics`]).
//! - [`validate`] — semantic diagnostics over a [`Document`].
//! - [`analyze`] — parse + validate, returning the document and all diagnostics
//!   merged and sorted by position.
//!
//! [`Diagnostic`]: crate::diagnostic::Diagnostic
//! [`Span`]: crate::span::Span
//! [`LineIndex`]: crate::span::LineIndex

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod lint;
pub mod parser;
pub mod paths;
pub mod span;
pub mod validate;

pub use ast::Document;
pub use diagnostic::{Diagnostic, Severity};
pub use parser::parse;
pub use span::{LineIndex, Position, Span};
pub use validate::validate;

/// The hobby-dialect spec extension, without the leading dot.
///
/// Distinct from the work dialect's `.mml` so Helix, the LSP, and the index
/// tool can tell the two apart by *file type* rather than by a path glob. Every
/// place that builds or filters a spec filename goes through this constant —
/// keep it that way, so the extension stays a one-line change.
pub const SPEC_EXT: &str = "hmml";

/// Parse and validate `src`, returning the [`Document`] together with all
/// diagnostics (parse + semantic) merged and sorted by start position. This is
/// the one-call entry point most consumers want.
pub fn analyze(src: &str) -> (Document, Vec<Diagnostic>) {
    let doc = parse(src);
    let mut all = doc.diagnostics.clone();
    all.extend(validate(&doc));
    all.sort_by_key(|d| (d.span.start, d.span.end));
    (doc, all)
}
