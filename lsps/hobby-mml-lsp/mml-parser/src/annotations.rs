//! Change annotations (grammar §10) — the `# +` / `# - <text>` markers a spec
//! carries while a change is in flight.
//!
//! The unannotated lines are the **baseline** — the version the change starts
//! from — and the markers are the change written on top of it, so the file
//! holds both versions. The markers are never stripped once the change lands:
//! the spec goes on representing it. To the language they are ordinary comments
//! (§1) — the parser skips them and no AST node exists for them — which is
//! exactly why the checks live here: the one rule that makes the convention
//! work is invisible to every other pass.
//!
//! **Baseline recoverability.** Dropping every `# +` block and un-commenting
//! every `# -` line must reproduce the unannotated baseline. [`baseline`]
//! performs that reconstruction and [`check`] re-parses the result: an error the
//! baseline has and the annotated spec does not means the markers do not
//! describe a real delta (typically a `# +` on something the baseline itself
//! already needs), so the file no longer holds two coherent versions.
//!
//! Everything here is advisory — warnings, never errors. An annotated spec is a
//! valid spec.

use std::collections::BTreeSet;

use crate::ast::Document;
use crate::diagnostic::{Diagnostic, code};
use crate::lexer::{Line, LineKind, lex};
use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// `# +` — covers the line beneath it and that line's nested children.
    Added,
    /// `# - <text>` — a baseline line this change removes, commented in place.
    Removed,
}

#[derive(Debug, Clone)]
pub struct Annotation<'a> {
    pub marker: Marker,
    /// 0-based line number of the marker.
    pub line_no: usize,
    /// Indentation of the marker line (the depth its content sits at).
    pub indent: usize,
    /// Text after the sign, with one separating space removed. Empty for a bare
    /// `# +`, and empty for a `# -` that kept nothing (which is the bug
    /// [`code::EMPTY_CHANGE_REMOVAL`] reports).
    pub text: &'a str,
    /// Span of the marker line's content.
    pub span: Span,
}

/// Recognise a marker in a comment line's `content` (already trimmed, so it
/// starts at the `#`). A repeated sign — `# --- divider ---`, `# +++` — is a
/// rule line, not a marker.
fn parse_marker(content: &str) -> Option<(Marker, &str)> {
    let rest = content.strip_prefix('#')?;
    let rest = rest.trim_start_matches([' ', '\t']);
    let marker = match rest.as_bytes().first()? {
        b'+' => Marker::Added,
        b'-' => Marker::Removed,
        _ => return None,
    };
    let tail = &rest[1..];
    if tail.starts_with('+') || tail.starts_with('-') {
        return None;
    }
    Some((marker, tail.strip_prefix(' ').unwrap_or(tail).trim_end()))
}

/// Every change annotation in `src`, in source order.
pub fn scan(src: &str) -> Vec<Annotation<'_>> {
    lex(src)
        .iter()
        .filter(|line| line.kind == LineKind::Comment)
        .filter_map(|line| {
            let (marker, text) = parse_marker(line.content)?;
            let (start, end) = line.content_span();
            Some(Annotation {
                marker,
                line_no: line.line_no,
                indent: line.indent,
                text,
                span: Span::new(start, end),
            })
        })
        .collect()
}

/// What an active `# +` still covers while the baseline is reconstructed.
#[derive(Debug, Clone, Copy)]
enum Covered {
    /// The added line's deeper-indented children.
    Subtree(usize),
    /// A `struct`/`enum` body, through the `}` that closes it — the brace sits
    /// at the opening line's own indentation, so the subtree rule would stop
    /// one line short of it.
    Braces(usize),
}

/// The unannotated spec an annotated one reconstructs to, plus the original
/// line each reconstructed line came from (so a diagnostic found in the
/// baseline can be reported where the author can see it).
pub struct Baseline {
    pub text: String,
    pub origin: Vec<usize>,
    /// 0-based source lines a `# +` introduced — the added line and everything
    /// its marker covers. These are the lines that are *not* in the baseline.
    pub added: BTreeSet<usize>,
}

