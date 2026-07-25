//! The MML AST.
//!
//! Every node carries a [`Span`]. The tree is built to survive errors: any
//! section that fails to parse simply contributes fewer nodes (and diagnostics),
//! so the rest of the document is still represented. See `grammar.md` for the
//! authoritative shape.

use crate::diagnostic::Diagnostic;
use crate::span::{Span, Spanned};
use serde::Serialize;

/// A parsed `.mml` document. `diagnostics` holds parse-time diagnostics only;
/// run [`crate::validate::validate`] to add semantic ones.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    pub header: Header,
    pub modules: Vec<Module>,
    pub data: DataSection,
    pub behavior: Vec<Step>,
    /// `where:` symbol bindings at the end of `## behavior` (grammar §5.3).
    pub bindings: Vec<Binding>,
    pub invariants: Vec<Invariant>,
    pub diagnostics: Vec<Diagnostic>,
}

// ---- header ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Header {
    pub system: Option<Spanned<String>>,
    pub unit: Option<Spanned<String>>,
    pub kind: Option<Spanned<Kind>>,
    /// HTTP method for `kind: endpoint` (`GET`/`POST`/…); grammar §2. Optional.
    pub method: Option<Spanned<String>>,
    /// Base directory that relative `source.paths` and module `@ path` entries
    /// resolve against (tilde-expanded; may itself be relative). Optional.
    pub base: Option<Spanned<String>>,
    pub source: Option<Source>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Endpoint,
    Workflow,
    Job,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "endpoint" => Some(Kind::Endpoint),
            "workflow" => Some(Kind::Workflow),
            "job" => Some(Kind::Job),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    pub repo: Option<Spanned<String>>,
    pub paths: Vec<Spanned<String>>,
    pub span: Span,
}

// ---- modules ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Module {
    pub name: Spanned<String>,
    pub responsibility: Spanned<String>,
    pub path: Option<Spanned<String>>,
    /// Trailing `@ spec <system>/<unit>` or `@ spec <unit>` annotation
    /// (grammar §5.4). Raw ref text; split with [`split_spec_ref`].
    pub spec: Option<Spanned<String>>,
    pub span: Span,
}

/// Split a raw spec ref into `(system, unit)`. A bare ref (no `/`) has no
/// system — it resolves within the referencing file's own system.
pub fn split_spec_ref(raw: &str) -> (Option<&str>, &str) {
    match raw.split_once('/') {
        Some((system, unit)) => (Some(system), unit),
        None => (None, raw),
    }
}

/// Whether `raw` is a well-formed spec ref: `system/unit` or `unit`, each
/// segment matching the header ident rule (grammar §2):
/// `[A-Za-z][A-Za-z0-9._-]*`.
pub fn is_valid_spec_ref(raw: &str) -> bool {
    raw.matches('/').count() <= 1
        && raw.split('/').all(|seg| {
            seg.starts_with(|c: char| c.is_ascii_alphabetic())
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
}

// ---- data ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct DataSection {
    /// `use <system>/<unit>: Type, …` type imports (grammar §4.5), which pull
    /// type definitions from another unit's `## data` into this file's scope.
    pub imports: Vec<Import>,
    pub types: Vec<TypeDef>,
    pub input: Option<TypeExpr>,
    pub output: Option<TypeExpr>,
}

/// A `use` line in `## data` (grammar §4.5): `use <spec-ref>: Type, Type`.
/// It brings the named types from the referenced unit's `## data` into this
/// file's type namespace (name only — the definitions are not copied).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Import {
    /// The raw `<system>/<unit>` or bare `<unit>` ref (split with
    /// [`split_spec_ref`]); same resolution as an `@ spec` annotation.
    pub spec: Spanned<String>,
    /// The imported type names (PascalCase), each carrying its own span.
    pub types: Vec<Spanned<String>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TypeDef {
    Struct {
        name: Spanned<String>,
        fields: Vec<Field>,
        span: Span,
    },
    Enum {
        name: Spanned<String>,
        variants: Vec<Spanned<String>>,
        span: Span,
    },
}

impl TypeDef {
    pub fn name(&self) -> &Spanned<String> {
        match self {
            TypeDef::Struct { name, .. } | TypeDef::Enum { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
    /// Endpoint parameter source (`path`/`query`/`body`/`header`); grammar §4.1.
    /// `None` when the field carries no `@source` annotation.
    pub source: Option<Spanned<String>>,
    pub span: Span,
}

/// A type expression: an identifier with optional generic arguments,
/// e.g. `Vec<Offer>`, `Map<String, i64>`, `String`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TypeExpr {
    pub ident: Spanned<String>,
    pub args: Vec<TypeExpr>,
    pub span: Span,
}

// ---- behavior --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    /// The full prose text of the step (without the leading `- ` and without a
    /// trailing `@ spec` annotation).
    pub text: Spanned<String>,
    pub kind: StepKind,
    /// Trailing `@ spec` annotation (grammar §5.4).
    pub spec: Option<Spanned<String>>,
    pub children: Vec<Step>,
    pub span: Span,
}

/// The closed set of soft control-flow forms (grammar §5.1); anything else is
/// [`StepKind::Prose`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Prose,
    If,
    Else,
    ForEach,
    While,
    Try,
    OnError,
}

// ---- where: bindings -------------------------------------------------------

/// A `where:` binding (grammar §5.3): `Symbol = value [@ config-source]`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Binding {
    pub symbol: Spanned<String>,
    /// The literal value as written (an integer or a quoted string).
    pub value: Spanned<String>,
    /// Optional `@ config-source` — env var, constant, or parameter default.
    pub config: Option<Spanned<String>>,
    pub span: Span,
}

// ---- invariants ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Invariant {
    /// The invariant prose (without a trailing `@ spec` annotation).
    pub text: Spanned<String>,
    /// Trailing `@ spec` annotation (grammar §5.4).
    pub spec: Option<Spanned<String>>,
    pub span: Span,
}
