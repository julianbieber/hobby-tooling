//! Turning significant lines into a [`Spec`], with parse-time diagnostics.
//!
//! Everything here is decidable from the file alone. What the counts actually
//! are, and whether a budget holds, belongs to the checker.

use crate::ast::{Header, Metric, Spec, Standard, Verification};
use crate::diagnostic::{Diagnostic, code};
use crate::lexer::{Line, Marker, lines};
use crate::span::{Span, Spanned};

/// A parsed file and everything the parser noticed while reading it. Parsing
/// always yields a `Spec`: a file with errors is still parsed as far as it can
/// be, so an editor keeps working while a line is half-typed.
pub struct Parsed {
    pub spec: Spec,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse `.hstruct` source.
pub fn parse(src: &str) -> Parsed {
    let mut p = Parser {
        diagnostics: Vec::new(),
    };
    let ls = lines(src);
    let spec = p.file(&ls);
    Parsed {
        spec,
        diagnostics: p.diagnostics,
    }
}

struct Parser {
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn error(&mut self, span: Span, code: &'static str, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(span, code, msg));
    }

    fn warning(&mut self, span: Span, code: &'static str, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic::warning(span, code, msg));
    }

    fn file(&mut self, ls: &[Line]) -> Spec {
        for l in ls {
            if l.tab_indent {
                self.error(l.span, code::TAB_INDENT, "indent with spaces, not tabs");
            }
        }

        let split = ls
            .iter()
            .position(|l| l.marker == Marker::Section)
            .unwrap_or(ls.len());
        let (header_lines, standard_lines) = ls.split_at_checked(split).unwrap_or((ls, &[]));
        let header = self.header(header_lines);
        let standards = self.standards(standard_lines);

        for (key, present) in [
            ("repo", header.repo.is_some()),
            ("root", header.root.is_some()),
        ] {
            if !present {
                self.error(
                    header.span,
                    code::MISSING_HEADER_KEY,
                    format!("`{key}:` is required"),
                );
            }
        }
        if standards.is_empty() {
            self.error(
                Span::at(0),
                code::NO_STANDARDS,
                "a file states no standards",
            );
        }

        let mut seen: Vec<&str> = Vec::new();
        for s in &standards {
            if seen.contains(&s.name.value.as_str()) {
                self.error(
                    s.name.span,
                    code::DUPLICATE_STANDARD,
                    format!("`{}` is stated twice", s.name.value),
                );
            }
            seen.push(&s.name.value);
        }

        Spec { header, standards }
    }

    fn header(&mut self, ls: &[Line]) -> Header {
        let mut h = Header {
            span: match (ls.first(), ls.last()) {
                (Some(first), Some(last)) => Span::new(first.span.start, last.span.end),
                _ => Span::at(0),
            },
            ..Header::default()
        };
        for l in ls {
            let Some((key, value)) = l.text.split_once(':') else {
                self.error(l.span, code::MALFORMED_HEADER_LINE, "expected `key: value`");
                continue;
            };
            let value = Spanned::new(value.trim().to_owned(), l.span);
            match key.trim() {
                "repo" => h.repo = Some(value),
                "root" => h.root = Some(value),
                "crate" => h.krate = Some(value),
                other => self.error(
                    l.span,
                    code::UNKNOWN_HEADER_KEY,
                    format!("unknown header key `{other}`"),
                ),
            }
        }
        h
    }

