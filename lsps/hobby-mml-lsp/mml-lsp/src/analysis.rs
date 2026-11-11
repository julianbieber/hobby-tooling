//! Cursor logic over a parsed delta, free of protocol types so it can be
//! unit-tested: definitions, hover text, references, symbols, completions.

use std::path::{Path, PathBuf};

use mml_parser::Document;
use mml_parser::ast::{DiffLink, EcsRole, Line, SectionKind, Side, TestKind, TypeKind};
use mml_parser::git::{self, GitCache, Resolution};
use mml_parser::parser::HEADER_KEYS;
use mml_parser::span::{Position, Span};

pub fn offset_of(text: &str, pos: Position) -> usize {
    let mut line_start = 0usize;
    if pos.line > 0 {
        let mut seen = 0u32;
        let mut found = None;
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                seen += 1;
                if seen == pos.line {
                    found = Some(i + 1);
                    break;
                }
            }
        }
        match found {
            Some(s) => line_start = s,
            None => return text.len(),
        }
    }
    let mut utf16 = 0u32;
    for (rel, ch) in text[line_start..].char_indices() {
        if utf16 >= pos.character || ch == '\n' {
            return line_start + rel;
        }
        utf16 += ch.len_utf16() as u32;
    }
    text.len()
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The identifier under (or just before) `offset`.
pub fn word_at(text: &str, offset: usize) -> Option<(Span, &str)> {
    let offset = offset.min(text.len());
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word(c))
        .last()
        .map_or(offset, |(i, _)| i);
    let end = text[offset..]
        .char_indices()
        .find(|&(_, c)| !is_word(c))
        .map_or(text.len(), |(i, _)| offset + i);
    (start < end).then(|| (Span::new(start, end), &text[start..end]))
}

