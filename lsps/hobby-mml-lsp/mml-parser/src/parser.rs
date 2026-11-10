//! Text → [`Document`], recovering from every malformed line.
//!
//! The parser runs line by line: header `key: value` lines until the first
//! `##`, then section bodies. Each body line is split into gutter, content
//! depth, bullet, `~` versions and trailing annotations (grammar §3, §4), and
//! placed in its section's indentation tree. When a section ends, its lines
//! are given their roles and the section's structures are built.

use std::collections::HashSet;

use crate::ast::*;
use crate::diagnostic::{Diagnostic, code};
use crate::span::{Span, Spanned};

pub const HEADER_KEYS: [&str; 5] = ["repo", "branch", "base", "head", "pr"];
pub const REQUIRED_HEADER_KEYS: [&str; 4] = ["repo", "branch", "base", "head"];
pub const ARROW: &str = " → ";

pub fn parse(src: &str) -> Document {
    let mut p = Parser {
        doc: Document::default(),
        diags: Vec::new(),
        max_rank: None,
    };
    p.run(src);
    p.doc.diagnostics = p.diags;
    p.doc
}

struct Parser {
    doc: Document,
    diags: Vec<Diagnostic>,
    max_rank: Option<u8>,
}

struct Open {
    kind: Option<SectionKind>,
    header: Span,
    end: usize,
    lines: Vec<usize>,
    unit: Option<UnitHead>,
}

struct UnitHead {
    name: Spanned<String>,
    status: UnitStatus,
    trigger: Option<Trigger>,
    what: String,
    link: Option<DiffLink>,
}

fn split_lines(src: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for part in src.split_inclusive('\n') {
        let text = part.strip_suffix('\n').unwrap_or(part);
        let text = text.strip_suffix('\r').unwrap_or(text);
        out.push((start, text));
        start += part.len();
    }
    out
}

pub fn is_comment(text: &str) -> bool {
    text == "#" || text.starts_with("# ")
}

impl Parser {
    fn err(&mut self, span: Span, code: &'static str, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(span, code, msg));
    }

    fn run(&mut self, src: &str) {
        let mut open: Option<Open> = None;
        let mut in_header = true;
        let mut seen: HashSet<String> = HashSet::new();
        for (start, text) in split_lines(src) {
            if text.trim().is_empty() || is_comment(text) {
                continue;
            }
            let span = Span::new(start, start + text.len());
            if text.starts_with("##") {
                if let Some(o) = open.take() {
                    self.finish(o);
                }
                in_header = false;
                open = Some(self.section_header(start, text));
                continue;
            }
            if in_header {
                self.header_line(start, text, &mut seen);
                continue;
            }
            let Some(o) = open.as_mut() else { continue };
            if o.kind.is_none() {
                continue;
            }
            let kind = o.kind.unwrap();
            if let Some(idx) = self.body_line(start, text, kind, &o.lines) {
                o.lines.push(idx);
            }
            o.end = span.end;
        }
        if let Some(o) = open.take() {
            self.finish(o);
        }
        self.header_checks(&seen);
        self.file_checks();
    }

    fn header_line(&mut self, start: usize, text: &str, seen: &mut HashSet<String>) {
        let span = Span::new(start, start + text.len());
        let Some((key, value)) = text.split_once(':') else {
            self.err(
                span,
                code::UNKNOWN_HEADER_KEY,
                "not a `key: value` header line",
            );
            return;
        };
        let key_span = Span::new(start, start + key.len());
        if !HEADER_KEYS.contains(&key) {
            self.err(
                key_span,
                code::UNKNOWN_HEADER_KEY,
                format!(
                    "unknown header key `{}`; expected one of {}",
                    key.trim(),
                    HEADER_KEYS.join(", ")
                ),
            );
            return;
        }
        let lead = value.len() - value.trim_start().len();
        let vstart = start + key.len() + 1 + lead;
        let v = value.trim();
        let vspan = Span::new(vstart, vstart + v.len());
        if !seen.insert(key.to_owned()) {
            self.err(
                key_span,
                code::BAD_HEADER_VALUE,
                format!("`{key}` is given twice"),
            );
            return;
        }
        if v.is_empty() {
            self.err(
                span,
                code::BAD_HEADER_VALUE,
                format!("`{key}` has no value"),
            );
        } else if v.contains(char::is_whitespace) {
            self.err(
                vspan,
                code::BAD_HEADER_VALUE,
                format!("`{key}` takes a single token"),
            );
        } else if (key == "base" || key == "head") && !is_commit_id(v) {
            self.err(
                vspan,
                code::BAD_HEADER_VALUE,
                format!("`{key}` must be a commit id of 7–40 hex digits"),
            );
        }
        self.doc.header.entries.push(HeaderEntry {
            key: Spanned::new(key.to_owned(), key_span),
            value: Spanned::new(v.to_owned(), vspan),
            span,
        });
    }

    fn header_checks(&mut self, seen: &HashSet<String>) {
        let span = match (
            self.doc.header.entries.first(),
            self.doc.header.entries.last(),
        ) {
            (Some(a), Some(b)) => Span::new(a.span.start, b.span.end),
            _ => Span::at(0),
        };
        for key in REQUIRED_HEADER_KEYS {
            if !seen.contains(key) {
                self.err(
                    span,
                    code::MISSING_HEADER_KEY,
                    format!("missing required header key `{key}`"),
                );
            }
        }
    }