/// The source lines a `# +` marker introduces. Empty for an unannotated spec.
///
/// A marker is a comment and the parser skips it, but the line beneath it is
/// ordinary content the parser reads like any other — so a pass that asks "is
/// this true of the code today?" has to exclude these itself, or it reads a
/// proposed member as an assertion about code that does not have it yet.
pub fn added_lines(src: &str) -> BTreeSet<usize> {
    baseline(src).added
}

/// Which sections write their content as `- ` bullets, so a removed line —
/// whose bullet the marker's own `-` stands in for — needs one restored.
fn bulleted(section: Option<&str>, in_source_paths: bool, removed: &str) -> bool {
    match section {
        Some("behavior") | Some("invariants") => true,
        // `## api` interleaves `fn` signature lines with the bullets nested
        // under them, so only the bullets need one restored (grammar §11).
        Some("api") => !removed.starts_with("fn "),
        // The header's `source.paths:` entries are bullets too.
        None => in_source_paths,
        _ => false,
    }
}

/// Reconstruct the baseline: drop every `# +` block, un-comment every `# -`
/// line, leave everything else untouched.
pub fn baseline(src: &str) -> Baseline {
    let lines = lex(src);
    let mut text = String::with_capacity(src.len());
    let mut origin = Vec::new();
    let mut added = BTreeSet::new();
    let mut section: Option<&str> = None;
    let mut in_source_paths = false;
    // What the active `# +` is still covering, if anything (grammar §10).
    let mut dropping: Option<Covered> = None;
    let mut awaiting_added_line = false;

    for line in &lines {
        let annotation = (line.kind == LineKind::Comment)
            .then(|| parse_marker(line.content))
            .flatten();

        // The added line itself: the first content line under the marker.
        if awaiting_added_line {
            if line.kind == LineKind::Content && annotation.is_none() {
                awaiting_added_line = false;
                dropping = Some(if line.content.ends_with('{') {
                    Covered::Braces(line.indent)
                } else {
                    Covered::Subtree(line.indent)
                });
                added.insert(line.line_no);
                continue;
            }
            // Nothing followed the marker; `empty-change-addition` reports it.
            awaiting_added_line = false;
        }

        // Then the rest of what the marker covers: a `struct`/`enum` body
        // through its closing brace, otherwise the added line's deeper children.
        match dropping {
            Some(Covered::Braces(indent)) => {
                let closes = line.kind == LineKind::Content
                    && line.indent <= indent
                    && line.content.starts_with('}');
                if closes {
                    dropping = None;
                }
                added.insert(line.line_no);
                continue;
            }
            Some(Covered::Subtree(indent)) => {
                let continues =
                    line.kind == LineKind::Content && annotation.is_none() && line.indent > indent;
                if continues {
                    added.insert(line.line_no);
                    continue;
                }
                dropping = None;
            }
            None => {}
        }

        match line.kind {
            LineKind::Section => {
                section = Some(line.content.trim_start_matches('#').trim());
                in_source_paths = false;
            }
            LineKind::Content if section.is_none() => {
                in_source_paths = if line.content.starts_with('-') {
                    in_source_paths
                } else {
                    line.content.trim_end().ends_with("paths:")
                };
            }
            _ => {}
        }

        match annotation {
            Some((Marker::Added, _)) => {
                awaiting_added_line = true;
                continue;
            }
            Some((Marker::Removed, removed)) if !removed.is_empty() => {
                let bullet =
                    if bulleted(section, in_source_paths, removed) && !removed.starts_with("- ") {
                        "- "
                    } else {
                        ""
                    };
                text.push_str(&" ".repeat(line.indent));
                text.push_str(bullet);
                text.push_str(removed);
                text.push('\n');
                origin.push(line.line_no);
            }
            Some((Marker::Removed, _)) => {}
            None => {
                let (_, content_end) = line.content_span();
                text.push_str(&src[line.start..content_end]);
                text.push('\n');
                origin.push(line.line_no);
            }
        }
    }

    Baseline {
        text,
        origin,
        added,
    }
}

