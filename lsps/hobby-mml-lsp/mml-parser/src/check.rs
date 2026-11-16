//! Semantic checks over a parsed [`Document`]: name uniqueness, references
//! between annotations and their targets, and the change-level warnings.

use std::collections::HashMap;

use crate::ast::*;
use crate::diagnostic::{Diagnostic, code};
use crate::span::Span;

pub fn validate(doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    anchors(doc, &mut out);
    api_refs(doc, &mut out);
    duplicates(doc, &mut out);
    changes(doc, &mut out);
    out
}

fn anchors(doc: &Document, out: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<&str, Span> = HashMap::new();
    for line in &doc.lines {
        if let Some(a) = &line.ann.anchor
            && seen.insert(&a.value, a.span).is_some()
        {
            out.push(Diagnostic::error(
                a.span,
                code::DUPLICATE_ANCHOR,
                format!("anchor `^{}` is already defined", a.value),
            ));
        }
    }
    for line in &doc.lines {
        for g in line.ann.guards.iter().flatten() {
            if !seen.contains_key(g.value.as_str()) {
                out.push(Diagnostic::error(
                    g.span,
                    code::UNKNOWN_ANCHOR,
                    format!("no line carries the anchor `^{}`", g.value),
                ));
            }
        }
    }
}

fn api_refs(doc: &Document, out: &mut Vec<Diagnostic>) {
    for line in &doc.lines {
        for a in line
            .ann
            .api
            .iter()
            .filter(|a| line.role == Role::Step && doc.resolve_api(&a.value).is_none())
        {
            out.push(Diagnostic::error(
                a.span,
                code::UNKNOWN_API_REF,
                format!("`{}` names no `## api` entry of this file", a.value),
            ));
        }
    }
}

fn same_side_dups<'a>(items: impl Iterator<Item = (&'a str, Gutter, Span)>) -> Vec<(String, Span)> {
    let mut seen: Vec<(&str, Gutter)> = Vec::new();
    let mut out = Vec::new();
    for (name, g, span) in items {
        let clash = seen.iter().any(|&(n, sg)| {
            n == name && ((sg.on_head() && g.on_head()) || (sg.on_base() && g.on_base()))
        });
        if clash {
            out.push((name.to_owned(), span));
        }
        seen.push((name, g));
    }
    out
}

fn duplicates(doc: &Document, out: &mut Vec<Diagnostic>) {
    let g = |li: usize| doc.lines.get(li).map(|l| l.gutter);
    for (name, span) in same_side_dups(
        doc.data
            .iter()
            .filter_map(|t| Some((t.name.value.as_str(), g(t.line)?, t.name.span))),
    ) {
        out.push(Diagnostic::error(
            span,
            code::DUPLICATE_TYPE,
            format!("type `{name}` is defined twice"),
        ));
    }
    let mut units: HashMap<&str, Span> = HashMap::new();
    for u in &doc.units {
        if !u.name.value.is_empty() && units.insert(&u.name.value, u.name.span).is_some() {
            out.push(Diagnostic::error(
                u.name.span,
                code::DUPLICATE_UNIT,
                format!("unit `{}` appears twice", u.name.value),
            ));
        }
    }
    for (name, span) in same_side_dups(
        doc.api
            .iter()
            .filter_map(|m| Some((m.name.value.as_str(), g(m.line)?, m.name.span))),
    ) {
        out.push(Diagnostic::error(
            span,
            code::DUPLICATE_MODULE,
            format!("module `{name}` is listed twice"),
        ));
    }
    for m in &doc.api {
        for (name, span) in same_side_dups(
            m.fns
                .iter()
                .filter_map(|f| Some((f.name.value.as_str(), g(f.line)?, f.name.span))),
        ) {
            out.push(Diagnostic::error(
                span,
                code::DUPLICATE_FN,
                format!("`{}` lists `fn {name}` twice", m.name.value),
            ));
        }
    }
}

fn has_link_below(doc: &Document, idx: usize, gutter: Gutter) -> bool {
    doc.children(idx).any(|c| {
        doc.lines.get(c).is_some_and(|l| {
            l.gutter == gutter && (l.ann.link.is_some() || has_link_below(doc, c, gutter))
        })
    })
}

fn has_link_above(doc: &Document, idx: usize, gutter: Gutter) -> bool {
    let mut cur = doc.lines.get(idx).and_then(|l| l.parent);
    while let Some(l) = cur.and_then(|p| doc.lines.get(p)) {
        if l.gutter != gutter {
            return false;
        }
        if l.ann.link.is_some() {
            return true;
        }
        cur = l.parent;
    }
    false
}

fn changes(doc: &Document, out: &mut Vec<Diagnostic>) {
    if !doc.lines.iter().any(|l| l.gutter.is_change()) {
        let span = doc.sections.first().map_or(Span::at(0), |s| s.header);
        out.push(Diagnostic::warning(
            span,
            code::NO_CHANGE,
            "no `+`, `-` or `~` line anywhere; the delta shows no change",
        ));
    }
    for (i, line) in doc.lines.iter().enumerate() {
        if !line.gutter.is_change() || line.ann.link.is_some() {
            continue;
        }
        let what = match line.role {
            Role::Step => {
                let block = matches!(line.gutter, Gutter::Added | Gutter::Removed);
                if block
                    && (has_link_below(doc, i, line.gutter) || has_link_above(doc, i, line.gutter))
                {
                    continue;
                }
                "changed step"
            }
            Role::Fn => "changed `fn` entry",
            Role::Test => "changed test line",
            _ => continue,
        };
        out.push(Diagnostic::warning(
            line.span,
            code::UNLINKED_CHANGE,
            format!("{what} without a diff link `@ <file>:<line>`"),
        ));
    }
}
