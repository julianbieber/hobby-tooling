//! Parsing and checking `.hstruct` files — a repo's code quality standards.
//!
//! `grammar.md` beside this crate is the source of truth for the language; this
//! crate implements it. A file states **standards**: what a rule is, why it
//! exists, and how it is verified. A counted standard carries a **budget** — a
//! ceiling on how many offenders the repo may have — and never the offenders
//! themselves, because a stored list of instances is what goes stale as an
//! implementation grows.

pub mod ast;
pub mod check;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod rust_source;
pub mod span;

pub use ast::{Metric, Spec, Standard, Verification};
pub use check::{Checked, Measured, check, has_errors, ratchet};
pub use diagnostic::{Diagnostic, Severity};
pub use parser::{Parsed, parse};

/// The filename an `.hstruct` file has at a repo or crate root.
pub const SPEC_FILE: &str = "structure.hstruct";

/// The extension the language is routed by.
pub const SPEC_EXT: &str = "hstruct";
