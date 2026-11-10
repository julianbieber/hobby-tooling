//! LSP-shaped diagnostics.
//!
//! A [`Diagnostic`] carries a byte [`Span`], a [`Severity`], a stable string
//! `code` (grammar §11), and a human message. The LSP server maps these 1:1
//! onto `lsp_types::Diagnostic`.

use crate::span::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Stable diagnostic codes, exactly as listed in grammar §11.
pub mod code {
    pub const MISSING_HEADER_KEY: &str = "missing-header-key";
    pub const UNKNOWN_HEADER_KEY: &str = "unknown-header-key";
    pub const BAD_HEADER_VALUE: &str = "bad-header-value";
    pub const UNKNOWN_SECTION: &str = "unknown-section";
    pub const SECTION_ORDER: &str = "section-order";
    pub const MISSING_INTENT: &str = "missing-intent";
    pub const NO_UNIT: &str = "no-unit";
    pub const BAD_GUTTER: &str = "bad-gutter";
    pub const BAD_INDENT: &str = "bad-indent";
    pub const NESTED_CHANGE_MISMATCH: &str = "nested-change-mismatch";
    pub const CHANGE_IN_INTENT: &str = "change-in-intent";
    pub const BAD_TILDE: &str = "bad-tilde";
    pub const BAD_ANNOTATION: &str = "bad-annotation";
    pub const DUPLICATE_ANCHOR: &str = "duplicate-anchor";
    pub const UNKNOWN_API_REF: &str = "unknown-api-ref";
    pub const UNKNOWN_ANCHOR: &str = "unknown-anchor";
    pub const BAD_STRUCT: &str = "bad-struct";
    pub const DUPLICATE_TYPE: &str = "duplicate-type";
    pub const DUPLICATE_UNIT: &str = "duplicate-unit";
    pub const BAD_UNIT_HEADER: &str = "bad-unit-header";
    pub const MISPLACED_INVARIANTS: &str = "misplaced-invariants";
    pub const BAD_FN: &str = "bad-fn";
    pub const DUPLICATE_MODULE: &str = "duplicate-module";
    pub const DUPLICATE_FN: &str = "duplicate-fn";
    pub const BAD_TEST: &str = "bad-test";
    pub const NO_CHANGE: &str = "no-change";
    pub const UNLINKED_CHANGE: &str = "unlinked-change";
    pub const UNRESOLVED_DIFF_LINK: &str = "unresolved-diff-link";
    pub const CHECKOUT_NOT_FOUND: &str = "checkout-not-found";
    pub const STALE_DELTA: &str = "stale-delta";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub span: Span,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            span,
            severity: Severity::Error,
            code,
            message: message.into(),
        }
    }

    pub fn warning(span: Span, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            span,
            severity: Severity::Warning,
            code,
            message: message.into(),
        }
    }

    /// An advisory hint. Never gates the CLI's exit status.
    pub fn info(span: Span, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            span,
            severity: Severity::Info,
            code,
            message: message.into(),
        }
    }
}