    fn file_checks(&mut self) {
        let first = self
            .doc
            .sections
            .first()
            .map(|s| s.header)
            .unwrap_or(Span::at(0));
        match self
            .doc
            .sections
            .iter()
            .find(|s| s.kind == SectionKind::Intent)
        {
            None => self.err(first, code::MISSING_INTENT, "no `## intent` section"),
            Some(s) if self.doc.intent.is_empty() => {
                let h = s.header;
                self.err(h, code::MISSING_INTENT, "`## intent` holds no bullet")
            }
            _ => {}
        }
        if self.doc.units.is_empty() {
            self.err(first, code::NO_UNIT, "no `## unit` section");
        }
    }

    fn section_header(&mut self, start: usize, text: &str) -> Open {
        let span = Span::new(start, start + text.len());
        let mut open = Open {
            kind: None,
            header: span,
            end: span.end,
            lines: Vec::new(),
            unit: None,
        };
        let rest = &text[2..];
        let word = rest.split_whitespace().next().unwrap_or("");
        let kind = if !rest.starts_with(' ') {
            None
        } else {
            match word {
                "intent" => Some(SectionKind::Intent),
                "data" => Some(SectionKind::Data),
                "unit" => Some(SectionKind::Unit),
                "api" => Some(SectionKind::Api),
                "tests" => Some(SectionKind::Tests),
                _ => None,
            }
        };
        let kind = match kind {
            Some(k) if k != SectionKind::Unit && rest.trim() != word => None,
            k => k,
        };
        let Some(kind) = kind else {
            self.err(
                span,
                code::UNKNOWN_SECTION,
                format!(
                    "unknown section `{}`; expected intent, data, unit, api or tests",
                    text.trim()
                ),
            );
            return open;
        };
        let rank = kind.rank();
        match self.max_rank {
            Some(m) if rank < m || (rank == m && kind != SectionKind::Unit) => {
                let msg = if rank == m {
                    format!("`## {}` appears twice", kind.title())
                } else {
                    format!(
                        "`## {}` is out of order; sections go intent, data, unit, api, tests",
                        kind.title()
                    )
                };
                self.err(span, code::SECTION_ORDER, msg);
            }
            _ => self.max_rank = Some(rank),
        }
        open.kind = Some(kind);
        if kind == SectionKind::Unit {
            open.unit = Some(self.unit_header(start, text));
        }
        open
    }

    fn unit_header(&mut self, start: usize, text: &str) -> UnitHead {
        let span = Span::new(start, start + text.len());
        let after = "## unit".len();
        let rest = &text[after..];
        let lead = rest.len() - rest.trim_start().len();
        let body = rest.trim();
        let base = start + after + lead;
        let (body_len, ann) = self.annotations(body, base);
        if ann.anchor.is_some() || !ann.api.is_empty() || ann.guards.is_some() {
            self.err(
                ann.span.unwrap_or(span),
                code::BAD_ANNOTATION,
                "a unit header takes only a diff link",
            );
        }
        let body = body[..body_len].trim_end();
        let mut head = UnitHead {
            name: Spanned::new(String::new(), span),
            status: UnitStatus::Present,
            trigger: None,
            what: String::new(),
            link: ann.link,
        };
        let (left, what) = match body.split_once(" — ") {
            Some((l, w)) => (l, Some(w.trim())),
            None => (body, None),
        };
        let mut toks = left.split_whitespace();
        let name = toks.next().unwrap_or("");
        if !name.is_empty() {
            let off = left.find(name).unwrap_or(0);
            head.name = Spanned::new(
                name.to_owned(),
                Span::new(base + off, base + off + name.len()),
            );
        }
        let mut next = toks.next();
        head.status = match next {
            Some("(new)") => {
                next = toks.next();
                UnitStatus::New
            }
            Some("(removed)") => {
                next = toks.next();
                UnitStatus::Removed
            }
            _ => UnitStatus::Present,
        };
        match next {
            None => {}
            Some(kw @ ("in" | "on")) => match toks.next() {
                Some(target) => {
                    let off = left.rfind(target).unwrap_or(0);
                    let t = Spanned::new(
                        target.to_owned(),
                        Span::new(base + off, base + off + target.len()),
                    );
                    if kw == "in" && !is_schedule(target) {
                        self.err(
                            t.span,
                            code::BAD_UNIT_HEADER,
                            format!("`{target}` is not a schedule, such as `Update` or `OnEnter(Screen::Title)`"),
                        );
                    } else if kw == "on" && !(is_pascal(target.split('<').next().unwrap_or("")) && valid_type(target)) {
                        self.err(
                            t.span,
                            code::BAD_UNIT_HEADER,
                            format!("`{target}` is not an event type, such as `WaveCleared` or `Add<Enemy>`"),
                        );
                    }
                    head.trigger = Some(if kw == "in" {
                        Trigger::Schedule(t)
                    } else {
                        Trigger::Observer(t)
                    });
                    if toks.next().is_some() {
                        self.err(
                            span,
                            code::BAD_UNIT_HEADER,
                            "a unit header is `<name> [(new)|(removed)] [in <Schedule> | on <Event>] — <what>`",
                        );
                    }
                }
                None => self.err(
                    span,
                    code::BAD_UNIT_HEADER,
                    format!("`{kw}` needs a schedule or event after it"),
                ),
            },
            Some(other) => self.err(
                span,
                code::BAD_UNIT_HEADER,
                format!(
                    "`{other}` after the unit name; only `(new)`, `(removed)`, `in <Schedule>` or `on <Event>` go there"
                ),
            ),
        }
        if name.is_empty() {
            self.err(span, code::BAD_UNIT_HEADER, "`## unit` without a name");
        } else if !is_kebab(name) {
            self.err(
                head.name.span,
                code::BAD_UNIT_HEADER,
                format!("unit name `{name}` is not kebab-case"),
            );
        }
        match what {
            None => self.err(
                span,
                code::BAD_UNIT_HEADER,
                "`## unit <name> — <what it is>` needs ` — ` and a description",
            ),
            Some("") => self.err(
                span,
                code::BAD_UNIT_HEADER,
                "the unit header says nothing after ` — `",
            ),
            Some(w) => head.what = w.to_owned(),
        }
        head
    }

