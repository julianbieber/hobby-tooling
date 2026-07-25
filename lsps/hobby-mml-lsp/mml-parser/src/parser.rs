//! Hand-written, error-recovering parser: source text → [`Document`].
//!
//! The parser is line-oriented (see [`crate::lexer`]). It parses the header,
//! then dispatches section by section. Every sub-parser records diagnostics but
//! never aborts the whole parse, so a mistake in one section still yields an AST
//! for the rest — the property the LSP relies on for partial ASTs on every
//! keystroke. Parse-time diagnostics land in [`Document::diagnostics`]; semantic
//! checks live in [`crate::validate`].

use crate::ast::*;
use crate::diagnostic::{Diagnostic, code};
use crate::lexer::{Line, LineKind, lex};
use crate::span::{Span, Spanned};

/// Parse `src` into a [`Document`]. Never panics; malformed input yields
/// diagnostics plus a best-effort AST.
pub fn parse(src: &str) -> Document {
    let lines = lex(src);
    let mut p = Parser { diags: Vec::new() };

    // Tabs in indentation are illegal everywhere (grammar §1); flag once per line.
    for l in &lines {
        if l.has_tab_indent && l.kind != LineKind::Blank {
            p.push(Diagnostic::error(
                Span::new(l.start, l.content_start),
                code::TAB_INDENT,
                "indentation must use spaces, not tabs",
            ));
        }
    }

    let first_section = lines
        .iter()
        .position(|l| l.kind == LineKind::Section)
        .unwrap_or(lines.len());

    let header = p.parse_header(&lines[..first_section]);

    let mut modules = Vec::new();
    let mut data = DataSection::default();
    let mut components = WorldSection::new(Role::Component);
    let mut resources = WorldSection::new(Role::Resource);
    let mut events = WorldSection::new(Role::Event);
    let mut systems: Vec<SystemDef> = Vec::new();
    let mut access: Option<AccessBlock> = None;
    let mut behavior = Vec::new();
    let mut bindings = Vec::new();
    let mut invariants = Vec::new();

    // Which sections are legal here is a function of `kind` (grammar §7.2). A
    // missing or unparsable kind falls back to the classic four-section schema,
    // so a file being typed top-down does not first look like a broken plugin.
    let kind = header.kind.as_ref().map(|k| k.value);
    let allowed = kind.map(Kind::allowed_sections).unwrap_or(LEGACY_SECTIONS);
    let required = kind.map(Kind::required_sections).unwrap_or(LEGACY_SECTIONS);

    let mut seen = [false; SECTION_ORDER.len()];
    let mut max_order: i32 = -1;

    let mut i = first_section;
    while i < lines.len() {
        let sec_line = &lines[i];
        let name = sec_line.content.trim_start_matches('#').trim();
        let order = SECTION_ORDER
            .iter()
            .position(|n| *n == name)
            .map(|o| o as i32);

        let body_start = i + 1;
        let mut body_end = body_start;
        while body_end < lines.len() && lines[body_end].kind != LineKind::Section {
            body_end += 1;
        }
        let body = &lines[body_start..body_end];

        match order {
            None => {
                let (s, e) = sec_line.content_span();
                p.push(Diagnostic::warning(
                    Span::new(s, e),
                    code::UNKNOWN_SECTION,
                    format!("unknown section `{name}`; expected one of {allowed:?}"),
                ));
            }
            Some(order) => {
                let oi = order as usize;
                let (s, e) = sec_line.content_span();
                if seen[oi] {
                    p.push(Diagnostic::error(
                        Span::new(s, e),
                        code::DUPLICATE_SECTION,
                        format!("duplicate `## {name}` section"),
                    ));
                }
                if order < max_order {
                    p.push(Diagnostic::warning(
                        Span::new(s, e),
                        code::SECTION_OUT_OF_ORDER,
                        format!(
                            "section `{name}` is out of order; canonical order is {SECTION_ORDER:?}"
                        ),
                    ));
                }
                // A section that is real grammar but wrong for this kind still
                // parses below — the LSP wants an AST for it either way.
                if !allowed.contains(&name) {
                    let for_kind = kind
                        .map(|k| format!("`kind: {}`", k.as_str()))
                        .unwrap_or_else(|| "this kind".to_string());
                    p.push(Diagnostic::error(
                        Span::new(s, e),
                        code::SECTION_NOT_ALLOWED,
                        format!(
                            "`## {name}` is not a section of {for_kind}; expected one of {allowed:?}"
                        ),
                    ));
                }
                max_order = max_order.max(order);
                seen[oi] = true;
                match name {
                    "modules" => modules.extend(p.parse_modules(body)),
                    "data" => p.parse_data(body, &mut data),
                    "components" => p.parse_world(body, &mut components),
                    "resources" => p.parse_world(body, &mut resources),
                    "events" => p.parse_world(body, &mut events),
                    "systems" => systems.extend(p.parse_systems(body)),
                    "access" => {
                        let (block, _) = p.parse_access_block(body, 0, Span::new(s, e), true);
                        access = Some(block);
                    }
                    "behavior" => {
                        let (steps, binds) = p.parse_behavior(body);
                        behavior.extend(steps);
                        bindings.extend(binds);
                    }
                    _ => invariants.extend(p.parse_invariants(body)),
                }
            }
        }
        i = body_end;
    }

    let eof = Span::at(src.len());
    for name in required {
        let oi = SECTION_ORDER.iter().position(|n| n == name).unwrap_or(0);
        if !seen[oi] {
            p.push(Diagnostic::error(
                eof,
                code::MISSING_SECTION,
                match kind {
                    Some(k) => format!(
                        "missing required `## {name}` section for `kind: {}`",
                        k.as_str()
                    ),
                    None => format!("missing required `## {name}` section"),
                },
            ));
        }
    }
    if header.system.is_none() {
        p.push(Diagnostic::error(
            Span::at(0),
            code::MISSING_HEADER_KEY,
            "missing required header key `system`",
        ));
    }
    if header.unit.is_none() {
        p.push(Diagnostic::error(
            Span::at(0),
            code::MISSING_HEADER_KEY,
            "missing required header key `unit`",
        ));
    }
    if header.kind.is_none() {
        p.push(Diagnostic::error(
            Span::at(0),
            code::MISSING_HEADER_KEY,
            "missing required header key `kind`",
        ));
    }

    let mut diagnostics = p.diags;
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));

    Document {
        header,
        modules,
        data,
        components,
        resources,
        events,
        systems,
        access,
        behavior,
        bindings,
        invariants,
        diagnostics,
    }
}

