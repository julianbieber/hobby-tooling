//! LSP-shaped diagnostics and the stable code vocabulary they are reported under.

use crate::span::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// Stable diagnostic codes, matching grammar.md §6.
pub mod code {
    pub const MISSING_HEADER_KEY: &str = "missing-header-key";
    pub const UNKNOWN_HEADER_KEY: &str = "unknown-header-key";
    pub const MALFORMED_HEADER_LINE: &str = "malformed-header-line";
    pub const TAB_INDENT: &str = "tab-indent";

    pub const NO_STANDARDS: &str = "no-standards";
    pub const DUPLICATE_STANDARD: &str = "duplicate-standard";
    pub const UNVERIFIED_STANDARD: &str = "unverified-standard";
    pub const TWO_VERIFICATIONS: &str = "two-verifications";
    pub const UNKNOWN_METRIC: &str = "unknown-metric";
    pub const MALFORMED_BUDGET: &str = "malformed-budget";
    pub const MISSING_BUDGET: &str = "missing-budget";
    pub const MISSING_AUDIT: &str = "missing-audit";

    pub const BUDGET_EXCEEDED: &str = "budget-exceeded";
    pub const BUDGET_SLACK: &str = "budget-slack";

    pub const CRATE_NAME_MISMATCH: &str = "crate-name-mismatch";
    pub const MISSING_MANIFEST: &str = "missing-manifest";
    pub const MISSING_ROOT: &str = "missing-root";
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
}