fn within(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

/// Whole-word occurrences of `word` in `text`.
pub fn occurrences(text: &str, word: &str) -> Vec<Span> {
    text.match_indices(word)
        .filter(|&(i, _)| {
            let before = text[..i].chars().next_back().is_none_or(|c| !is_word(c));
            let after = text[i + word.len()..]
                .chars()
                .next()
                .is_none_or(|c| !is_word(c));
            before && after
        })
        .map(|(i, _)| Span::new(i, i + word.len()))
        .collect()
}

#[derive(Debug, Clone)]
pub enum Definition {
    Local(Span),
    Link(DiffLink, Side),
}

pub fn definition(doc: &Document, text: &str, offset: usize) -> Option<Definition> {
    for line in &doc.lines {
        if let Some(a) = line.ann.api.iter().find(|a| within(a.span, offset)) {
            let (_, f) = doc.resolve_api(&a.value)?;
            return Some(Definition::Local(doc.lines[f.line].span));
        }
        for g in line.ann.guards.iter().flatten() {
            if within(g.span, offset) {
                let target = doc
                    .lines
                    .iter()
                    .find(|l| l.ann.anchor.as_ref().is_some_and(|a| a.value == g.value))?;
                return Some(Definition::Local(target.span));
            }
        }
    }
    if let Some((link, side)) = doc.link_at(offset) {
        return Some(Definition::Link(link.clone(), side));
    }
    let (_, word) = word_at(text, offset)?;
    if let Some(t) = doc.type_def(word) {
        return Some(Definition::Local(t.name.span));
    }
    doc.module(word).map(|m| Definition::Local(m.name.span))
}

/// The file and 0-based line a diff link points at, from the checkout.
pub fn link_target(
    cache: &mut GitCache,
    doc: &Document,
    delta: &Path,
    home: &Path,
    cache_dir: &Path,
    link: &DiffLink,
    side: Side,
) -> Option<(PathBuf, u32)> {
    let t = git::target(cache, doc, delta, home)?;
    let sha = t.sha(side)?.to_owned();
    let base = t.base.clone().unwrap_or_else(|| sha.clone());
    let Resolution::Found(path) =
        git::resolve_file(cache, &t.checkout, &base, &t.head, &sha, &link.file.value)
    else {
        return None;
    };
    let file = git::source_file(&t.checkout, &sha, &path, cache_dir)?;
    let line = link.line.as_ref().map_or(0, |n| n.value.saturating_sub(1));
    Some((file, line))
}

pub fn hover(doc: &Document, text: &str, offset: usize) -> Option<String> {
    for line in &doc.lines {
        if let Some(a) = line.ann.api.iter().find(|a| within(a.span, offset)) {
            let (m, f) = doc.resolve_api(&a.value)?;
            return Some(format!(
                "```hmml\nfn {}\n```\n{} — {}",
                f.signature.trim_start_matches("fn "),
                m.name.value,
                m.responsibility
            ));
        }
    }
    if let Some(i) = doc.line_at(offset)
        && let Some(t) = &doc.lines[i].tilde
    {
        let in_ann = doc.lines[i].ann.span.is_some_and(|s| within(s, offset));
        if !in_ann {
            return Some(format!("**before:** {}\n\n**after:** {}", t.old, t.new));
        }
    }
    let (_, word) = word_at(text, offset)?;
    if let Some(t) = doc.type_def(word) {
        let span = if t.span.end > t.span.start {
            t.span
        } else {
            t.name.span
        };
        let src = &text[span.start.min(text.len())..span.end.min(text.len())];
        let body: Vec<&str> = src.lines().map(|l| l.get(2..).unwrap_or("")).collect();
        return Some(format!("```hmml\n{}\n```", body.join("\n")));
    }
    doc.module(word)
        .map(|m| format!("**{}** — {}", m.name.value, m.responsibility))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RefKey {
    Anchor,
    Fn(usize, usize),
}

fn api_key(doc: &Document, path: &str) -> Option<RefKey> {
    let (m, f) = doc.resolve_api(path)?;
    let mi = doc.api.iter().position(|x| std::ptr::eq(x, m))?;
    let fi = m.fns.iter().position(|x| std::ptr::eq(x, f))?;
    Some(RefKey::Fn(mi, fi))
}

pub fn references(doc: &Document, text: &str, offset: usize, include_decl: bool) -> Vec<Span> {
    let mut found: Option<(RefKey, String)> = None;
    for line in &doc.lines {
        if let Some(a) = &line.ann.anchor
            && within(a.span, offset)
        {
            found = Some((RefKey::Anchor, a.value.clone()));
        }
        for g in line.ann.guards.iter().flatten() {
            if within(g.span, offset) {
                found = Some((RefKey::Anchor, g.value.clone()));
            }
        }
        if let Some(a) = line.ann.api.iter().find(|a| within(a.span, offset))
            && let Some(k) = api_key(doc, &a.value)
        {
            found = Some((k, String::new()));
        }
    }
    for (mi, m) in doc.api.iter().enumerate() {
        for (fi, f) in m.fns.iter().enumerate() {
            if within(f.name.span, offset) {
                found = Some((RefKey::Fn(mi, fi), String::new()));
            }
        }
    }
    match found {
        Some((RefKey::Anchor, name)) => {
            let mut out = Vec::new();
            for line in &doc.lines {
                if include_decl
                    && let Some(a) = &line.ann.anchor
                    && a.value == name
                {
                    out.push(a.span);
                }
                out.extend(
                    line.ann
                        .guards
                        .iter()
                        .flatten()
                        .filter(|g| g.value == name)
                        .map(|g| g.span),
                );
            }
            out
        }
        Some((key @ RefKey::Fn(mi, fi), _)) => {
            let mut out = Vec::new();
            if include_decl {
                out.push(doc.api[mi].fns[fi].name.span);
            }
            out.extend(doc.lines.iter().flat_map(|l| {
                l.ann
                    .api
                    .iter()
                    .filter(|a| api_key(doc, &a.value) == Some(key))
                    .map(|a| a.span)
            }));
            out
        }
        None => {
            let Some((_, word)) = word_at(text, offset) else {
                return Vec::new();
            };
            let decls: Vec<Span> = doc
                .data
                .iter()
                .filter(|t| t.name.value == word)
                .map(|t| t.name.span)
                .chain(
                    doc.api
                        .iter()
                        .filter(|m| m.name.value == word)
                        .map(|m| m.name.span),
                )
                .collect();
            if decls.is_empty() {
                return Vec::new();
            }
            occurrences(text, word)
                .into_iter()
                .filter(|s| include_decl || !decls.contains(s))
                .collect()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    Namespace,
    Unit,
    Anchor,
    Struct,
    Enum,
    Field,
    Variant,
    Module,
    Function,
    Test,
    Text,
}

#[derive(Debug, Clone)]
pub struct Sym {
    pub name: String,
    pub detail: Option<String>,
    pub kind: SymKind,
    pub span: Span,
    pub selection: Span,
    pub children: Vec<Sym>,
}

fn sym(name: impl Into<String>, kind: SymKind, span: Span, selection: Span) -> Sym {
    Sym {
        name: name.into(),
        detail: None,
        kind,
        span,
        selection,
        children: Vec::new(),
    }
}

fn section_span(doc: &Document, kind: SectionKind) -> Option<(Span, Span)> {
    doc.sections
        .iter()
        .find(|s| s.kind == kind)
        .map(|s| (s.span, s.header))
}

pub fn symbols(doc: &Document) -> Vec<Sym> {
    let mut out = Vec::new();
    if let Some((span, header)) = section_span(doc, SectionKind::Intent) {
        let mut s = sym("intent", SymKind::Namespace, span, header);
        for &li in &doc.intent {
            let l = &doc.lines[li];
            s.children
                .push(sym(l.text.clone(), SymKind::Text, l.span, l.span));
        }
        out.push(s);
    }
    for t in &doc.data {
        let kind = match t.kind {
            TypeKind::Struct => SymKind::Struct,
            TypeKind::Enum => SymKind::Enum,
        };
        let span = Span::new(t.span.start, t.span.end.max(t.name.span.end));
        let mut s = sym(t.name.value.clone(), kind, span, t.name.span);
        s.detail = t.role.map(|r| r.keyword().to_owned());
        for f in &t.fields {
            let fk = if f.ty.is_some() {
                SymKind::Field
            } else {
                SymKind::Variant
            };
            let mut c = sym(f.name.value.clone(), fk, f.name.span, f.name.span);
            c.detail = f.ty.as_ref().map(|t| t.value.clone());
            s.children.push(c);
        }
        out.push(s);
    }
    for u in &doc.units {
        let mut s = sym(
            format!("unit {}", u.name.value),
            SymKind::Unit,
            u.span,
            u.name.span,
        );
        s.detail = Some(match &u.trigger {
            Some(t) => format!("{} {} — {}", t.keyword(), t.target().value, u.what),
            None => u.what.clone(),
        });
        for &li in u.steps.iter().chain(&u.invariants) {
            if let Some(a) = &doc.lines[li].ann.anchor {
                let mut c = sym(
                    format!("^{}", a.value),
                    SymKind::Anchor,
                    doc.lines[li].span,
                    a.span,
                );
                c.detail = Some(doc.lines[li].text.clone());
                s.children.push(c);
            }
        }
        s.children.sort_by_key(|c| c.span.start);
        out.push(s);
    }
    for m in &doc.api {
        let mut s = sym(m.name.value.clone(), SymKind::Module, m.span, m.name.span);
        s.detail = Some(m.responsibility.clone());
        for f in &m.fns {
            let line = &doc.lines[f.line];
            let mut c = sym(
                f.name.value.clone(),
                SymKind::Function,
                line.span,
                f.name.span,
            );
            c.detail = Some(f.signature.clone());
            s.children.push(c);
        }
        out.push(s);
    }
    if let Some((span, header)) = section_span(doc, SectionKind::Tests) {
        let mut s = sym("tests", SymKind::Namespace, span, header);
        for t in &doc.tests {
            let name = match &t.kind {
                TestKind::Single(n) => format!("test {n}"),
                TestKind::Group(n) => format!("tests {n}"),
            };
            let mut c = sym(name, SymKind::Test, doc.lines[t.line].span, t.label.span);
            c.detail = Some(t.description.clone());
            s.children.push(c);
        }
        out.push(s);
    }
    out.sort_by_key(|s| s.span.start);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Section,
    Key,
    Member,
    Anchor,
    Keyword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub insert: String,
    pub replace: Span,
    pub kind: ItemKind,
    pub detail: Option<String>,
}

pub const STEP_KEYWORDS: [&str; 6] = ["if ", "else:", "for each ", "while ", "try:", "on error "];

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |i| i + 1)
}

fn section_at(doc: &Document, offset: usize) -> Option<SectionKind> {
    doc.sections
        .iter()
        .rev()
        .find(|s| s.header.start <= offset)
        .map(|s| s.kind)
}

fn api_members(doc: &Document) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for m in &doc.api {
        for f in &m.fns {
            let prefix = format!("{}::", m.name.value);
            let label = if f.name.value.starts_with(&prefix) {
                f.name.value.clone()
            } else {
                format!("{prefix}{}", f.name.value)
            };
            out.push((label, f.signature.clone()));
        }
    }
    out
}

fn anchors(doc: &Document) -> Vec<&Line> {
    doc.lines
        .iter()
        .filter(|l| l.ann.anchor.is_some())
        .collect()
}

pub fn completions(doc: &Document, text: &str, offset: usize) -> Vec<Item> {
    let offset = offset.min(text.len());
    let ls = line_start(text, offset);
    let prefix = &text[ls..offset];
    let first_section = doc.sections.first().map(|s| s.header.start);
    let item =
        |label: &str, insert: &str, replace: Span, kind: ItemKind, detail: Option<String>| Item {
            label: label.to_owned(),
            insert: insert.to_owned(),
            replace,
            kind,
            detail,
        };

    if prefix.starts_with('#') {
        let whole = Span::new(ls, offset);
        return ["## intent", "## data", "## unit ", "## api", "## tests"]
            .iter()
            .map(|h| item(h.trim_end(), h, whole, ItemKind::Section, None))
            .collect();
    }

    if first_section.is_none_or(|f| offset <= f)
        && !prefix.contains(':')
        && !prefix.starts_with(' ')
    {
        let whole = Span::new(ls, offset);
        return HEADER_KEYS
            .iter()
            .filter(|k| doc.header.get(k).is_none())
            .map(|k| item(k, &format!("{k}: "), whole, ItemKind::Key, None))
            .collect();
    }

    if let Some(i) = prefix.rfind("@ api ") {
        let partial = &prefix[i + 6..];
        if !partial.contains(' ') {
            let replace = Span::new(offset - partial.len(), offset);
            return api_members(doc)
                .into_iter()
                .map(|(label, sig)| item(&label, &label, replace, ItemKind::Member, Some(sig)))
                .collect();
        }
    }

    if let Some(i) = prefix.rfind("@ guards ") {
        let list = &prefix[i + 9..];
        if list.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '^' | ',' | ' ')
        }) {
            let partial = list.rsplit([',', ' ']).next().unwrap_or("");
            let replace = Span::new(offset - partial.len(), offset);
            return anchors(doc)
                .into_iter()
                .filter_map(|l| {
                    let a = l.ann.anchor.as_ref()?;
                    let label = format!("^{}", a.value);
                    Some(item(
                        &label,
                        &label,
                        replace,
                        ItemKind::Anchor,
                        Some(l.text.clone()),
                    ))
                })
                .collect();
        }
    }

    if section_at(doc, offset) == Some(SectionKind::Data)
        && let Some(partial) = prefix.get(2..)
        && !partial.is_empty()
        && partial.chars().all(|c| c.is_ascii_lowercase())
    {
        let replace = Span::new(offset - partial.len(), offset);
        return EcsRole::ALL
            .iter()
            .map(|r| r.keyword())
            .chain(["struct", "enum"])
            .filter(|k| k.starts_with(partial))
            .map(|k| item(k, &format!("{k} "), replace, ItemKind::Keyword, None))
            .collect();
    }

    if section_at(doc, offset) == Some(SectionKind::Unit) && prefix.len() >= 2 {
        let content = prefix[2..].trim_start_matches(' ');
        let indent = prefix.len() - 2 - content.len();
        if indent.is_multiple_of(2)
            && let Some(partial) = content.strip_prefix("- ")
            && partial.chars().all(|c| c.is_ascii_lowercase() || c == ' ')
            && STEP_KEYWORDS.iter().any(|k| k.starts_with(partial))
        {
            let replace = Span::new(offset - partial.len(), offset);
            return STEP_KEYWORDS
                .iter()
                .filter(|k| k.starts_with(partial))
                .map(|k| item(k.trim_end(), k, replace, ItemKind::Keyword, None))
                .collect();
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_parser::parse;

    const FIXTURE: &str = include_str!("../../mml-parser/tests/fixtures/delta-wave-bounties.hmml");

    fn at(needle: &str, extra: usize) -> usize {
        FIXTURE.find(needle).expect("needle in fixture") + extra
    }

    fn line_of(span: Span) -> &'static str {
        let s = line_start(FIXTURE, span.start);
        FIXTURE[s..].lines().next().unwrap()
    }

    #[test]
    fn goto_api_ref_lands_on_the_fn_line() {
        let doc = parse(FIXTURE);
        let off = at("@ api WaveTable::bounty", 8);
        let Some(Definition::Local(span)) = definition(&doc, FIXTURE, off) else {
            panic!("no definition");
        };
        assert!(line_of(span).contains("fn bounty(wave: u32)"));
    }

    #[test]
    fn goto_type_qualified_api_ref() {
        let doc = parse(FIXTURE);
        let off = at("@ api Purse::spend", 8);
        let Some(Definition::Local(span)) = definition(&doc, FIXTURE, off) else {
            panic!("no definition");
        };
        assert!(line_of(span).contains("fn Purse::spend("));
    }

    #[test]
    fn goto_guard_lands_on_the_anchored_line() {
        let doc = parse(FIXTURE);
        let off = at("@ guards ^wave_bonus", 12);
        let Some(Definition::Local(span)) = definition(&doc, FIXTURE, off) else {
            panic!("no definition");
        };
        assert!(line_of(span).contains("^wave_bonus @ waves.rs:92"));
    }

    #[test]
    fn goto_type_name_in_prose() {
        let doc = parse(FIXTURE);
        let off = at("each Enemy a", 6);
        let Some(Definition::Local(span)) = definition(&doc, FIXTURE, off) else {
            panic!("no definition");
        };
        assert_eq!(&FIXTURE[span.start..span.end], "Enemy");
        assert!(line_of(span).starts_with("  component struct Enemy"));
    }

    #[test]
    fn goto_observer_event_in_unit_header() {
        let doc = parse(FIXTURE);
        let off = at("on EnemyDied —", 4);
        let Some(Definition::Local(span)) = definition(&doc, FIXTURE, off) else {
            panic!("no definition");
        };
        assert!(line_of(span).starts_with("+ event struct EnemyDied"));
    }

    #[test]
    fn goto_module_name_in_prose() {
        let doc = parse(FIXTURE);
        let off = at("check the Score covers", 11);
        let Some(Definition::Local(span)) = definition(&doc, FIXTURE, off) else {
            panic!("no definition");
        };
        assert!(line_of(span).starts_with("  Score — "));
    }

    #[test]
    fn goto_diff_link_reports_the_side() {
        let doc = parse(FIXTURE);
        let off = at("@ waves.rs:96", 4);
        match definition(&doc, FIXTURE, off) {
            Some(Definition::Link(l, side)) => {
                assert_eq!(l.file.value, "waves.rs");
                assert_eq!(l.line.map(|n| n.value), Some(96));
                assert_eq!(side, Side::Base);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn hover_api_ref_shows_signature() {
        let doc = parse(FIXTURE);
        let h = hover(&doc, FIXTURE, at("@ api Purse::credit, WaveTable", 8)).unwrap();
        assert!(h.contains("credit(coins: u32)"), "{h}");
        assert!(h.contains("Purse — the coins the player holds this run"));
    }

    #[test]
    fn hover_tilde_line_shows_both_sentences() {
        let doc = parse(FIXTURE);
        let h = hover(&doc, FIXTURE, at("~   - check the Score", 8)).unwrap();
        assert!(
            h.contains("**before:** check the Score covers the price"),
            "{h}"
        );
        assert!(
            h.contains("**after:** check the Purse covers the price"),
            "{h}"
        );
    }

    #[test]
    fn hover_type_shows_its_block_with_its_role() {
        let doc = parse(FIXTURE);
        let h = hover(&doc, FIXTURE, at("no Enemy is", 4)).unwrap();
        assert!(h.contains("component struct Enemy {"), "{h}");
        assert!(h.contains("bounty: u32,"), "{h}");
    }

    #[test]
    fn references_anchor_and_guards() {
        let doc = parse(FIXTURE);
        let refs = references(&doc, FIXTURE, at("^paid_once\n", 3), true);
        assert_eq!(refs.len(), 2);
        let only_uses = references(&doc, FIXTURE, at("^paid_once\n", 3), false);
        assert_eq!(only_uses.len(), 1);
        assert!(line_of(only_uses[0]).starts_with("+ tests 2"));
    }

    #[test]
    fn references_fn_and_linking_steps() {
        let doc = parse(FIXTURE);
        let decl = doc.resolve_api("Purse::spend").unwrap().1.name.span;
        let refs = references(&doc, FIXTURE, decl.start + 1, true);
        assert_eq!(refs.len(), 2);
        assert!(line_of(refs[1]).contains("the Purse covers the price"));
    }

    #[test]
    fn references_type_name() {
        let doc = parse(FIXTURE);
        let refs = references(&doc, FIXTURE, at("struct Enemy {", 8), true);
        assert!(refs.len() >= 3, "{refs:?}");
    }

    #[test]
    fn symbols_cover_every_section() {
        let doc = parse(FIXTURE);
        let syms = symbols(&doc);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.first(), Some(&"intent"));
        assert!(names.contains(&"Enemy"));
        assert!(names.contains(&"unit pay-bounty"));
        assert!(names.contains(&"WaveTable"));
        assert_eq!(names.last(), Some(&"tests"));
        let enemy = syms.iter().find(|s| s.name == "Enemy").unwrap();
        assert_eq!(enemy.detail.as_deref(), Some("component"));
        let unit = syms.iter().find(|s| s.name == "unit pay-bounty").unwrap();
        assert!(
            unit.detail
                .as_deref()
                .is_some_and(|d| d.starts_with("on EnemyDied — "))
        );
        let anchors: Vec<&str> = unit.children.iter().map(|c| c.name.as_str()).collect();
        assert!(anchors.contains(&"^coin_burst"));
        assert!(anchors.contains(&"^paid_once"));
    }

    fn complete(text: &str, cursor: &str) -> Vec<String> {
        let doc = parse(text);
        let off = text.find(cursor).unwrap() + cursor.len();
        completions(&doc, text, off)
            .into_iter()
            .map(|i| i.label)
            .collect()
    }

    #[test]
    fn complete_api_members() {
        let text = FIXTURE.replace("@ api WaveTable::bounty", "@ api Wa");
        let labels = complete(&text, "@ api Wa");
        assert!(labels.contains(&"WaveTable::bonus".to_owned()));
        assert!(labels.contains(&"Purse::spend".to_owned()));
    }

    #[test]
    fn complete_anchors_after_guards() {
        let text = FIXTURE.replace("@ guards ^wave_bonus", "@ guards ^wa");
        let labels = complete(&text, "@ guards ^wa");
        assert!(labels.contains(&"^wave_bonus".to_owned()));
        assert!(labels.contains(&"^paid_once".to_owned()));
    }

    #[test]
    fn complete_step_keywords() {
        let text = FIXTURE.replace("  - refresh the button labels", "  - o");
        let labels = complete(&text, "\n  - o");
        assert_eq!(labels, vec!["on error".to_owned()]);
    }

    #[test]
    fn complete_data_roles() {
        let text = FIXTURE.replace("+ resource struct Purse", "+ res");
        assert_eq!(complete(&text, "\n+ res"), vec!["resource".to_owned()]);
        let text = FIXTURE.replace("+ state enum Shop", "+ s");
        assert_eq!(
            complete(&text, "\n+ s"),
            vec!["state".to_owned(), "struct".to_owned()]
        );
    }

    #[test]
    fn complete_section_headers_and_header_keys() {
        let labels = complete(FIXTURE, "\n#");
        assert!(labels.contains(&"## unit".to_owned()));
        let text = FIXTURE.replace(
            "pr:     https://github.com/example/bevy_survivor/pull/42",
            "p",
        );
        let doc = parse(&text);
        let off = text.find("\np\n").unwrap() + 2;
        let labels: Vec<String> = completions(&doc, &text, off)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert_eq!(labels, vec!["pr".to_owned()]);
    }

    #[test]
    fn word_and_occurrences() {
        assert_eq!(word_at("a Purse b", 4).unwrap().1, "Purse");
        assert_eq!(occurrences("Purse Purses Purse::spend", "Purse").len(), 2);
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[test]
    fn goto_diff_link_opens_the_file_at_its_side() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = home.join("fun_worktrees/demo/t/task");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        git(&repo, &["init", "-q"]);
        std::fs::write(repo.join("src/run.rs"), "base 1\nbase 2\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "base"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        std::fs::write(repo.join("src/run.rs"), "head 1\nhead 2\nhead 3\n").unwrap();
        git(&repo, &["commit", "-q", "-am", "head"]);
        let head = git(&repo, &["rev-parse", "HEAD"]);

        let notes = tmp.path().join("notes/demo/t/task");
        std::fs::create_dir_all(&notes).unwrap();
        let delta = notes.join("delta-x.hmml");
        let src = format!(
            "repo: demo\nbranch: x\nbase: {base}\nhead: {head}\n\n## intent\n  - x\n\n## unit run — r\n+ - new @ run.rs:3\n- - old @ run.rs:2\n"
        );
        let doc = parse(&src);
        let cache_dir = tmp.path().join("cache");
        let mut cache = GitCache::new();
        let resolve = |cache: &mut GitCache, needle: &str| {
            let Some(Definition::Link(link, side)) =
                definition(&doc, &src, src.find(needle).unwrap() + 3)
            else {
                panic!("no link at {needle}");
            };
            link_target(cache, &doc, &delta, &home, &cache_dir, &link, side).unwrap()
        };

        let (file, line) = resolve(&mut cache, "@ run.rs:3");
        assert_eq!(file, repo.join("src/run.rs"));
        assert_eq!(line, 2);

        let (file, line) = resolve(&mut cache, "@ run.rs:2");
        assert_eq!(file, cache_dir.join(&base).join("src/run.rs"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "base 1\nbase 2\n");
        assert_eq!(line, 1);

        git(&repo, &["checkout", "-q", &base]);
        let (file, _) = resolve(&mut cache, "@ run.rs:3");
        assert_eq!(file, cache_dir.join(&head).join("src/run.rs"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "head 1\nhead 2\nhead 3\n"
        );
    }
}