struct Parser {
    diags: Vec<Diagnostic>,
}

impl Parser {
    fn push(&mut self, d: Diagnostic) {
        self.diags.push(d);
    }

    // ---- header ------------------------------------------------------------

    fn parse_header(&mut self, hlines: &[Line]) -> Header {
        let mut header = Header {
            system: None,
            unit: None,
            kind: None,
            method: None,
            base: None,
            source: None,
        };
        let mut i = 0;
        while i < hlines.len() {
            let line = &hlines[i];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                i += 1;
                continue;
            }
            let Some((key, _key_abs, value, value_abs)) = split_kv(line) else {
                let (s, e) = line.content_span();
                self.push(Diagnostic::error(
                    Span::new(s, e),
                    code::MALFORMED_HEADER_LINE,
                    "expected `key: value` in header",
                ));
                i += 1;
                continue;
            };
            let value_span = Span::new(value_abs, value_abs + value.len());
            match key {
                "system" => header.system = Some(Spanned::new(value.to_owned(), value_span)),
                "unit" => header.unit = Some(Spanned::new(value.to_owned(), value_span)),
                "method" => header.method = Some(Spanned::new(value.to_owned(), value_span)),
                "base" => header.base = Some(Spanned::new(value.to_owned(), value_span)),
                "kind" => match Kind::parse(value) {
                    Some(k) => header.kind = Some(Spanned::new(k, value_span)),
                    None => self.push(Diagnostic::error(
                        value_span,
                        code::BAD_KIND,
                        format!("`{value}` is not a valid kind; expected one of {KIND_VALUES:?}"),
                    )),
                },
                "source" => {
                    let (source, next) = self.parse_source(hlines, i);
                    header.source = Some(source);
                    i = next;
                    continue;
                }
                other => {
                    let (s, e) = line.content_span();
                    self.push(Diagnostic::warning(
                        Span::new(s, e),
                        code::UNKNOWN_HEADER_KEY,
                        format!("unknown header key `{other}`"),
                    ));
                }
            }
            i += 1;
        }
        header
    }

    /// Parse a `source:` block: indented `repo:` / `paths:` lines and `- path`
    /// items. `start` indexes the `source:` line. Returns the parsed [`Source`]
    /// and the index of the first line after the block.
    fn parse_source(&mut self, hlines: &[Line], start: usize) -> (Source, usize) {
        let mut repo = None;
        let mut paths = Vec::new();
        let mut end = hlines[start].content_start + hlines[start].content.len();

        let mut i = start + 1;
        while i < hlines.len() {
            let line = &hlines[i];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                i += 1;
                continue;
            }
            if line.indent == 0 {
                break; // dedent to column 0 ends the block
            }
            let (cs, ce) = line.content_span();
            end = ce;
            if let Some(item) = line.content.strip_prefix("- ") {
                let item = item.trim();
                let item_abs =
                    cs + (line.content.len() - line.content.trim_start_matches("- ").len());
                paths.push(Spanned::new(
                    item.to_owned(),
                    Span::new(item_abs, item_abs + item.len()),
                ));
            } else if let Some((key, _ka, value, value_abs)) = split_kv(line) {
                match key {
                    "repo" => {
                        repo = Some(Spanned::new(
                            value.to_owned(),
                            Span::new(value_abs, value_abs + value.len()),
                        ));
                    }
                    "paths" => { /* marker line; items follow as `- path` */ }
                    other => self.push(Diagnostic::warning(
                        Span::new(cs, ce),
                        code::UNKNOWN_HEADER_KEY,
                        format!("unknown source key `{other}`"),
                    )),
                }
            }
            i += 1;
        }

        let src_start = hlines[start].content_start;
        (
            Source {
                repo,
                paths,
                span: Span::new(src_start, end),
            },
            i,
        )
    }

    // ---- cross-spec references ----------------------------------------------

    /// Strip a trailing `@ spec <ref>` annotation (grammar §5.4) from `s`, whose
    /// first byte sits at absolute offset `abs`. Returns the remainder (which
    /// may keep trailing whitespace) and the ref. Records diagnostics for a
    /// missing ref, a ref that is not the last annotation on the line, and
    /// duplicate annotations (the last one wins).
    fn split_spec_annotation<'a>(
        &mut self,
        s: &'a str,
        abs: usize,
    ) -> (&'a str, Option<Spanned<String>>) {
        let Some(pos) = find_spec_marker(s) else {
            return (s, None);
        };
        let after = &s[pos + SPEC_MARKER.len()..];
        let raw = after.trim();
        let raw_abs = abs + pos + SPEC_MARKER.len() + leading_ws(after);

        let mut remainder = &s[..pos];
        while let Some(dup) = find_spec_marker(remainder) {
            self.push(Diagnostic::error(
                Span::new(abs + dup, abs + dup + SPEC_MARKER.len()),
                code::DUPLICATE_SPEC_REF,
                "at most one `@ spec` annotation per line",
            ));
            remainder = &remainder[..dup];
        }

        let spec = if raw.is_empty() {
            self.push(Diagnostic::error(
                Span::new(abs + pos, abs + pos + SPEC_MARKER.len()),
                code::MALFORMED_SPEC_REF,
                "`@ spec` is missing its reference; expected `@ spec system/unit` or `@ spec unit`",
            ));
            None
        } else if raw.chars().any(char::is_whitespace) {
            self.push(Diagnostic::error(
                Span::new(raw_abs, raw_abs + raw.len()),
                code::MALFORMED_SPEC_REF,
                format!(
                    "spec reference `{raw}` must not contain spaces; `@ spec` must be the last annotation on the line"
                ),
            ));
            None
        } else {
            Some(Spanned::new(
                raw.to_owned(),
                Span::new(raw_abs, raw_abs + raw.len()),
            ))
        };
        (remainder, spec)
    }

    // ---- modules -----------------------------------------------------------

    fn parse_modules(&mut self, body: &[Line]) -> Vec<Module> {
        let mut modules = Vec::new();
        for line in body {
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                continue;
            }
            let c = line.content;
            let (cs, ce) = line.content_span();
            let Some((sep_rel, sep_len)) = find_separator(c) else {
                self.push(Diagnostic::error(
                    Span::new(cs, ce),
                    code::MALFORMED_MODULE,
                    "expected `Name — responsibility` (use ` — ` or ` -- `)",
                ));
                continue;
            };
            let name_raw = &c[..sep_rel];
            let name = name_raw.trim();
            let name_lead = name_raw.len() - name_raw.trim_start().len();
            let name_abs = cs + name_lead;
            let name_span = Span::new(name_abs, name_abs + name.len());
            if !is_pascal(name) {
                self.push(Diagnostic::warning(
                    name_span,
                    code::MODULE_NAME_NOT_PASCAL,
                    format!("module name `{name}` should be PascalCase"),
                ));
            }

            let rest = &c[sep_rel + sep_len..];
            let rest_abs = cs + sep_rel + sep_len;
            // Strip the `@ spec` annotation first: it is always the last
            // annotation, so the `@ path` split below sees only the remainder.
            let (rest, spec) = self.split_spec_annotation(rest, rest_abs);
            let (resp_raw, path) = match rest.rfind(" @ ") {
                Some(at) => {
                    let path_str = rest[at + 3..].trim();
                    let path_abs = rest_abs + at + 3 + leading_ws(&rest[at + 3..]);
                    let path = Spanned::new(
                        path_str.to_owned(),
                        Span::new(path_abs, path_abs + path_str.len()),
                    );
                    (&rest[..at], Some(path))
                }
                None => (rest, None),
            };
            let resp = resp_raw.trim();
            let resp_abs = rest_abs + leading_ws(resp_raw);
            modules.push(Module {
                name: Spanned::new(name.to_owned(), name_span),
                responsibility: Spanned::new(
                    resp.to_owned(),
                    Span::new(resp_abs, resp_abs + resp.len()),
                ),
                path,
                spec,
                span: Span::new(cs, ce),
            });
        }
        modules
    }

    // ---- data --------------------------------------------------------------

    fn parse_data(&mut self, body: &[Line], data: &mut DataSection) {
        let mut j = 0;
        while j < body.len() {
            let line = &body[j];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                j += 1;
                continue;
            }
            let c = line.content;
            if keyword_at(c, "struct") {
                let (def, next) = self.parse_type_def(body, j, true);
                data.types.push(def);
                j = next;
            } else if keyword_at(c, "enum") {
                let (def, next) = self.parse_type_def(body, j, false);
                data.types.push(def);
                j = next;
            } else if let Some(rest) = c.strip_prefix("in:") {
                data.input = self.parse_io_type(line, "in:", rest);
                j += 1;
            } else if let Some(rest) = c.strip_prefix("out:") {
                data.output = self.parse_io_type(line, "out:", rest);
                j += 1;
            } else if keyword_at(c, "use") {
                let imp = self.parse_use_import(line);
                data.imports.push(imp);
                j += 1;
            } else {
                let (s, e) = line.content_span();
                self.push(Diagnostic::error(
                    Span::new(s, e),
                    code::MALFORMED_STRUCT,
                    "expected a `struct`/`enum` definition, a `use` import, or an `in:`/`out:` line",
                ));
                j += 1;
            }
        }
    }

    // ---- Bevy world sections (grammar §8.1) ---------------------------------

    /// Parse a `## components` / `## resources` / `## events` body: the `## data`
    /// type grammar plus `marker`, minus `in:`/`out:` (data flows through the
    /// World, so a Bevy unit declares no input or output type).
    fn parse_world(&mut self, body: &[Line], section: &mut WorldSection) {
        let mut j = 0;
        while j < body.len() {
            let line = &body[j];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                j += 1;
                continue;
            }
            let c = line.content;
            if keyword_at(c, "struct") {
                let (def, next) = self.parse_type_def(body, j, true);
                section.types.push(def);
                j = next;
            } else if keyword_at(c, "enum") {
                let (def, next) = self.parse_type_def(body, j, false);
                section.types.push(def);
                j = next;
            } else if keyword_at(c, "marker") {
                if let Some(def) = self.parse_marker(line, section.role) {
                    section.types.push(def);
                }
                j += 1;
            } else if keyword_at(c, "use") {
                let imp = self.parse_use_import(line);
                section.imports.push(imp);
                j += 1;
            } else if c.starts_with("in:") || c.starts_with("out:") {
                let (s, e) = line.content_span();
                self.push(Diagnostic::error(
                    Span::new(s, e),
                    code::MALFORMED_STRUCT,
                    format!(
                        "`in:`/`out:` have no meaning in `## {}`; a Bevy unit's interface is its \
                         access block, not an input and output type",
                        section.role.section()
                    ),
                ));
                j += 1;
            } else {
                let (s, e) = line.content_span();
                self.push(Diagnostic::error(
                    Span::new(s, e),
                    code::MALFORMED_STRUCT,
                    "expected a `struct`/`enum`/`marker` definition or a `use` import",
                ));
                j += 1;
            }
        }
    }

    /// Parse a `marker Name` line — a zero-sized component (grammar §8.1).
    fn parse_marker(&mut self, line: &Line, role: Role) -> Option<TypeDef> {
        let c = line.content;
        let (cs, ce) = line.content_span();
        let kw = "marker";
        let name_raw = &c[kw.len()..];
        let name = name_raw.trim();
        let name_abs = cs + kw.len() + leading_ws(name_raw);
        let name_span = Span::new(name_abs, name_abs + name.len());

        if role != Role::Component {
            self.push(Diagnostic::error(
                Span::new(cs, ce),
                code::MALFORMED_MARKER,
                format!(
                    "`marker` declares a zero-sized component; it has no meaning in `## {}`",
                    role.section()
                ),
            ));
            return None;
        }
        if !is_pascal(name) {
            self.push(Diagnostic::error(
                if name.is_empty() {
                    Span::at(name_abs)
                } else {
                    name_span
                },
                code::MALFORMED_MARKER,
                "expected a PascalCase marker name",
            ));
            return None;
        }
        Some(TypeDef::Marker {
            name: Spanned::new(name.to_owned(), name_span),
            span: Span::new(cs, ce),
        })
    }

    // ---- Bevy systems (grammar §8.2) ----------------------------------------

    /// Parse a `## systems` body: declaration lines at column 0, each optionally
    /// followed by an indented access block.
    fn parse_systems(&mut self, body: &[Line]) -> Vec<SystemDef> {
        let mut systems = Vec::new();
        let mut j = 0;
        while j < body.len() {
            let line = &body[j];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                j += 1;
                continue;
            }
            let (cs, ce) = line.content_span();
            if line.indent > 0 {
                // An attribute line with nothing to attach to.
                self.push(Diagnostic::error(
                    Span::new(cs, ce),
                    code::MALFORMED_ACCESS,
                    "access attribute has no system entry above it",
                ));
                j += 1;
                continue;
            }

            let c = line.content;
            let Some((sep_rel, sep_len)) = find_separator(c) else {
                self.push(Diagnostic::error(
                    Span::new(cs, ce),
                    code::MALFORMED_SYSTEM,
                    "expected `system_name — responsibility` (use ` — ` or ` -- `)",
                ));
                j += 1;
                continue;
            };

            let name_raw = &c[..sep_rel];
            let name = name_raw.trim();
            let name_abs = cs + (name_raw.len() - name_raw.trim_start().len());
            let name_span = Span::new(name_abs, name_abs + name.len());
            if !is_lower_id(name) {
                self.push(Diagnostic::warning(
                    name_span,
                    code::SYSTEM_NAME_NOT_SNAKE,
                    format!("system name `{name}` should be snake_case, like the Rust fn"),
                ));
            }

            // Same annotation grammar as a module line: `@ spec` last, then
            // `@ path`. The schedule lives in the access block, not here.
            let rest = &c[sep_rel + sep_len..];
            let rest_abs = cs + sep_rel + sep_len;
            let (rest, spec) = self.split_spec_annotation(rest, rest_abs);
            let (resp_raw, path) = match rest.rfind(" @ ") {
                Some(at) => {
                    let path_str = rest[at + 3..].trim();
                    let path_abs = rest_abs + at + 3 + leading_ws(&rest[at + 3..]);
                    (
                        &rest[..at],
                        Some(Spanned::new(
                            path_str.to_owned(),
                            Span::new(path_abs, path_abs + path_str.len()),
                        )),
                    )
                }
                None => (rest, None),
            };
            let resp = resp_raw.trim();
            let resp_abs = rest_abs + leading_ws(resp_raw);

            let (access, next) = self.parse_access_block(body, j + 1, Span::new(cs, ce), false);
            let end = if access.span.end > ce {
                access.span.end
            } else {
                ce
            };
            systems.push(SystemDef {
                name: Spanned::new(name.to_owned(), name_span),
                responsibility: Spanned::new(
                    resp.to_owned(),
                    Span::new(resp_abs, resp_abs + resp.len()),
                ),
                path,
                spec,
                access,
                span: Span::new(cs, end),
            });
            j = next;
        }
        systems
    }

    /// Parse consecutive `key: value` attribute lines starting at `body[start]`,
    /// stopping at the first column-0 line (which begins the next system entry).
    /// Returns the block and the index of that line. With `whole_body` set — the
    /// `## access` section of a `kind: system` file — every line is an attribute
    /// and the indent rule does not apply, so nothing stops the scan early.
    fn parse_access_block(
        &mut self,
        body: &[Line],
        start: usize,
        anchor: Span,
        whole_body: bool,
    ) -> (AccessBlock, usize) {
        let in_access_section = whole_body;
        let mut block = AccessBlock {
            anchor,
            span: anchor,
            ..Default::default()
        };
        let mut seen: Vec<&str> = Vec::new();
        let mut end = anchor.end;

        let mut j = start;
        while j < body.len() {
            let line = &body[j];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                j += 1;
                continue;
            }
            if line.indent == 0 && !in_access_section {
                break; // dedent starts the next system entry
            }
            let (cs, ce) = line.content_span();
            let Some((key, key_abs, value, value_abs)) = split_kv(line) else {
                self.push(Diagnostic::error(
                    Span::new(cs, ce),
                    code::MALFORMED_ACCESS,
                    format!("expected `key: value`; one of {ACCESS_KEYS:?}"),
                ));
                j += 1;
                continue;
            };
            end = ce;

            if !ACCESS_KEYS.contains(&key) {
                self.push(Diagnostic::error(
                    Span::new(key_abs, key_abs + key.len()),
                    code::UNKNOWN_ACCESS_KEY,
                    format!("unknown access key `{key}`; expected one of {ACCESS_KEYS:?}"),
                ));
                j += 1;
                continue;
            }
            if seen.contains(&key) {
                self.push(Diagnostic::error(
                    Span::new(key_abs, key_abs + key.len()),
                    code::DUPLICATE_ACCESS_KEY,
                    format!("duplicate access key `{key}`"),
                ));
            } else {
                seen.push(key);
            }

            let value_span = Span::new(value_abs, value_abs + value.len());
            if value.is_empty() {
                self.push(Diagnostic::error(
                    value_span,
                    code::MALFORMED_ACCESS,
                    format!("`{key}:` has no value"),
                ));
                j += 1;
                continue;
            }
            let spanned = || Spanned::new(value.to_owned(), value_span);

            match key {
                // Free-text values: a schedule label may carry a state
                // (`OnEnter(Playing)`), a run condition is prose.
                "schedule" => block.schedule = Some(spanned()),
                "run_if" => block.run_if = Some(spanned()),
                "query" => block.query = Some(self.parse_query_filter(value, value_abs)),
                _ => {
                    let items = self.parse_ident_list(key, value, value_abs);
                    match key {
                        "set" => block.sets = items,
                        "after" => block.after = items,
                        "before" => block.before = items,
                        "reads" => block.reads = items,
                        "writes" => block.writes = items,
                        "spawns" => block.spawns = items,
                        "despawns" => block.despawns = items,
                        "emits" => block.emits = items,
                        _ => block.consumes = items,
                    }
                }
            }
            j += 1;
        }

        block.span = Span::new(anchor.start, end.max(anchor.end));
        (block, j)
    }

    /// Parse a comma-separated identifier list (`reads: WaveConfig, Time`).
    fn parse_ident_list(&mut self, key: &str, value: &str, abs: usize) -> Vec<Spanned<String>> {
        let mut out = Vec::new();
        for (item_abs, item) in split_commas(value, abs) {
            let t = item.trim();
            if t.is_empty() {
                continue;
            }
            let t_abs = item_abs + leading_ws(item);
            let span = Span::new(t_abs, t_abs + t.len());
            if !is_ident(t) {
                self.push(Diagnostic::error(
                    span,
                    code::MALFORMED_ACCESS,
                    format!("`{key}:` expects names, but `{t}` is not an identifier"),
                ));
                continue;
            }
            out.push(Spanned::new(t.to_owned(), span));
        }
        out
    }

    /// Parse a `query:` value — `Transform + Spawner - Dead`. A bare or
    /// `+`-prefixed term is required; a `-`-prefixed one is excluded.
    fn parse_query_filter(&mut self, value: &str, abs: usize) -> QueryFilter {
        let mut filter = QueryFilter {
            with: Vec::new(),
            without: Vec::new(),
            span: Span::new(abs, abs + value.len()),
        };
        let mut negate = false;
        for (tok_abs, tok) in split_whitespace_spans(value, abs) {
            // A standalone sign, or one glued to the term (`-Dead`).
            let (tok_abs, tok) = match tok {
                "+" => {
                    negate = false;
                    continue;
                }
                "-" => {
                    negate = true;
                    continue;
                }
                t if t.len() > 1 && (t.starts_with('+') || t.starts_with('-')) => {
                    negate = t.starts_with('-');
                    (tok_abs + 1, &t[1..])
                }
                t => (tok_abs, t),
            };
            let span = Span::new(tok_abs, tok_abs + tok.len());
            if !is_ident(tok) {
                self.push(Diagnostic::error(
                    span,
                    code::MALFORMED_ACCESS,
                    format!("`query:` expects component names joined by `+`/`-`, got `{tok}`"),
                ));
                negate = false;
                continue;
            }
            let entry = Spanned::new(tok.to_owned(), span);
            if negate {
                filter.without.push(entry);
            } else {
                filter.with.push(entry);
            }
            negate = false;
        }
        filter
    }

    /// Parse a `use <spec-ref>: Type, Type` import line (grammar §4.5). The ref
    /// carries no colon, so the first `:` separates it from the comma-separated
    /// type list. Structural problems (missing `:`, empty ref, empty type list)
    /// are `MALFORMED_IMPORT` errors here; ref shape, PascalCase type names, and
    /// duplicates are checked in [`crate::validate`].
    fn parse_use_import(&mut self, line: &Line) -> Import {
        let c = line.content;
        let kw_len = "use".len();
        let rest = &c[kw_len..];
        let rest_abs = line.content_start + kw_len;
        let (cs, ce) = line.content_span();

        let (had_colon, ref_raw, ref_abs, types_raw, types_abs) = match rest.find(':') {
            Some(colon) => (
                true,
                &rest[..colon],
                rest_abs,
                &rest[colon + 1..],
                rest_abs + colon + 1,
            ),
            None => {
                self.push(Diagnostic::error(
                    Span::new(cs, ce),
                    code::MALFORMED_IMPORT,
                    "expected `use system/unit: Type, …`",
                ));
                (false, rest, rest_abs, "", rest_abs + rest.len())
            }
        };

        let spec_str = ref_raw.trim();
        let spec_abs = ref_abs + leading_ws(ref_raw);
        let spec_span = Span::new(spec_abs, spec_abs + spec_str.len());
        if spec_str.is_empty() {
            self.push(Diagnostic::error(
                spec_span,
                code::MALFORMED_IMPORT,
                "`use` is missing its spec reference; expected `use system/unit: Type`",
            ));
        }

        let mut types = Vec::new();
        for (item_abs, item) in split_commas(types_raw, types_abs) {
            let t = item.trim();
            if t.is_empty() {
                continue;
            }
            let t_abs = item_abs + leading_ws(item);
            types.push(Spanned::new(
                t.to_owned(),
                Span::new(t_abs, t_abs + t.len()),
            ));
        }
        if had_colon && types.is_empty() && !spec_str.is_empty() {
            self.push(Diagnostic::error(
                Span::new(cs, ce),
                code::MALFORMED_IMPORT,
                "`use` names no types to import; expected `use system/unit: Type, …`",
            ));
        }

        Import {
            spec: Spanned::new(spec_str.to_owned(), spec_span),
            types,
            span: Span::new(cs, ce),
        }
    }

    fn parse_io_type(&mut self, line: &Line, prefix: &str, rest: &str) -> Option<TypeExpr> {
        let type_str = rest.trim();
        let abs = line.content_start + prefix.len() + leading_ws(rest);
        if type_str.is_empty() {
            self.push(Diagnostic::error(
                Span::at(line.content_start + prefix.len()),
                code::MALFORMED_TYPE,
                format!("`{}` line has no type", prefix.trim_end_matches(':')),
            ));
            return None;
        }
        Some(self.parse_type_expr(type_str, abs))
    }

    /// Parse a `struct`/`enum` block starting at `body[start]`. Returns the
    /// definition and the index of the first line after the closing `}`.
    fn parse_type_def(&mut self, body: &[Line], start: usize, is_struct: bool) -> (TypeDef, usize) {
        let first = &body[start];
        let c = first.content;
        let kw = if is_struct { "struct" } else { "enum" };
        let def_start = first.content_start;

        let brace_rel = c.find('{');
        let name_end = brace_rel.unwrap_or(c.len());
        let name_raw = &c[kw.len()..name_end];
        let name = name_raw.trim();
        let name_abs = first.content_start + kw.len() + leading_ws(name_raw);
        let name_span = Span::new(name_abs, name_abs + name.len());
        if !is_pascal(name) {
            self.push(Diagnostic::error(
                if name.is_empty() {
                    Span::at(name_abs)
                } else {
                    name_span
                },
                code::MALFORMED_STRUCT,
                format!("expected a PascalCase {kw} name"),
            ));
        }

        // Collect (absolute_start, text) segments between the braces.
        let mut segments: Vec<(usize, &str)> = Vec::new();
        let mut closed = false;
        let mut end_offset = first.content_start + c.len();
        let mut j = start;

        match brace_rel {
            Some(br) => {
                let rest = &c[br + 1..];
                let rest_abs = first.content_start + br + 1;
                if let Some(cl) = rest.find('}') {
                    segments.push((rest_abs, &rest[..cl]));
                    end_offset = rest_abs + cl + 1;
                    closed = true;
                } else {
                    segments.push((rest_abs, rest));
                }
            }
            None => {
                self.push(Diagnostic::error(
                    name_span,
                    code::MALFORMED_STRUCT,
                    format!("expected `{{` after {kw} name"),
                ));
            }
        }
        j += 1;

        if !closed && brace_rel.is_some() {
            while j < body.len() {
                let line = &body[j];
                if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                    j += 1;
                    continue;
                }
                let cc = line.content;
                if let Some(cl) = cc.find('}') {
                    segments.push((line.content_start, &cc[..cl]));
                    end_offset = line.content_start + cl + 1;
                    closed = true;
                    j += 1;
                    break;
                }
                segments.push((line.content_start, cc));
                j += 1;
            }
        }

        if !closed {
            self.push(Diagnostic::error(
                name_span,
                code::UNTERMINATED_BLOCK,
                format!("unterminated {kw} block; expected `}}`"),
            ));
        }

        let span = Span::new(def_start, end_offset);
        if is_struct {
            let fields = self.parse_fields(&segments);
            (
                TypeDef::Struct {
                    name: Spanned::new(name.to_owned(), name_span),
                    fields,
                    span,
                },
                j,
            )
        } else {
            let variants = self.parse_variants(&segments);
            (
                TypeDef::Enum {
                    name: Spanned::new(name.to_owned(), name_span),
                    variants,
                    span,
                },
                j,
            )
        }
    }

    fn parse_fields(&mut self, segments: &[(usize, &str)]) -> Vec<Field> {
        let mut fields = Vec::new();
        for &(abs, seg) in segments {
            for (item_abs, item) in split_commas(seg, abs) {
                let item_trim = item.trim();
                if item_trim.is_empty() {
                    continue;
                }
                let item_abs = item_abs + leading_ws(item);
                let Some(colon) = item_trim.find(':') else {
                    self.push(Diagnostic::error(
                        Span::new(item_abs, item_abs + item_trim.len()),
                        code::MALFORMED_FIELD,
                        "expected `name: Type`",
                    ));
                    continue;
                };
                let fname = item_trim[..colon].trim();
                let fname_abs = item_abs + leading_ws(&item_trim[..colon]);
                let fname_span = Span::new(fname_abs, fname_abs + fname.len());
                if !is_lower_id(fname) {
                    self.push(Diagnostic::error(
                        fname_span,
                        code::MALFORMED_FIELD,
                        format!("field name `{fname}` must be snake_case or camelCase"),
                    ));
                }
                let type_part = &item_trim[colon + 1..];
                let tp_base = item_abs + colon + 1;
                // Optional trailing ` @source` (endpoint parameter source, §4.1).
                // Types never contain `@`, so the first ` @` splits it cleanly.
                let (type_str, source) = match type_part.find(" @") {
                    Some(at) => {
                        let src_raw = &type_part[at + 2..];
                        let src = src_raw.trim();
                        let src_abs = tp_base + at + 2 + leading_ws(src_raw);
                        let src_span = Span::new(src_abs, src_abs + src.len());
                        if !matches!(src, "path" | "query" | "body" | "header") {
                            self.push(Diagnostic::error(
                                src_span,
                                code::BAD_PARAM_SOURCE,
                                format!(
                                    "`{src}` is not a valid parameter source; expected path, query, body, or header"
                                ),
                            ));
                        }
                        (
                            &type_part[..at],
                            Some(Spanned::new(src.to_owned(), src_span)),
                        )
                    }
                    None => (type_part, None),
                };
                let type_abs = tp_base + leading_ws(type_str);
                let ty = self.parse_type_expr(type_str.trim(), type_abs);
                let end = source.as_ref().map(|s| s.span.end).unwrap_or(ty.span.end);
                let span = Span::new(fname_abs, end);
                fields.push(Field {
                    name: Spanned::new(fname.to_owned(), fname_span),
                    ty,
                    source,
                    span,
                });
            }
        }
        fields
    }

    fn parse_variants(&mut self, segments: &[(usize, &str)]) -> Vec<Spanned<String>> {
        let mut variants = Vec::new();
        for &(abs, seg) in segments {
            for (item_abs, item) in split_commas(seg, abs) {
                let v = item.trim();
                if v.is_empty() {
                    continue;
                }
                let v_abs = item_abs + leading_ws(item);
                let span = Span::new(v_abs, v_abs + v.len());
                if !is_ident(v) {
                    self.push(Diagnostic::error(
                        span,
                        code::MALFORMED_FIELD,
                        format!("enum variant `{v}` is not a valid identifier"),
                    ));
                }
                variants.push(Spanned::new(v.to_owned(), span));
            }
        }
        variants
    }

    /// Parse a (trimmed) type expression `s` whose first byte is at `abs`.
    fn parse_type_expr(&mut self, s: &str, abs: usize) -> TypeExpr {
        match s.find('<') {
            None => {
                let ident = s.trim();
                let ident_abs = abs + leading_ws(s);
                let span = Span::new(ident_abs, ident_abs + ident.len());
                if ident.is_empty() {
                    self.push(Diagnostic::error(
                        span,
                        code::MALFORMED_TYPE,
                        "expected a type",
                    ));
                }
                TypeExpr {
                    ident: Spanned::new(ident.to_owned(), span),
                    args: Vec::new(),
                    span,
                }
            }
            Some(lt) => {
                let ident_raw = &s[..lt];
                let ident = ident_raw.trim();
                let ident_abs = abs + leading_ws(ident_raw);
                let ident_span = Span::new(ident_abs, ident_abs + ident.len());

                let close = s.rfind('>');
                let (args, end_abs) = match close {
                    Some(cl) if cl > lt => {
                        let inner = &s[lt + 1..cl];
                        let inner_abs = abs + lt + 1;
                        let mut args = Vec::new();
                        for (arg_abs, arg) in split_commas(inner, inner_abs) {
                            if arg.trim().is_empty() {
                                continue;
                            }
                            let a = arg.trim();
                            args.push(self.parse_type_expr(a, arg_abs + leading_ws(arg)));
                        }
                        (args, abs + cl + 1)
                    }
                    _ => {
                        self.push(Diagnostic::error(
                            Span::new(abs + lt, abs + s.len()),
                            code::MALFORMED_TYPE,
                            "unterminated generic type; expected `>`",
                        ));
                        (Vec::new(), abs + s.len())
                    }
                };
                TypeExpr {
                    ident: Spanned::new(ident.to_owned(), ident_span),
                    args,
                    span: Span::new(ident_abs, end_abs),
                }
            }
        }
    }

    // ---- behavior ----------------------------------------------------------

    fn parse_behavior(&mut self, body: &[Line]) -> (Vec<Step>, Vec<Binding>) {
        // A `where:` line at column 0 ends the bullets and opens the binding
        // block (§5.3). It is a sub-block of `## behavior`, not a fifth section.
        let where_pos = body.iter().position(|l| {
            !matches!(l.kind, LineKind::Blank | LineKind::Comment)
                && l.indent == 0
                && l.content.trim() == "where:"
        });
        let (step_body, bind_body) = match where_pos {
            Some(p) => (&body[..p], &body[p + 1..]),
            None => (body, &body[body.len()..]),
        };
        let mut idx = 0;
        let steps = self.parse_steps(step_body, &mut idx, 0);
        let bindings = self.parse_bindings(bind_body);
        (steps, bindings)
    }

    /// Parse `where:` binding lines: `Symbol = value [@ config-source]` (§5.3).
    fn parse_bindings(&mut self, body: &[Line]) -> Vec<Binding> {
        let mut out = Vec::new();
        for line in body {
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                continue;
            }
            let c = line.content;
            let base = line.content_start;
            let Some(eq) = c.find('=') else {
                let (s, e) = line.content_span();
                self.push(Diagnostic::error(
                    Span::new(s, e),
                    code::MALFORMED_BINDING,
                    "expected `Symbol = value [@ config-source]`",
                ));
                continue;
            };
            let sym = c[..eq].trim();
            let sym_abs = base + leading_ws(c);
            let sym_span = Span::new(sym_abs, sym_abs + sym.len());
            if !is_ident(sym) {
                self.push(Diagnostic::error(
                    sym_span,
                    code::MALFORMED_BINDING,
                    format!("binding symbol `{sym}` is not a valid identifier"),
                ));
            }
            let rest = &c[eq + 1..];
            let rest_base = base + eq + 1;
            // Optional ` @ config-source`.
            let (val_raw, config) = match rest.find(" @") {
                Some(at) => {
                    let cfg_raw = &rest[at + 2..];
                    let cfg = cfg_raw.trim();
                    let cfg_abs = rest_base + at + 2 + leading_ws(cfg_raw);
                    let cfg_span = Span::new(cfg_abs, cfg_abs + cfg.len());
                    (&rest[..at], Some(Spanned::new(cfg.to_owned(), cfg_span)))
                }
                None => (rest, None),
            };
            let val = val_raw.trim();
            let val_abs = rest_base + leading_ws(val_raw);
            let val_span = Span::new(val_abs, val_abs + val.len());
            if val.is_empty() {
                self.push(Diagnostic::error(
                    val_span,
                    code::MALFORMED_BINDING,
                    "binding is missing a value",
                ));
            }
            let end = config.as_ref().map(|c| c.span.end).unwrap_or(val_span.end);
            out.push(Binding {
                symbol: Spanned::new(sym.to_owned(), sym_span),
                value: Spanned::new(val.to_owned(), val_span),
                config,
                span: Span::new(sym_span.start, end),
            });
        }
        out
    }

    /// Recursively parse steps at indentation >= `min_indent`. Children of a
    /// step are the following lines indented strictly more than it.
    fn parse_steps(&mut self, body: &[Line], idx: &mut usize, min_indent: usize) -> Vec<Step> {
        let mut steps: Vec<Step> = Vec::new();
        while *idx < body.len() {
            let line = &body[*idx];
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                *idx += 1;
                continue;
            }
            if line.indent < min_indent {
                break;
            }
            let this_indent = line.indent;
            let (text, kind, spec) = self.parse_step_line(line);

            // Attachment rules for `else:` / `on error:`.
            match kind {
                StepKind::Else if !matches!(steps.last().map(|s| s.kind), Some(StepKind::If)) => {
                    self.push(Diagnostic::error(
                        text.span,
                        code::DANGLING_ELSE,
                        "`else:` must follow an `if` at the same indentation",
                    ));
                }
                StepKind::OnError
                    if !matches!(steps.last().map(|s| s.kind), Some(StepKind::Try)) =>
                {
                    self.push(Diagnostic::error(
                        text.span,
                        code::DANGLING_ON_ERROR,
                        "`on error:` must follow a `try` at the same indentation",
                    ));
                }
                _ => {}
            }

            *idx += 1;
            let children = self.parse_steps(body, idx, this_indent + 1);
            let own_end = spec.as_ref().map(|s| s.span.end).unwrap_or(text.span.end);
            let end = children.last().map(|c| c.span.end).unwrap_or(own_end);
            let span = Span::new(line.content_start, end);
            steps.push(Step {
                text,
                kind,
                spec,
                children,
                span,
            });
        }
        steps
    }

    fn parse_step_line(
        &mut self,
        line: &Line,
    ) -> (Spanned<String>, StepKind, Option<Spanned<String>>) {
        let c = line.content;
        let (dash_len, _) = if let Some(r) = c.strip_prefix("- ") {
            (2, r)
        } else if let Some(r) = c.strip_prefix('-') {
            (1, r)
        } else {
            (0, c)
        };
        let after = &c[dash_len..];
        let full = after.trim();
        let text_abs = line.content_start + dash_len + leading_ws(after);
        // Strip the annotation before classifying, so `- if x: @ spec y` is
        // still an `if` step.
        let (stripped, spec) = self.split_spec_annotation(full, text_abs);
        let text = stripped.trim_end();
        let span = Span::new(text_abs, text_abs + text.len());
        let kind = classify_step(text);
        (Spanned::new(text.to_owned(), span), kind, spec)
    }

    // ---- invariants --------------------------------------------------------

    fn parse_invariants(&mut self, body: &[Line]) -> Vec<Invariant> {
        let mut invariants = Vec::new();
        for line in body {
            if matches!(line.kind, LineKind::Blank | LineKind::Comment) {
                continue;
            }
            let c = line.content;
            let (dash_len, _) = if let Some(r) = c.strip_prefix("- ") {
                (2, r)
            } else if let Some(r) = c.strip_prefix('-') {
                (1, r)
            } else {
                (0, c)
            };
            let after = &c[dash_len..];
            let full = after.trim();
            let text_abs = line.content_start + dash_len + leading_ws(after);
            let (stripped, spec) = self.split_spec_annotation(full, text_abs);
            let text = stripped.trim_end();
            let text_span = Span::new(text_abs, text_abs + text.len());
            let end = spec.as_ref().map(|s| s.span.end).unwrap_or(text_span.end);
            invariants.push(Invariant {
                text: Spanned::new(text.to_owned(), text_span),
                spec,
                span: Span::new(text_abs, end),
            });
        }
        invariants
    }
}