    fn annotations(&mut self, content: &str, base: usize) -> (usize, Annotations) {
        let bytes = content.as_bytes();
        let candidates: Vec<usize> = content
            .char_indices()
            .filter(|&(i, c)| {
                (i == 0 || bytes[i - 1] == b' ')
                    && ((c == '^' && bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase))
                        || (c == '@' && (i + 1 == bytes.len() || bytes.get(i + 1) == Some(&b' '))))
            })
            .map(|(i, _)| i)
            .collect();
        for &i in &candidates {
            if let Some(toks) = parse_tail(&content[i..], base + i) {
                return (
                    i,
                    self.order_annotations(toks, base + i, base + content.len()),
                );
            }
        }
        if let Some(&i) = candidates.iter().find(|&&i| bytes[i] == b'@') {
            self.err(
                Span::new(base + i, base + content.len()),
                code::BAD_ANNOTATION,
                "malformed annotation; expected `^anchor`, `@ <file>[:<line>]`, `@ api Module::fn` or `@ guards ^a, ^b`",
            );
            return (i, Annotations::default());
        }
        (content.len(), Annotations::default())
    }

    fn order_annotations(&mut self, toks: Vec<Tok>, start: usize, end: usize) -> Annotations {
        let mut ann = Annotations {
            span: Some(Span::new(start, end)),
            ..Annotations::default()
        };
        let mut last: Option<u8> = None;
        for tok in toks {
            let (order, span, what) = match &tok {
                Tok::Anchor(a) => (0, a.span, "`^anchor`"),
                Tok::Link(l) => (1, l.span, "diff link"),
                Tok::Api(_, s) => (2, *s, "`@ api`"),
                Tok::Guards(_, s) => (3, *s, "`@ guards`"),
            };
            let taken = match &tok {
                Tok::Anchor(_) => ann.anchor.is_some(),
                Tok::Link(_) => ann.link.is_some(),
                Tok::Api(..) => !ann.api.is_empty(),
                Tok::Guards(..) => ann.guards.is_some(),
            };
            if taken {
                self.err(
                    span,
                    code::BAD_ANNOTATION,
                    format!("{what} given twice on one line"),
                );
                continue;
            }
            if last.is_some_and(|l| order < l) {
                self.err(
                    span,
                    code::BAD_ANNOTATION,
                    format!("{what} out of order; annotations go `^anchor`, `@ <file>`, `@ api`, `@ guards`"),
                );
            }
            last = Some(last.map_or(order, |l| l.max(order)));
            match tok {
                Tok::Anchor(a) => ann.anchor = Some(a),
                Tok::Link(l) => ann.link = Some(l),
                Tok::Api(a, _) => ann.api = a,
                Tok::Guards(g, _) => ann.guards = Some(g),
            }
        }
        ann
    }

    fn body_line(
        &mut self,
        start: usize,
        text: &str,
        section: SectionKind,
        siblings: &[usize],
    ) -> Option<usize> {
        let c0 = text.chars().next()?;
        let Some(gutter) = Gutter::from_char(c0) else {
            self.err(
                Span::new(start, start + c0.len_utf8()),
                code::BAD_GUTTER,
                format!("`{c0}` in the gutter; column 0 is one of ` `, `+`, `-`, `~`"),
            );
            return None;
        };
        match text[1..].chars().next() {
            Some(' ') => {}
            None => {
                self.err(
                    Span::new(start, start + 1),
                    code::BAD_GUTTER,
                    "a gutter with no content",
                );
                return None;
            }
            Some(c) => {
                self.err(
                    Span::new(start + 1, start + 1 + c.len_utf8()),
                    code::BAD_GUTTER,
                    "column 1 after the gutter must be a space",
                );
                return None;
            }
        }
        let rest = &text[2..];
        let indent = rest.len() - rest.trim_start_matches(' ').len();
        let content = rest[indent..].trim_end();
        if content.is_empty() {
            if gutter != Gutter::Context {
                self.err(
                    Span::new(start, start + 1),
                    code::BAD_GUTTER,
                    "a gutter with no content",
                );
            }
            return None;
        }
        let content_start = start + 2 + indent;
        if content.starts_with('\t') {
            self.err(
                Span::new(start + 2, content_start + 1),
                code::BAD_INDENT,
                "content is indented with spaces, not tabs",
            );
        }
        if !indent.is_multiple_of(2) {
            self.err(
                Span::new(start + 2, content_start),
                code::BAD_INDENT,
                "content indents in steps of 2 spaces",
            );
        }
        let depth = indent / 2;

        let (body_len, ann) = self.annotations(content, content_start);
        let body = content[..body_len].trim_end();
        let (bullet, body, body_start) = if body == "-" {
            (true, "", content_start + 1)
        } else if let Some(r) = body.strip_prefix("- ") {
            let lead = r.len() - r.trim_start().len();
            (true, r.trim_start(), content_start + 2 + lead)
        } else {
            (false, body, content_start)
        };

        let mut tilde = None;
        let mut line_text = body.to_owned();
        if gutter == Gutter::Changed {
            let arrows: Vec<usize> = body.match_indices(ARROW).map(|(i, _)| i).collect();
            let span = Span::new(body_start, body_start + body.len());
            match arrows.as_slice() {
                [a] if *a > 0 && a + ARROW.len() < body.len() => {
                    let old = &body[..*a];
                    let new = &body[a + ARROW.len()..];
                    let (prefix_len, full_new) = match new.strip_prefix("… ") {
                        Some(added) => (old.len(), format!("{old} {added}")),
                        None => {
                            let cut = shared_prefix(old, new);
                            (cut, format!("{}{}", &old[..cut], new))
                        }
                    };
                    let t = Tilde {
                        old: old.to_owned(),
                        new: full_new,
                        prefix_len,
                        old_clause: Span::new(body_start + prefix_len, body_start + a),
                        new_clause: Span::new(body_start + a + ARROW.len(), body_start + body.len()),
                        arrow: Span::new(body_start + a, body_start + a + ARROW.len()),
                    };
                    line_text = t.new.clone();
                    tilde = Some(t);
                }
                [] => self.err(span, code::BAD_TILDE, "a `~` line holds `<old> → <new>`; no ` → ` found"),
                [_] => self.err(span, code::BAD_TILDE, "a `~` line needs wording on both sides of ` → `"),
                _ => self.err(
                    span,
                    code::BAD_TILDE,
                    "a `~` line holds exactly one ` → `; write a `-` and a `+` line when more than one clause changes",
                ),
            }
        }

        let line_span = Span::new(start, start + text.trim_end().len());
        let max_depth = siblings.last().map_or(0, |&s| self.doc.lines[s].depth + 1);
        let parent = siblings
            .iter()
            .rev()
            .copied()
            .find(|&s| self.doc.lines[s].depth < depth);
        if depth > max_depth {
            self.err(
                Span::new(start + 2, content_start),
                code::BAD_INDENT,
                "content is indented more than one step past the line above",
            );
        }
        if let Some(p) = parent {
            let pg = self.doc.lines[p].gutter;
            if (pg == Gutter::Removed || pg == Gutter::Added) && gutter != pg {
                let what = if pg == Gutter::Removed {
                    "removed"
                } else {
                    "added"
                };
                self.err(
                    Span::new(start, start + 1),
                    code::NESTED_CHANGE_MISMATCH,
                    format!(
                        "nested under a {what} `{}` line, so this line is `{}` too",
                        pg.as_char(),
                        pg.as_char()
                    ),
                );
            }
        }

        self.doc.lines.push(Line {
            span: line_span,
            gutter,
            depth,
            section,
            role: Role::Other,
            bullet,
            text: line_text,
            text_start: body_start,
            tilde,
            ann,
            parent,
        });
        Some(self.doc.lines.len() - 1)
    }

    fn finish(&mut self, o: Open) {
        let Some(kind) = o.kind else { return };
        let span = Span::new(o.header.start, o.end);
        self.doc.sections.push(Section {
            kind,
            header: o.header,
            span,
        });
        match kind {
            SectionKind::Intent => self.intent(&o.lines),
            SectionKind::Data => self.data(&o.lines),
            SectionKind::Unit => self.unit(&o, span),
            SectionKind::Api => self.api(&o.lines),
            SectionKind::Tests => self.tests(&o.lines),
        }
        for &li in &o.lines {
            self.annotation_roles(li);
        }
    }

    fn annotation_roles(&mut self, li: usize) {
        let line = &self.doc.lines[li];
        let role = line.role;
        let mut bad = Vec::new();
        if let Some(a) = &line.ann.anchor
            && !matches!(role, Role::Step | Role::Invariant)
        {
            bad.push((a.span, "`^anchor` goes on unit steps and invariants only"));
        }
        if let Some(a) = line.ann.api.first()
            && role != Role::Step
        {
            bad.push((a.span, "`@ api` goes on unit steps only"));
        }
        if line.ann.guards.is_some() && role != Role::Test {
            let s = line.ann.span.unwrap_or(line.span);
            bad.push((s, "`@ guards` goes on test lines only"));
        }
        for (s, m) in bad {
            self.err(s, code::BAD_ANNOTATION, m);
        }
    }

    fn intent(&mut self, lines: &[usize]) {
        for &li in lines {
            let l = &mut self.doc.lines[li];
            l.role = Role::Intent;
            let gutter = l.gutter;
            let span = Span::new(l.span.start, l.span.start + 1);
            if l.bullet && !l.text.is_empty() {
                self.doc.intent.push(li);
            }
            if gutter != Gutter::Context {
                self.err(
                    span,
                    code::CHANGE_IN_INTENT,
                    "`## intent` lines are context only; no `+`, `-` or `~`",
                );
            }
        }
    }

    fn unit(&mut self, o: &Open, span: Span) {
        let head = o.unit.as_ref().expect("unit header parsed");
        let mut unit = Unit {
            name: head.name.clone(),
            status: head.status,
            trigger: head.trigger.clone(),
            what: head.what.clone(),
            link: head.link.clone(),
            header: o.header,
            span,
            steps: Vec::new(),
            invariants: Vec::new(),
        };
        let mut label: Option<usize> = None;
        for &li in &o.lines {
            let (bullet, depth, is_label, lspan) = {
                let l = &self.doc.lines[li];
                (
                    l.bullet,
                    l.depth,
                    !l.bullet && l.tilde.is_none() && l.text == "invariants:",
                    l.span,
                )
            };
            if is_label {
                self.doc.lines[li].role = Role::InvariantsLabel;
                if depth != 0 {
                    self.err(
                        lspan,
                        code::MISPLACED_INVARIANTS,
                        "`invariants:` sits at content depth 0",
                    );
                }
                if label.is_some() {
                    self.err(
                        lspan,
                        code::MISPLACED_INVARIANTS,
                        "a unit holds one `invariants:` block",
                    );
                }
                label = Some(li);
                continue;
            }
            if label.is_some() && depth == 0 && bullet {
                self.doc.lines[li].role = Role::Invariant;
                unit.invariants.push(li);
                continue;
            }
            self.doc.lines[li].role = Role::Step;
            unit.steps.push(li);
            if label.is_some() {
                self.err(
                    lspan,
                    code::MISPLACED_INVARIANTS,
                    "a step after `invariants:`; invariants are flat bullets after the last step",
                );
            }
        }
        if let Some(lab) = label {
            let lg = self.doc.lines[lab].gutter;
            let first = unit.invariants.first().map(|&i| self.doc.lines[i].gutter);
            if lg != Gutter::Context && first != Some(lg) {
                let s = self.doc.lines[lab].span;
                self.err(
                    Span::new(s.start, s.start + 1),
                    code::BAD_GUTTER,
                    "`invariants:` takes the gutter of its first bullet, or a space",
                );
            }
        }
        let want = match unit.status {
            UnitStatus::New => Some(Gutter::Added),
            UnitStatus::Removed => Some(Gutter::Removed),
            UnitStatus::Present => None,
        };
        if let Some(g) = want {
            for &li in &o.lines {
                let l = &self.doc.lines[li];
                if l.gutter != g {
                    let s = Span::new(l.span.start, l.span.start + 1);
                    let what = if g == Gutter::Added {
                        "(new)"
                    } else {
                        "(removed)"
                    };
                    self.err(
                        s,
                        code::NESTED_CHANGE_MISMATCH,
                        format!("every line of a {what} unit is `{}`", g.as_char()),
                    );
                }
            }
        }
        self.doc.units.push(unit);
    }

    fn data(&mut self, lines: &[usize]) {
        struct Block {
            def: usize,
            depth: usize,
            gutter: Gutter,
            kind: TypeKind,
        }
        let mut open: Option<Block> = None;
        for &li in lines {
            let (depth, gutter, text, span) = {
                let l = &self.doc.lines[li];
                (l.depth, l.gutter, l.text.clone(), l.span)
            };
            if let Some(b) = &open {
                if depth == b.depth && text == "}" {
                    self.doc.lines[li].role = Role::TypeClose;
                    if gutter != b.gutter {
                        self.err(
                            Span::new(span.start, span.start + 1),
                            code::BAD_STRUCT,
                            "the closing `}` takes the gutter of its `struct`/`enum` line",
                        );
                    }
                    self.doc.data[b.def].span.end = span.end;
                    open = None;
                    continue;
                }
                if depth == b.depth + 1 {
                    self.doc.lines[li].role = Role::Field;
                    let (def, kind) = (b.def, b.kind);
                    self.member(li, def, kind);
                    continue;
                }
                if depth > b.depth + 1 {
                    self.doc.lines[li].role = Role::Field;
                    self.err(
                        span,
                        code::BAD_STRUCT,
                        "fields sit exactly one step deeper than their type",
                    );
                    continue;
                }
                let def_span = self.doc.lines[self.doc.data[b.def].line].span;
                let name = self.doc.data[b.def].name.value.clone();
                self.err(
                    def_span,
                    code::BAD_STRUCT,
                    format!("`{name}` is not closed by `}}` at its own depth"),
                );
                open = None;
            }
            self.doc.lines[li].role = Role::TypeHeader;
            if depth != 0 {
                self.err(
                    span,
                    code::BAD_STRUCT,
                    "a `struct`/`enum` line sits at content depth 0",
                );
            }
            if let Some((def, true)) = self.type_header(li) {
                let kind = self.doc.data[def].kind;
                open = Some(Block {
                    def,
                    depth,
                    gutter,
                    kind,
                });
            }
        }
        if let Some(b) = open {
            let def_span = self.doc.lines[self.doc.data[b.def].line].span;
            let name = self.doc.data[b.def].name.value.clone();
            self.err(
                def_span,
                code::BAD_STRUCT,
                format!("`{name}` is not closed by `}}`"),
            );
        }
    }

    fn type_header(&mut self, li: usize) -> Option<(usize, bool)> {
        let line = &self.doc.lines[li];
        let text = line.text.clone();
        let span = line.span;
        let (role, role_len) = match text.split_once(' ') {
            Some((word, _)) => match EcsRole::from_keyword(word) {
                Some(r) => (Some(r), word.len() + 1),
                None => (None, 0),
            },
            None => (None, 0),
        };
        let typed = &text[role_len..];
        let (kind, kw_word) = if typed.starts_with("struct ") {
            (TypeKind::Struct, "struct ")
        } else if typed.starts_with("enum ") {
            (TypeKind::Enum, "enum ")
        } else {
            self.err(
                span,
                code::BAD_STRUCT,
                "expected `[<role>] struct Name {`, `struct Name { a: A }`, `enum Name { a, b }` or `enum Name {`, with <role> one of component, resource, event, message, state",
            );
            return None;
        };
        let kw = &text[..role_len + kw_word.len()];
        let rest = &text[kw.len()..];
        let name_len = rest.find([' ', '{']).unwrap_or(rest.len());
        let name = &rest[..name_len];
        let name_span = self.doc.lines[li].sub_span(kw.len(), name.len());
        if !is_pascal(name) {
            self.err(
                name_span,
                code::BAD_STRUCT,
                format!("type name `{name}` is not PascalCase"),
            );
            return None;
        }
        let after = rest[name_len..].trim_start();
        let after_rel = text.len() - after.len();
        let Some(inner) = after.strip_prefix('{') else {
            self.err(span, code::BAD_STRUCT, format!("`{name}` needs a `{{`"));
            return None;
        };
        let def = TypeDef {
            kind,
            role,
            name: Spanned::new(name.to_owned(), name_span),
            line: li,
            span,
            fields: Vec::new(),
        };
        self.doc.data.push(def);
        let idx = self.doc.data.len() - 1;
        let inner_t = inner.trim();
        if inner_t.is_empty() {
            return Some((idx, true));
        }
        let Some(body) = inner_t.strip_suffix('}') else {
            self.err(
                span,
                code::BAD_STRUCT,
                format!("`{name} {{ … }}` on one line must end with `}}`"),
            );
            return Some((idx, false));
        };
        let body_rel = after_rel + 1 + (inner.len() - inner.trim_start().len());
        for (off, item) in split_top(body) {
            let item_t = item.trim();
            if item_t.is_empty() {
                continue;
            }
            let rel = body_rel + off + (item.len() - item.trim_start().len());
            self.field(li, idx, kind, item_t, rel);
        }
        Some((idx, false))
    }

    fn member(&mut self, li: usize, def: usize, kind: TypeKind) {
        let text = self.doc.lines[li].text.clone();
        let item = text.strip_suffix(',').unwrap_or(&text).trim_end();
        self.field(li, def, kind, item, 0);
    }

    fn field(&mut self, li: usize, def: usize, kind: TypeKind, item: &str, rel: usize) {
        let line = &self.doc.lines[li];
        match kind {
            TypeKind::Enum => {
                if !is_ident(item) {
                    let s = line.sub_span(rel, item.len());
                    self.err(
                        s,
                        code::BAD_STRUCT,
                        format!("`{item}` is not an enum variant name"),
                    );
                    return;
                }
                let s = line.sub_span(rel, item.len());
                self.doc.data[def].fields.push(Field {
                    name: Spanned::new(item.to_owned(), s),
                    ty: None,
                    line: li,
                });
            }
            TypeKind::Struct => {
                let Some((name, ty)) = item.split_once(':') else {
                    let s = line.sub_span(rel, item.len());
                    self.err(
                        s,
                        code::BAD_STRUCT,
                        format!("`{item}` is not a `name: Type` field"),
                    );
                    return;
                };
                let name_t = name.trim();
                let ty_t = ty.trim();
                let ty_rel = rel + name.len() + 1 + (ty.len() - ty.trim_start().len());
                let name_span = line.sub_span(rel, name_t.len());
                let ty_span = line.sub_span(ty_rel, ty_t.len());
                if !is_ident(name_t) {
                    self.err(
                        name_span,
                        code::BAD_STRUCT,
                        format!("`{name_t}` is not a field name"),
                    );
                    return;
                }
                if !valid_type(ty_t) {
                    self.err(ty_span, code::BAD_STRUCT, format!("`{ty_t}` is not a type"));
                    return;
                }
                self.doc.data[def].fields.push(Field {
                    name: Spanned::new(name_t.to_owned(), name_span),
                    ty: Some(Spanned::new(ty_t.to_owned(), ty_span)),
                    line: li,
                });
            }
        }
    }

    fn api(&mut self, lines: &[usize]) {
        let mut module: Option<usize> = None;
        for &li in lines {
            let (depth, text, span) = {
                let l = &self.doc.lines[li];
                (l.depth, l.text.clone(), l.span)
            };
            if depth == 0 && !text.starts_with("fn ") {
                self.doc.lines[li].role = Role::Module;
                module = self.module_line(li, &text);
                continue;
            }
            self.doc.lines[li].role = Role::Fn;
            if depth == 0 {
                self.err(
                    span,
                    code::BAD_FN,
                    "a `fn` entry sits one step under its module",
                );
                module = None;
                continue;
            }
            if depth > 1 {
                self.err(
                    span,
                    code::BAD_FN,
                    "a `fn` entry is nested exactly one step under its module and has no body",
                );
                continue;
            }
            if !text.starts_with("fn ") {
                self.err(
                    span,
                    code::BAD_FN,
                    "expected `fn [Type::]name(param: Type, …) [-> Type]`",
                );
                continue;
            }
            let Some(m) = module else {
                self.err(span, code::BAD_FN, "a `fn` entry outside a module");
                continue;
            };
            match parse_signature(&text) {
                Some(name) => {
                    let s = self.doc.lines[li].sub_span(3, name.len());
                    self.doc.api[m].fns.push(ApiFn {
                        name: Spanned::new(name, s),
                        signature: text.clone(),
                        line: li,
                    });
                    self.doc.api[m].span.end = span.end;
                }
                None => self.err(
                    span,
                    code::BAD_FN,
                    "malformed signature; expected `fn [Type::]name(param: Type, …) [-> Type]`",
                ),
            }
        }
    }

    fn module_line(&mut self, li: usize, text: &str) -> Option<usize> {
        let span = self.doc.lines[li].span;
        let (name, resp) = match text.split_once(" — ") {
            Some((n, r)) if is_pascal(n.trim()) && !r.trim().is_empty() => (n.trim(), r.trim()),
            _ => {
                self.err(
                    span,
                    code::BAD_FN,
                    "a module line is `PascalName — responsibility`",
                );
                return None;
            }
        };
        let s = self.doc.lines[li].sub_span(0, name.len());
        self.doc.api.push(Module {
            name: Spanned::new(name.to_owned(), s),
            responsibility: resp.to_owned(),
            line: li,
            span,
            fns: Vec::new(),
        });
        Some(self.doc.api.len() - 1)
    }

    fn tests(&mut self, lines: &[usize]) {
        for &li in lines {
            self.doc.lines[li].role = Role::Test;
            let l = &self.doc.lines[li];
            let span = l.span;
            if l.depth != 0 || l.parent.is_some() {
                self.err(
                    span,
                    code::BAD_TEST,
                    "test entries are flat, at content depth 0",
                );
                continue;
            }
            if l.bullet {
                self.err(span, code::BAD_TEST, "test entries take no `- ` bullet");
                continue;
            }
            let text = l.text.clone();
            let parsed = if let Some(r) = text.strip_prefix("test ") {
                r.split_once(" — ").and_then(|(n, d)| {
                    (is_snake(n) && !d.trim().is_empty())
                        .then(|| (TestKind::Single(n.to_owned()), n.to_owned(), 5, d))
                })
            } else if let Some(r) = text.strip_prefix("tests ") {
                r.split_once(" — ").and_then(|(n, d)| {
                    let count = n.parse::<u32>().ok().filter(|&c| c > 0)?;
                    (!d.trim().is_empty()).then(|| (TestKind::Group(count), n.to_owned(), 6, d))
                })
            } else {
                None
            };
            match parsed {
                Some((kind, label, rel, desc)) => {
                    let s = self.doc.lines[li].sub_span(rel, label.len());
                    self.doc.tests.push(TestEntry {
                        kind,
                        label: Spanned::new(label, s),
                        description: desc.trim().to_owned(),
                        line: li,
                    });
                }
                None => self.err(
                    span,
                    code::BAD_TEST,
                    "expected `test <snake_name> — <description>` or `tests <N> — <description>`",
                ),
            }
        }
    }
}