/// Run every change-annotation check over `doc` / `src`. Returns nothing for a
/// spec with no annotations, which is the common case.
pub fn check(doc: &Document, src: &str) -> Vec<Diagnostic> {
    let annotations = scan(src);
    if annotations.is_empty() {
        return Vec::new();
    }
    let mut diags = Vec::new();
    let lines = lex(src);

    for annotation in &annotations {
        match annotation.marker {
            Marker::Removed if annotation.text.is_empty() => diags.push(Diagnostic::warning(
                annotation.span,
                code::EMPTY_CHANGE_REMOVAL,
                "`# -` with nothing after it: keep the removed line's text verbatim \
                 after the marker, or the baseline can no longer be recovered \
                 (grammar §10)",
            )),
            Marker::Removed if annotation.text.starts_with("- ") => {
                diags.push(Diagnostic::warning(
                    annotation.span,
                    code::REDUNDANT_CHANGE_BULLET,
                    "the marker's own `-` stands in for the removed line's bullet; \
                 write `# - <text>` (grammar §10)",
                ))
            }
            Marker::Added if !opens_a_block(&lines, annotation) => diags.push(Diagnostic::warning(
                annotation.span,
                code::EMPTY_CHANGE_ADDITION,
                "`# +` introduces nothing: the marker covers the line directly \
                 beneath it and that line's nested children (grammar §10)",
            )),
            _ => {}
        }
    }

    diags.extend(unrecoverable_baseline(doc, src, &lines));
    diags
}

/// Whether the line after an added-marker is content it can actually cover.
fn opens_a_block(lines: &[Line<'_>], annotation: &Annotation<'_>) -> bool {
    lines
        .get(annotation.line_no + 1)
        .is_some_and(|next| next.kind == LineKind::Content)
}

/// Reconstruct the baseline and report anything it breaks that the annotated
/// spec does not — the check that keeps "the unannotated lines are a version
/// that stands on its own" true.
fn unrecoverable_baseline(doc: &Document, src: &str, lines: &[Line<'_>]) -> Vec<Diagnostic> {
    let errors = |diags: Vec<Diagnostic>| -> Vec<Diagnostic> {
        diags
            .into_iter()
            .filter(|d| d.severity == crate::diagnostic::Severity::Error)
            .collect()
    };

    let mut intended: Vec<Diagnostic> = doc.diagnostics.clone();
    intended.extend(crate::validate::validate(doc));
    let intended = errors(intended);

    let reconstructed = baseline(src);
    let baseline_doc = crate::parser::parse(&reconstructed.text);
    let mut found: Vec<Diagnostic> = baseline_doc.diagnostics.clone();
    found.extend(crate::validate::validate(&baseline_doc));

    let index = crate::span::LineIndex::new(&reconstructed.text);
    errors(found)
        .into_iter()
        .filter(|d| {
            !intended
                .iter()
                .any(|i| i.code == d.code && i.message == d.message)
        })
        .map(|d| {
            let line_no = index.position(d.span.start).line as usize;
            let span = reconstructed
                .origin
                .get(line_no)
                .and_then(|origin| lines.get(*origin))
                .map(|line| {
                    let (start, end) = line.content_span();
                    Span::new(start, end)
                })
                .unwrap_or(Span::at(0));
            Diagnostic::warning(
                span,
                code::UNRECOVERABLE_BASELINE,
                format!(
                    "dropping the `# +` blocks and un-commenting the `# -` lines gives \
                     a baseline that does not hold: {}. The markers do not describe a \
                     real delta — the unannotated lines are supposed to stand on their \
                     own as the version this change starts from (grammar §10)",
                    d.message
                ),
            )
        })
        .collect()
}
