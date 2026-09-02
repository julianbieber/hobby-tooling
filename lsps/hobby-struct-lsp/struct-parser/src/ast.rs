//! The parsed shape of an `.hstruct` file.
//!
//! A file is a list of **standards**. A standard is the durable half — what the
//! rule is and why it exists — and it carries at most a single number for the
//! volatile half. Nothing here names a module, a file or a type: an enumeration
//! of instances is what churns as an implementation grows, and it is what this
//! format deliberately does not store.

use crate::span::{Span, Spanned};
use serde::Serialize;

/// A whole `.hstruct` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Spec {
    pub header: Header,
    pub standards: Vec<Standard>,
}

/// The `key: value` block above the first standard.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Header {
    pub repo: Option<Spanned<String>>,
    pub root: Option<Spanned<String>>,
    pub krate: Option<Spanned<String>>,
    pub span: Span,
}

/// One `##` section: a named standard, the prose arguing for it, and how it is
/// verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Standard {
    pub name: Spanned<String>,
    /// The paragraphs between the heading and the verification keys. Kept so
    /// the LSP can show a standard's reason on hover; never interpreted.
    pub prose: String,
    pub verification: Option<Verification>,
    pub span: Span,
}

/// How a standard is held to. Every standard declares one, because a rule that
/// does not say how it is verified is a rule that reads as enforced and may not
/// be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum Verification {
    /// `check: <metric>` with a `budget: <n>` ceiling the checker enforces.
    Counted {
        metric: Spanned<Metric>,
        budget: Spanned<usize>,
    },
    /// `review: <what a reader checks>` with an `audited: <date>`. Not
    /// mechanical, and says so — which is the honest form for a rule no parser
    /// can settle.
    Reviewed {
        what: String,
        audited: Option<Spanned<String>>,
    },
}

/// The closed set of things the checker can count.
///
/// Every metric is a **count of offenders**, never a list of them, and every
/// one is computable from the code alone. A metric needing per-module
/// configuration would put the enumeration back in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "metric", rename_all = "kebab-case")]
pub enum Metric {
    /// Pairs of modules that import each other.
    ImportCycles,
    /// Files longer than `lines`.
    LargeFiles { lines: usize },
    /// Modules importing more than `limit` other modules.
    WideModules { limit: usize },
    /// Types more than one module takes `&mut` to.
    MultiWriterTypes,
    /// `Query` parameters with no `With` / `Without` / `Changed` / `Added`.
    UnfilteredQueries,
    /// Functions taking a system parameter that also allocate.
    AllocatingSystems,
}

impl Metric {
    /// How the metric is written in a `check:` line.
    pub fn render(&self) -> String {
        match self {
            Metric::ImportCycles => "import cycles".to_owned(),
            Metric::LargeFiles { lines } => format!("files over {lines} lines"),
            Metric::WideModules { limit } => format!("modules importing more than {limit}"),
            Metric::MultiWriterTypes => "multi-writer types".to_owned(),
            Metric::UnfilteredQueries => "unfiltered queries".to_owned(),
            Metric::AllocatingSystems => "allocating systems".to_owned(),
        }
    }

    /// What one offender is called, for the `--detail` listing.
    pub fn noun(&self) -> &'static str {
        match self {
            Metric::ImportCycles => "pair",
            Metric::LargeFiles { .. } => "file",
            Metric::WideModules { .. } => "module",
            Metric::MultiWriterTypes => "type",
            Metric::UnfilteredQueries => "query",
            Metric::AllocatingSystems => "system",
        }
    }
}

impl Spec {
    /// Every standard the checker can act on, with its metric and ceiling.
    pub fn counted(&self) -> impl Iterator<Item = (&Standard, &Spanned<Metric>, &Spanned<usize>)> {
        self.standards.iter().filter_map(|s| match &s.verification {
            Some(Verification::Counted { metric, budget }) => Some((s, metric, budget)),
            _ => None,
        })
    }
}