// ---- free helpers ----------------------------------------------------------

/// The `@ spec` annotation marker (grammar §5.4).
const SPEC_MARKER: &str = "@ spec";

/// Byte offset of the last `@ spec` marker in `s` that sits on word boundaries:
/// at line start or after a space, and followed by a space or end of line (so
/// `@ spec_utils/x` and `@ specification` don't match).
fn find_spec_marker(s: &str) -> Option<usize> {
    let mut search_end = s.len();
    while let Some(pos) = s[..search_end].rfind(SPEC_MARKER) {
        let before_ok = pos == 0 || s.as_bytes()[pos - 1] == b' ';
        let after = &s[pos + SPEC_MARKER.len()..];
        let after_ok = after.is_empty() || after.starts_with(' ');
        if before_ok && after_ok {
            return Some(pos);
        }
        search_end = pos;
    }
    None
}

fn classify_step(text: &str) -> StepKind {
    let ends_colon = text.ends_with(':');
    if text == "else:" {
        StepKind::Else
    } else if text == "try:" {
        StepKind::Try
    } else if ends_colon && text.starts_with("if ") {
        StepKind::If
    } else if ends_colon && text.starts_with("for each ") {
        StepKind::ForEach
    } else if ends_colon && text.starts_with("while ") {
        StepKind::While
    } else if ends_colon && text.starts_with("on error") {
        StepKind::OnError
    } else {
        StepKind::Prose
    }
}