    fn standards(&mut self, ls: &[Line]) -> Vec<Standard> {
        let mut out: Vec<Standard> = Vec::new();
        let mut check: Option<Spanned<Metric>> = None;
        let mut budget: Option<Spanned<usize>> = None;
        let mut review: Option<String> = None;
        let mut audited: Option<Spanned<String>> = None;

        for l in ls {
            if l.marker == Marker::Section {
                self.finish(&mut out, &mut check, &mut budget, &mut review, &mut audited);
                out.push(Standard {
                    name: Spanned::new(l.text.clone(), l.span),
                    prose: String::new(),
                    verification: None,
                    span: l.span,
                });
                continue;
            }
            let Some(current) = out.last_mut() else {
                continue;
            };
            current.span = Span::new(current.span.start, l.span.end);

            match l.text.split_once(':') {
                Some((key, value)) if is_verification_key(key.trim()) => {
                    let value = value.trim();
                    match key.trim() {
                        "check" => match parse_metric(value) {
                            Some(m) => check = Some(Spanned::new(m, l.span)),
                            None => self.error(
                                l.span,
                                code::UNKNOWN_METRIC,
                                format!("`{value}` is not a metric the checker counts"),
                            ),
                        },
                        "budget" => match value.parse::<usize>() {
                            Ok(n) => budget = Some(Spanned::new(n, l.span)),
                            Err(_) => self.error(
                                l.span,
                                code::MALFORMED_BUDGET,
                                "a budget is a whole number of offenders",
                            ),
                        },
                        "review" => review = Some(value.to_owned()),
                        _ => audited = Some(Spanned::new(value.to_owned(), l.span)),
                    }
                }
                _ => {
                    if !current.prose.is_empty() {
                        current.prose.push(' ');
                    }
                    current.prose.push_str(&l.text);
                }
            }
        }
        self.finish(&mut out, &mut check, &mut budget, &mut review, &mut audited);
        out
    }

    fn finish(
        &mut self,
        out: &mut [Standard],
        check: &mut Option<Spanned<Metric>>,
        budget: &mut Option<Spanned<usize>>,
        review: &mut Option<String>,
        audited: &mut Option<Spanned<String>>,
    ) {
        let Some(current) = out.last_mut() else {
            return;
        };
        current.verification = match (check.take(), budget.take(), review.take()) {
            (Some(metric), Some(budget), None) => Some(Verification::Counted { metric, budget }),
            (Some(metric), None, None) => {
                self.diagnostics.push(Diagnostic::error(
                    metric.span,
                    code::MISSING_BUDGET,
                    "a `check:` needs a `budget:`; without a ceiling it counts and enforces nothing",
                ));
                None
            }
            (None, _, Some(what)) => Some(Verification::Reviewed {
                what,
                audited: audited.take(),
            }),
            (Some(metric), _, Some(_)) => {
                self.diagnostics.push(Diagnostic::error(
                    metric.span,
                    code::TWO_VERIFICATIONS,
                    "a standard is either counted or reviewed, not both",
                ));
                None
            }
            (None, _, None) => {
                self.diagnostics.push(Diagnostic::error(
                    current.name.span,
                    code::UNVERIFIED_STANDARD,
                    "state a `check:` + `budget:`, or a `review:` — a standard \
                     that does not say how it is held to reads as enforced",
                ));
                None
            }
        };
        if let Some(Verification::Reviewed { audited: None, .. }) = &current.verification {
            self.warning(
                current.name.span,
                code::MISSING_AUDIT,
                "a reviewed standard needs an `audited:` date, or nothing records \
                 that it was ever looked at",
            );
        }
        audited.take();
    }
}

fn is_verification_key(key: &str) -> bool {
    matches!(key, "check" | "budget" | "review" | "audited")
}

fn parse_metric(text: &str) -> Option<Metric> {
    if text == "import cycles" {
        return Some(Metric::ImportCycles);
    }
    if text == "multi-writer types" {
        return Some(Metric::MultiWriterTypes);
    }
    if text == "unfiltered queries" {
        return Some(Metric::UnfilteredQueries);
    }
    if text == "allocating systems" {
        return Some(Metric::AllocatingSystems);
    }
    if let Some(rest) = text.strip_prefix("files over ")
        && let Some(n) = rest.strip_suffix(" lines")
        && let Ok(lines) = n.trim().parse()
    {
        return Some(Metric::LargeFiles { lines });
    }
    if let Some(rest) = text.strip_prefix("modules importing more than ")
        && let Ok(limit) = rest.trim().parse()
    {
        return Some(Metric::WideModules { limit });
    }
    None
}
