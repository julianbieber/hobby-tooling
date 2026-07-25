//! LSP-shaped diagnostics.
//!
//! A [`Diagnostic`] carries a byte [`Span`], a [`Severity`], a stable string
//! `code`, and a human message. The LSP server (task 07) maps these 1:1 onto
//! `lsp_types::Diagnostic` (span → range via [`LineIndex`], severity → LSP
//! severity, code → `NumberOrString::String`).

use crate::span::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Stable diagnostic codes. Kept as `&'static str` so callers (skills, the LSP,
/// tests) can match on exact strings. Each variant is documented where it is
/// produced (see `parser.rs` and `validate.rs`).
pub mod code {
    // Header (parse-time).
    pub const MISSING_HEADER_KEY: &str = "missing-header-key";
    pub const BAD_KIND: &str = "bad-kind";
    pub const BAD_METHOD: &str = "bad-method";
    pub const UNKNOWN_HEADER_KEY: &str = "unknown-header-key";
    pub const MALFORMED_HEADER_LINE: &str = "malformed-header-line";

    // Structure (parse-time).
    pub const MISSING_SECTION: &str = "missing-section";
    pub const DUPLICATE_SECTION: &str = "duplicate-section";
    pub const SECTION_OUT_OF_ORDER: &str = "section-out-of-order";
    pub const UNKNOWN_SECTION: &str = "unknown-section";
    /// A section that exists in the grammar but is not legal for the file's
    /// `kind` — e.g. `## data` in a `kind: plugin` spec, or `## systems` in a
    /// `kind: job` one (grammar §7.2).
    pub const SECTION_NOT_ALLOWED: &str = "section-not-allowed-for-kind";
    pub const TAB_INDENT: &str = "tab-indent";

    // Modules (parse-time).
    pub const MALFORMED_MODULE: &str = "malformed-module";
    pub const MODULE_NAME_NOT_PASCAL: &str = "module-name-not-pascal";

    // Data (parse-time).
    pub const MALFORMED_STRUCT: &str = "malformed-struct";
    pub const MALFORMED_FIELD: &str = "malformed-field";
    pub const MALFORMED_TYPE: &str = "malformed-type";
    pub const BAD_PARAM_SOURCE: &str = "bad-param-source";
    pub const UNTERMINATED_BLOCK: &str = "unterminated-block";

    // Type imports (grammar §4.5).
    pub const MALFORMED_IMPORT: &str = "malformed-import";
    pub const DUPLICATE_IMPORT: &str = "duplicate-import";
    /// Emitted only by filesystem-aware consumers (the LSP, the CLI, the index
    /// tool) when a well-formed `use` import names a spec file that does not
    /// exist on disk, or a type that the target spec does not define. The pure
    /// parse/validate passes never produce it (mirrors [`UNRESOLVED_SPEC_REF`]).
    pub const UNRESOLVED_IMPORT: &str = "unresolved-import";

    // Behavior (parse-time).
    pub const DANGLING_ELSE: &str = "dangling-else";
    pub const DANGLING_ON_ERROR: &str = "dangling-on-error";
    pub const MALFORMED_BINDING: &str = "malformed-binding";

    // Cross-spec references (grammar §5.4).
    pub const MALFORMED_SPEC_REF: &str = "malformed-spec-ref";
    pub const DUPLICATE_SPEC_REF: &str = "duplicate-spec-ref";
    /// Emitted only by filesystem-aware consumers (the LSP, the CLI, the index
    /// tool) when a well-formed `@ spec` ref does not resolve to a spec file on
    /// disk. The pure parse/validate passes never produce it.
    pub const UNRESOLVED_SPEC_REF: &str = "unresolved-spec-ref";

    // Source linkage (header `source.paths` + module `@ path`).
    /// Emitted only by filesystem-aware consumers (the LSP, the CLI's `--lint`)
    /// when a `source.paths` entry or a module `@ path` does not resolve to a
    /// file on disk under any of the goto-definition search directories. The
    /// pure parse/validate passes never produce it.
    pub const UNRESOLVED_SOURCE_PATH: &str = "unresolved-source-path";

    // Bevy dialect — `## systems` / `## access` (grammar §8, §9).
    pub const MALFORMED_SYSTEM: &str = "malformed-system";
    pub const SYSTEM_NAME_NOT_SNAKE: &str = "system-name-not-snake";
    pub const DUPLICATE_SYSTEM: &str = "duplicate-system";
    pub const MALFORMED_MARKER: &str = "malformed-marker";
    /// An attribute line that is not `key: value`, or one that appears with no
    /// system entry to attach to.
    pub const MALFORMED_ACCESS: &str = "malformed-access";
    pub const UNKNOWN_ACCESS_KEY: &str = "unknown-access-key";
    pub const DUPLICATE_ACCESS_KEY: &str = "duplicate-access-key";
    pub const MISSING_SCHEDULE: &str = "missing-schedule";
    /// A `schedule:` label outside the known Bevy set. A warning, not an error —
    /// custom schedules are legitimate.
    pub const UNKNOWN_SCHEDULE: &str = "unknown-schedule";
    /// An access-list name that resolves to no declared, imported, or built-in
    /// component / resource / event.
    pub const UNDEFINED_ACCESS_ITEM: &str = "undefined-access-item";
    /// An access-list name that resolves, but whose ECS role does not fit the
    /// key it appears under (a component in `emits:`, an event in `query:`, …).
    pub const ACCESS_ROLE_MISMATCH: &str = "access-role-mismatch";
    /// An `after:` / `before:` / `set:` name matching no system or set declared
    /// in this file. A warning — ordering across plugins is legitimate.
    pub const UNRESOLVED_SYSTEM_REF: &str = "unresolved-system-ref";

    // Semantic (validate-time).
    pub const UNDEFINED_TYPE: &str = "undefined-type";
    pub const DUPLICATE_TYPE: &str = "duplicate-type";
    pub const DUPLICATE_FIELD: &str = "duplicate-field";
    pub const DUPLICATE_MODULE: &str = "duplicate-module";
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