/// Split `key: value`; returns `(key, key_abs, value, value_abs)`.
fn split_kv<'a>(line: &Line<'a>) -> Option<(&'a str, usize, &'a str, usize)> {
    let c = line.content;
    let colon = c.find(':')?;
    let key = c[..colon].trim();
    let key_abs = line.content_start + leading_ws(&c[..colon]);
    let after = &c[colon + 1..];
    let value = after.trim();
    let value_abs = line.content_start + colon + 1 + leading_ws(after);
    Some((key, key_abs, value, value_abs))
}

/// Find the first module separator (` — ` em dash or ` -- `). Returns its byte
/// offset within `c` and its byte length.
fn find_separator(c: &str) -> Option<(usize, usize)> {
    const EM: &str = " \u{2014} ";
    const DD: &str = " -- ";
    let em = c.find(EM).map(|p| (p, EM.len()));
    let dd = c.find(DD).map(|p| (p, DD.len()));
    match (em, dd) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Split `s` on top-level commas (not nested inside `<...>`), returning each
/// piece with its absolute start offset (`base` is the offset of `s[0]`).
fn split_commas(s: &str, base: usize) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => {
                out.push((base + start, &s[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push((base + start, &s[start..]));
    out
}

/// Split `s` on whitespace, returning each token with its absolute start offset
/// (`base` is the offset of `s[0]`) — `split_whitespace` with spans kept.
fn split_whitespace_spans(s: &str, base: usize) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in s.char_indices() {
        if ch.is_whitespace() {
            if let Some(st) = start.take() {
                out.push((base + st, &s[st..i]));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(st) = start {
        out.push((base + st, &s[st..]));
    }
    out
}

fn leading_ws(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

/// Whether `c` begins with keyword `kw` as a whole word (followed by whitespace,
/// `{`, or end of line).
fn keyword_at(c: &str, kw: &str) -> bool {
    c.strip_prefix(kw)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '{']))
}

fn is_pascal(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_lower_id(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