enum Tok {
    Anchor(Spanned<String>),
    Link(DiffLink),
    Api(Vec<Spanned<String>>, Span),
    Guards(Vec<Spanned<String>>, Span),
}

fn word_end(s: &str, from: usize) -> usize {
    s[from..].find(' ').map_or(s.len(), |i| from + i)
}

fn skip_spaces(s: &str, from: usize) -> usize {
    from + (s[from..].len() - s[from..].trim_start_matches(' ').len())
}

fn anchor_at(s: &str, i: usize, base: usize) -> Option<(Spanned<String>, usize)> {
    if s.as_bytes().get(i) != Some(&b'^') {
        return None;
    }
    let name_end = s[i + 1..]
        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .map_or(s.len(), |e| i + 1 + e);
    let name = &s[i + 1..name_end];
    if !is_anchor_name(name) {
        return None;
    }
    Some((
        Spanned::new(name.to_owned(), Span::new(base + i, base + name_end)),
        name_end,
    ))
}

fn parse_tail(s: &str, base: usize) -> Option<Vec<Tok>> {
    let mut toks = Vec::new();
    let mut i = 0;
    loop {
        i = skip_spaces(s, i);
        if i >= s.len() {
            break;
        }
        let start = i;
        if s.as_bytes()[i] == b'^' {
            let (a, end) = anchor_at(s, i, base)?;
            if end < s.len() && s.as_bytes()[end] != b' ' {
                return None;
            }
            toks.push(Tok::Anchor(a));
            i = end;
            continue;
        }
        if !(s[i..].starts_with("@ ")) {
            return None;
        }
        let ws = skip_spaces(s, i + 1);
        let we = word_end(s, ws);
        let word = &s[ws..we];
        match word {
            "api" => {
                let mut refs = Vec::new();
                let mut j = we;
                loop {
                    let ps = skip_spaces(s, j);
                    let pe = s[ps..].find([' ', ',']).map_or(s.len(), |e| ps + e);
                    let path = &s[ps..pe];
                    let segs: Vec<&str> = path.split("::").collect();
                    if segs.len() < 2 || !segs.iter().all(|seg| is_ident(seg)) {
                        return None;
                    }
                    refs.push(Spanned::new(
                        path.to_owned(),
                        Span::new(base + ps, base + pe),
                    ));
                    j = pe;
                    if s[j..].starts_with(',') {
                        j += 1;
                        continue;
                    }
                    break;
                }
                toks.push(Tok::Api(refs, Span::new(base + start, base + j)));
                i = j;
            }
            "guards" => {
                let mut list = Vec::new();
                let mut j = we;
                loop {
                    j = skip_spaces(s, j);
                    let (a, end) = anchor_at(s, j, base)?;
                    list.push(a);
                    j = end;
                    if s[j..].starts_with(',') {
                        j += 1;
                        continue;
                    }
                    break;
                }
                if j < s.len() && s.as_bytes()[j] != b' ' {
                    return None;
                }
                toks.push(Tok::Guards(list, Span::new(base + start, base + j)));
                i = j;
            }
            "" => return None,
            w => {
                let (file, line) = match w.rsplit_once(':') {
                    Some((f, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                        (f, Some(n))
                    }
                    _ => (w, None),
                };
                if file.is_empty() || file.contains(':') || file.starts_with('^') {
                    return None;
                }
                let fs = base + ws;
                let link = DiffLink {
                    file: Spanned::new(file.to_owned(), Span::new(fs, fs + file.len())),
                    line: match line {
                        Some(n) => {
                            let ls = fs + file.len() + 1;
                            Some(Spanned::new(n.parse().ok()?, Span::new(ls, ls + n.len())))
                        }
                        None => None,
                    },
                    span: Span::new(base + start, base + we),
                };
                toks.push(Tok::Link(link));
                i = we;
            }
        }
    }
    (!toks.is_empty()).then_some(toks)
}

/// How many leading bytes of `old` both versions of a `~` line share.
///
/// The cut is where the leading words of `new` match the most consecutive
/// words of `old`; without any match, it falls after the last clause
/// separator (` — `, `; `, `: `, `, `) in `old`, else at its start.
pub fn shared_prefix(old: &str, new: &str) -> usize {
    let mut old_words: Vec<(usize, &str)> = Vec::new();
    let mut pos = 0;
    for w in old.split(' ') {
        if !w.is_empty() {
            old_words.push((pos, w));
        }
        pos += w.len() + 1;
    }
    let new_words: Vec<&str> = new.split(' ').filter(|w| !w.is_empty()).collect();
    let mut best: Option<(usize, usize)> = None;
    for k in 0..old_words.len() {
        let m = old_words[k..]
            .iter()
            .zip(&new_words)
            .take_while(|((_, a), b)| a == *b)
            .count();
        if m > 0 && best.is_none_or(|(_, bm)| m > bm) {
            best = Some((old_words[k].0, m));
        }
    }
    if let Some((off, _)) = best {
        return off;
    }
    [" — ", "; ", ": ", ", ", " -> "]
        .iter()
        .filter_map(|sep| old.rfind(sep).map(|i| i + sep.len()))
        .max()
        .unwrap_or(0)
}

/// Splits `s` at top-level commas, outside `<>` and `()`. Yields each piece
/// with its byte offset in `s`.
pub fn split_top(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push((start, &s[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push((start, &s[start..]));
    out
}

pub const PRIMITIVES: [&str; 13] = [
    "String", "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "Uuid",
];

pub fn valid_type(s: &str) -> bool {
    let s = s.trim();
    let (name, args) = match s.find('<') {
        Some(i) => {
            let Some(inner) = s[i + 1..].strip_suffix('>') else {
                return false;
            };
            (&s[..i], Some(inner))
        }
        None => (s, None),
    };
    let name_ok = is_ident(name)
        && (PRIMITIVES.contains(&name) || name.starts_with(|c: char| c.is_ascii_uppercase()));
    if !name_ok {
        return false;
    }
    match args {
        None => true,
        Some(inner) => {
            let parts = split_top(inner);
            !inner.trim().is_empty() && parts.iter().all(|(_, p)| valid_type(p))
        }
    }
}

/// Parses `fn [Type::]name(param: Type, …) [-> Type]` and returns the name
/// as written.
pub fn parse_signature(text: &str) -> Option<String> {
    let s = text.strip_prefix("fn ")?;
    let open = s.find('(')?;
    let name = &s[..open];
    if name.is_empty() || !name.split("::").all(is_ident) {
        return None;
    }
    let mut depth = 0;
    let mut close = None;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' | '<' => depth += 1,
            ')' | '>' => {
                depth -= 1;
                if depth == 0 && c == ')' {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let params = &s[open + 1..close];
    if !params.trim().is_empty() {
        for (_, p) in split_top(params) {
            let (n, t) = p.split_once(':')?;
            if !is_ident(n.trim()) || !valid_type(t) {
                return None;
            }
        }
    }
    let rest = s[close + 1..].trim();
    if !rest.is_empty() {
        let ret = rest.strip_prefix("->")?;
        if !valid_type(ret) {
            return None;
        }
    }
    Some(name.to_owned())
}

/// A schedule label as Bevy code writes it: `Update`, `FixedUpdate`,
/// `OnEnter(Screen::Gameplay)`.
pub fn is_schedule(s: &str) -> bool {
    let (head, arg) = match s.split_once('(') {
        Some((h, rest)) => match rest.strip_suffix(')') {
            Some(a) => (h, Some(a)),
            None => return false,
        },
        None => (s, None),
    };
    is_pascal(head)
        && arg.is_none_or(|a| {
            !a.is_empty() && a.split("::").all(|seg| is_pascal(seg) || is_ident(seg))
        })
}

pub fn is_commit_id(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn is_pascal(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn is_snake(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn is_anchor_name(s: &str) -> bool {
    is_snake(s)
}

pub fn is_kebab(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && !s.ends_with('-')
        && !s.contains("--")
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
