//! Pure analysis over a parsed [`Document`] and its raw text.
//!
//! Everything the LSP needs to answer a request at a cursor lives here as plain
//! functions of `(text, offset)` or `(&Document)` — no `tower_lsp` types — so the
//! whole interesting surface is unit-testable with string inputs, mirroring the
//! `julian-path-lsp` template.
//!
//! The parser gives us a fully-spanned AST plus a forward byte-offset →
//! [`Position`] map ([`mml_parser::LineIndex`]). This module fills the three gaps
//! the parser API doesn't expose:
//! - the reverse map ([`offset_of`]): LSP [`Position`] → byte offset,
//! - section membership ([`section_at`] / [`sections`]): the AST carries no
//!   section spans,
//! - the identifier under a cursor ([`word_at`]): behavior [`Step`] text is a
//!   single span with no sub-tokens, so hover/goto/references scan raw text and
//!   match against the symbol tables ([`symbols`]).

use std::ops::Range;

use mml_parser::Position;
use mml_parser::ast::{Document, Role, Step, TypeDef};
use mml_parser::span::{Span, Spanned};

// ---- constants -------------------------------------------------------------

// Re-exported from the parser so the two can't drift.
pub use mml_parser::ast::{ACCESS_KEYS, KIND_VALUES};
pub use mml_parser::validate::{
    BUILTIN_COMPONENTS, BUILTIN_EVENTS, BUILTIN_RESOURCES, CONTAINERS, OPAQUE, PRIMITIVES,
    SCHEDULES,
};

pub const HEADER_KEYS: &[&str] = &["system", "unit", "kind", "method", "base", "source"];
pub const SOURCE_KEYS: &[&str] = &["repo", "paths"];
pub const METHOD_VALUES: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
pub const SECTION_HEADERS: &[&str] = &[
    "## modules",
    "## data",
    "## components",
    "## resources",
    "## events",
    "## systems",
    "## access",
    "## behavior",
    "## invariants",
];

/// Behavior control-flow forms (grammar §5.1); the spellings match
/// `parser::classify_step`. Offered verbatim as completions.
pub const CONTROL_FLOW: &[&str] = &["if ", "else:", "for each ", "while ", "try:", "on error:"];

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

// ---- position mapping ------------------------------------------------------

/// Convert an LSP [`Position`] (0-based line, UTF-16 character) into a byte
/// offset into `text` — the inverse of [`mml_parser::LineIndex::position`].
/// A line or column past the end clamps to the end of the line / text.
pub fn offset_of(text: &str, pos: Position) -> usize {
    // Byte offset of the start of the target line.
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
            None => return text.len(), // line beyond EOF
        }
    }

    // Advance `pos.character` UTF-16 units within the line, stopping at newline.
    let mut utf16 = 0u32;
    for (rel, ch) in text[line_start..].char_indices() {
        if utf16 >= pos.character || ch == '\n' {
            return line_start + rel;
        }
        utf16 += ch.len_utf16() as u32;
    }
    text.len()
}

// ---- word under cursor -----------------------------------------------------

/// An identifier found at a cursor, with its absolute byte [`Span`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub span: Span,
}

/// The identifier (`alphanumeric` + `_`) under the byte `offset`, if any. When
/// the cursor sits just after a word (e.g. at end of line), it steps back onto
/// that word — matching `path_lsp`'s `path_token_at` behaviour.
pub fn word_at(text: &str, offset: usize) -> Option<Word> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.is_empty() {
        return None;
    }
    let offset = offset.min(text.len());
    // Index of the char at (or containing) the cursor.
    let mut ci = match chars.binary_search_by_key(&offset, |&(b, _)| b) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    ci = ci.min(chars.len() - 1);

    // On a word char: use it. Otherwise, if the previous char is a word char
    // (cursor just past a word), step back. Else: no word here.
    if !is_ident_char(chars[ci].1) {
        if ci > 0 && is_ident_char(chars[ci - 1].1) {
            ci -= 1;
        } else {
            return None;
        }
    }

    let mut start = ci;
    while start > 0 && is_ident_char(chars[start - 1].1) {
        start -= 1;
    }
    let mut end = ci;
    while end + 1 < chars.len() && is_ident_char(chars[end + 1].1) {
        end += 1;
    }

    let start_b = chars[start].0;
    let end_b = chars[end].0 + chars[end].1.len_utf8();
    Some(Word {
        text: text[start_b..end_b].to_string(),
        span: Span::new(start_b, end_b),
    })
}

// ---- sections --------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Header,
    Modules,
    Data,
    /// One of the Bevy world sections, carrying its ECS role.
    World(Role),
    Systems,
    Access,
    Behavior,
    Invariants,
    Unknown,
}

impl SectionKind {
    /// Whether this section declares types — `## data` and the three world
    /// sections share a body grammar and therefore a completion behaviour.
    pub fn declares_types(self) -> bool {
        matches!(self, SectionKind::Data | SectionKind::World(_))
    }

    /// Whether this section holds access attribute lines.
    pub fn has_access(self) -> bool {
        matches!(self, SectionKind::Systems | SectionKind::Access)
    }
}

fn section_kind_from_name(name: &str) -> SectionKind {
    match name {
        "modules" => SectionKind::Modules,
        "data" => SectionKind::Data,
        "components" => SectionKind::World(Role::Component),
        "resources" => SectionKind::World(Role::Resource),
        "events" => SectionKind::World(Role::Event),
        "systems" => SectionKind::Systems,
        "access" => SectionKind::Access,
        "behavior" => SectionKind::Behavior,
        "invariants" => SectionKind::Invariants,
        _ => SectionKind::Unknown,
    }
}

/// Which section the byte `offset` falls in. Section boundaries are `##`-at-
/// column-0 header lines (the lexer's rule); everything before the first one is
/// the [`SectionKind::Header`].
pub fn section_at(text: &str, offset: usize) -> SectionKind {
    let mut current = SectionKind::Header;
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        let line_end = pos + line.len();
        if line.starts_with("##") {
            current = section_kind_from_name(line.trim_start_matches('#').trim());
        }
        if offset < line_end {
            return current;
        }
        pos = line_end;
    }
    current
}

/// The document's sections as `(kind, byte range)`, each running from its `##`
/// header line to the next header (or EOF). A leading [`SectionKind::Header`]
/// range covers the text before the first `##`. Used to build document symbols.
pub fn sections(text: &str) -> Vec<(SectionKind, Range<usize>)> {
    let mut headers: Vec<(SectionKind, usize)> = Vec::new();
    let mut pos = 0usize;
    for line in text.split_inclusive('\n') {
        if line.starts_with("##") {
            headers.push((
                section_kind_from_name(line.trim_start_matches('#').trim()),
                pos,
            ));
        }
        pos += line.len();
    }

    let mut out = Vec::new();
    let first = headers.first().map(|&(_, s)| s).unwrap_or(text.len());
    if first > 0 {
        out.push((SectionKind::Header, 0..first));
    }
    for (i, &(kind, start)) in headers.iter().enumerate() {
        let end = headers.get(i + 1).map(|&(_, s)| s).unwrap_or(text.len());
        out.push((kind, start..end));
    }
    out
}

// ---- symbol tables ---------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModuleSym {
    pub name: String,
    pub name_span: Span,
    pub responsibility: String,
    pub path: Option<Spanned<String>>,
}

/// How a type was declared. `Marker` exists only in `## components`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeShape {
    Struct,
    Enum,
    Marker,
}

impl TypeShape {
    pub fn noun(self) -> &'static str {
        match self {
            TypeShape::Struct => "struct",
            TypeShape::Enum => "enum",
            TypeShape::Marker => "marker",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeSym {
    pub name: String,
    pub name_span: Span,
    pub def_span: Span,
    pub shape: TypeShape,
    /// The ECS role, for a type declared in a world section; `None` for a
    /// `## data` type in the classic dialect.
    pub role: Option<Role>,
}

impl TypeSym {
    /// What hover and completion call this: `component struct`, `enum`, …
    pub fn detail(&self) -> String {
        match self.role {
            Some(role) => format!("{} {}", role.noun(), self.shape.noun()),
            None => self.shape.noun().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemSym {
    pub name: String,
    pub name_span: Span,
    pub responsibility: String,
    pub schedule: Option<String>,
    pub path: Option<Spanned<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct Symbols {
    pub modules: Vec<ModuleSym>,
    pub types: Vec<TypeSym>,
    pub systems: Vec<SystemSym>,
}

impl Symbols {
    /// Names declared with the given role — the completion list for a
    /// role-constrained access key.
    pub fn with_role(&self, role: Role) -> impl Iterator<Item = &TypeSym> {
        self.types.iter().filter(move |t| t.role == Some(role))
    }
}

/// Build the module + type + system symbol tables from a parsed document.
pub fn symbols(doc: &Document) -> Symbols {
    let modules = doc
        .modules
        .iter()
        .map(|m| ModuleSym {
            name: m.name.value.clone(),
            name_span: m.name.span,
            responsibility: m.responsibility.value.clone(),
            path: m.path.clone(),
        })
        .collect();

    let to_sym = |t: &TypeDef, role: Option<Role>| {
        let name = t.name();
        let shape = match t {
            TypeDef::Struct { .. } => TypeShape::Struct,
            TypeDef::Enum { .. } => TypeShape::Enum,
            TypeDef::Marker { .. } => TypeShape::Marker,
        };
        TypeSym {
            name: name.value.clone(),
            name_span: name.span,
            def_span: t.span(),
            shape,
            role,
        }
    };
    let mut types: Vec<TypeSym> = doc.data.types.iter().map(|t| to_sym(t, None)).collect();
    for section in doc.world_sections() {
        types.extend(section.types.iter().map(|t| to_sym(t, Some(section.role))));
        // Imported names resolve in this file too, but their definition lives
        // in the owning spec — they are handled by the import goto/hover path,
        // not listed here.
    }

    let systems = doc
        .systems
        .iter()
        .map(|s| SystemSym {
            name: s.name.value.clone(),
            name_span: s.name.span,
            responsibility: s.responsibility.value.clone(),
            schedule: s.access.schedule.as_ref().map(|s| s.value.clone()),
            path: s.path.clone(),
        })
        .collect();

    Symbols {
        modules,
        types,
        systems,
    }
}

pub fn module_named<'a>(s: &'a Symbols, name: &str) -> Option<&'a ModuleSym> {
    s.modules.iter().find(|m| m.name == name)
}

pub fn type_named<'a>(s: &'a Symbols, name: &str) -> Option<&'a TypeSym> {
    s.types.iter().find(|t| t.name == name)
}

pub fn system_named<'a>(s: &'a Symbols, name: &str) -> Option<&'a SystemSym> {
    s.systems.iter().find(|sy| sy.name == name)
}

// ---- access blocks (grammar §8.3) -------------------------------------------

/// The access-list entry whose span contains `offset`, with the key it sits
/// under and the roles that key accepts. Drives hover, goto, and references on
/// a `reads:` / `query:` / `emits:` name.
pub fn access_item_at<'a>(
    doc: &'a Document,
    offset: usize,
) -> Option<(&'static str, &'a [Role], &'a Spanned<String>)> {
    for (_, block) in doc.access_blocks() {
        for (key, roles, items) in block.world_lists() {
            for item in items {
                if offset >= item.span.start && offset <= item.span.end {
                    return Some((key, roles, item));
                }
            }
        }
    }
    None
}

/// The ordering reference (`after:` / `before:`) whose span contains `offset`.
pub fn ordering_ref_at(doc: &Document, offset: usize) -> Option<&Spanned<String>> {
    doc.access_blocks()
        .into_iter()
        .flat_map(|(_, b)| b.ordering_refs())
        .find(|r| offset >= r.span.start && offset <= r.span.end)
}

/// Every access block that names `name` in a world list, with the key it was
/// named under — "who writes this component?", the payoff of role-checked
/// access lists.
pub fn access_mentions(doc: &Document, name: &str) -> Vec<(&'static str, Span)> {
    let mut out = Vec::new();
    for (_, block) in doc.access_blocks() {
        for (key, _, items) in block.world_lists() {
            for item in items {
                if item.value == name {
                    out.push((key, item.span));
                }
            }
        }
    }
    out
}

// ---- cross-spec references (grammar §5.4) -----------------------------------

/// The `@ spec` ref whose span contains `offset` (inclusive end, so a cursor
/// just past the ref still hits it). Refs contain `-` and `/`, so callers must
/// use this rather than [`word_at`].
pub fn spec_ref_at(doc: &Document, offset: usize) -> Option<&Spanned<String>> {
    mml_parser::validate::collect_spec_refs(doc)
        .into_iter()
        .find(|s| offset >= s.span.start && offset <= s.span.end)
}

// ---- type imports (grammar §4.5) --------------------------------------------

/// The spec ref of a `use` import whose ref span contains `offset` (inclusive
/// end). Used for goto/hover on the `use system/unit` part of an import line.
pub fn import_ref_at(doc: &Document, offset: usize) -> Option<&Spanned<String>> {
    doc.all_imports()
        .find_map(|i| (offset >= i.spec.span.start && offset <= i.spec.span.end).then_some(&i.spec))
}

/// The `(import spec ref, imported type)` whose type name span contains
/// `offset`. Used for goto/hover on a specific imported type name.
pub fn import_type_at(
    doc: &Document,
    offset: usize,
) -> Option<(&Spanned<String>, &Spanned<String>)> {
    for import in doc.all_imports() {
        for ty in &import.types {
            if offset >= ty.span.start && offset <= ty.span.end {
                return Some((&import.spec, ty));
            }
        }
    }
    None
}

/// The `use` import whose whole-line span contains `offset` (used by completion
/// to find which import's target to list types from).
pub fn import_on_line(doc: &Document, offset: usize) -> Option<&mml_parser::ast::Import> {
    doc.all_imports()
        .find(|i| offset >= i.span.start && offset <= i.span.end)
}

/// Whether `prefix` (the line text before the cursor) ends inside an `@ spec `
/// annotation whose ref is still being typed.
fn in_spec_ref(prefix: &str) -> bool {
    let Some(pos) = prefix.rfind("@ spec") else {
        return false;
    };
    if pos > 0 && prefix.as_bytes()[pos - 1] != b' ' {
        return false;
    }
    let Some(partial) = prefix[pos + "@ spec".len()..].strip_prefix(' ') else {
        return false;
    };
    partial
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

// ---- completion context ----------------------------------------------------

/// What an access key's value position accepts (grammar §8.3) — the completion
/// list for the right-hand side of `schedule:` / `reads:` / `after:` / ….
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessValue {
    /// `schedule:` — the built-in schedule labels.
    Schedule,
    /// `after:` / `before:` / `set:` — the systems and sets in this file.
    Ordering,
    /// `run_if:` — free prose; nothing useful to offer.
    Free,
    /// `query:` / `spawns:` / `despawns:` — components only.
    Components,
    /// `reads:` / `writes:` — components and resources.
    ComponentsOrResources,
    /// `emits:` / `consumes:` — events only.
    Events,
}

impl AccessValue {
    /// The value kind an access key takes, or `None` if `key` is not one.
    pub fn of(key: &str) -> Option<AccessValue> {
        Some(match key {
            "schedule" => AccessValue::Schedule,
            "set" | "after" | "before" => AccessValue::Ordering,
            "run_if" => AccessValue::Free,
            "query" | "spawns" | "despawns" => AccessValue::Components,
            "reads" | "writes" => AccessValue::ComponentsOrResources,
            "emits" | "consumes" => AccessValue::Events,
            _ => return None,
        })
    }

    /// The roles this value position accepts, for filtering completions.
    pub fn roles(self) -> &'static [Role] {
        match self {
            AccessValue::Components => &[Role::Component],
            AccessValue::ComponentsOrResources => &[Role::Component, Role::Resource],
            AccessValue::Events => &[Role::Event],
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionContext {
    /// A `#`-starting line — offer the section headers.
    SectionHeader,
    /// Start of an attribute line in `## systems` / `## access` — offer the
    /// closed access-key set.
    AccessKey,
    /// The value position of an access key — offer what that key accepts.
    AccessValue(AccessValue),
    /// Start of a header line — offer `system:`/`unit:`/`kind:`/`source:`.
    HeaderKey,
    /// After `kind:` — offer `endpoint`/`workflow`/`job`.
    KindValue,
    /// After `method:` — offer the HTTP verbs.
    MethodValue,
    /// Indented header line inside a `source:` block — offer `repo:`/`paths:`.
    SourceKey,
    /// A data type position (`in:`/`out:`/field type) — offer type names.
    Type,
    /// After `use ` in `## data` (before the `:`) — offer spec refs, like
    /// [`SpecRef`](CompletionContext::SpecRef) but for a type import (§4.5).
    ImportRef,
    /// After `use <ref>: ` in `## data` — offer the type names the referenced
    /// unit defines.
    ImportType,
    /// After `@ spec ` on a module/behavior/invariant line — offer spec refs.
    SpecRef,
    /// Start of a behavior bullet — offer control-flow keywords + names.
    BehaviorKeyword,
    /// Mid-prose in behavior/invariants — offer module + type names.
    BehaviorProse,
    /// No useful completion.
    Other,
}

/// The `key` part of a `key: …` line, if the cursor prefix already contains a
/// colon (i.e. we are in value position).
fn key_before_colon(prefix: &str) -> Option<&str> {
    prefix.find(':').map(|i| prefix[..i].trim())
}

/// Classify what completions make sense at `offset`, from the surrounding
/// section and the current line's text before the cursor.
pub fn completion_context(text: &str, offset: usize) -> CompletionContext {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let prefix = &text[line_start..offset];
    let trimmed = line.trim_start();

    if trimmed.starts_with('#') {
        return CompletionContext::SectionHeader;
    }

    match section_at(text, offset) {
        SectionKind::Header => {
            if let Some(key) = key_before_colon(prefix) {
                if key == "kind" {
                    return CompletionContext::KindValue;
                }
                if key == "method" {
                    return CompletionContext::MethodValue;
                }
                return CompletionContext::Other;
            }
            let indented = line.starts_with(' ') || line.starts_with('\t');
            if indented {
                CompletionContext::SourceKey
            } else {
                CompletionContext::HeaderKey
            }
        }
        // `## data` and the three world sections share a body grammar.
        section if section.declares_types() => {
            // A `use` import line takes precedence over the generic
            // `key: Type` classification (it also contains a `:`).
            if let Some(after_use) = prefix.trim_start().strip_prefix("use ") {
                if after_use.contains(':') {
                    CompletionContext::ImportType
                } else {
                    CompletionContext::ImportRef
                }
            } else if key_before_colon(prefix).is_some() {
                CompletionContext::Type
            } else {
                CompletionContext::Other
            }
        }
        // A `## systems` entry line sits at column 0 and reads like a module
        // line; its indented attributes, and every line of a `## access`
        // section, are access keys.
        section if section.has_access() => {
            if in_spec_ref(prefix) {
                return CompletionContext::SpecRef;
            }
            let is_attribute =
                section == SectionKind::Access || line.starts_with(' ') || line.starts_with('\t');
            if !is_attribute {
                return CompletionContext::Other;
            }
            match key_before_colon(prefix) {
                Some(key) => match AccessValue::of(key) {
                    Some(v) => CompletionContext::AccessValue(v),
                    None => CompletionContext::Other,
                },
                None => CompletionContext::AccessKey,
            }
        }
        section @ (SectionKind::Behavior | SectionKind::Invariants) => {
            if in_spec_ref(prefix) {
                return CompletionContext::SpecRef;
            }
            let body = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix('-'))
                .unwrap_or(trimmed);
            let body_start = line.len() - body.len(); // leading ws + dash bytes
            let cursor_in_line = offset - line_start;
            if section == SectionKind::Invariants {
                return CompletionContext::BehaviorProse;
            }
            if cursor_in_line <= body_start {
                return CompletionContext::BehaviorKeyword;
            }
            let body_prefix = &line[body_start..cursor_in_line];
            if body_prefix.contains(' ') {
                CompletionContext::BehaviorProse
            } else {
                CompletionContext::BehaviorKeyword
            }
        }
        SectionKind::Modules if in_spec_ref(prefix) => CompletionContext::SpecRef,
        _ => CompletionContext::Other,
    }
}

// ---- mentions (references) -------------------------------------------------

/// All whole-word occurrences of `name` in the behavior step tree, as absolute
/// spans. Whole-word so `Proxy` does not match inside `ProxyPool`.
pub fn find_mentions(steps: &[Step], name: &str) -> Vec<Span> {
    let mut out = Vec::new();
    collect_mentions(steps, name, &mut out);
    out
}

fn collect_mentions(steps: &[Step], name: &str, out: &mut Vec<Span>) {
    if name.is_empty() {
        return;
    }
    for step in steps {
        let base = step.text.span.start;
        let hay = step.text.value.as_str();
        for (idx, _) in hay.match_indices(name) {
            let end = idx + name.len();
            let before_ok = hay[..idx]
                .chars()
                .next_back()
                .map(|c| !is_ident_char(c))
                .unwrap_or(true);
            let after_ok = hay[end..]
                .chars()
                .next()
                .map(|c| !is_ident_char(c))
                .unwrap_or(true);
            if before_ok && after_ok {
                out.push(Span::new(base + idx, base + end));
            }
        }
        collect_mentions(&step.children, name, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_parser::{LineIndex, analyze};

    const SAMPLE: &str =
        include_str!("../../mml-parser/tests/fixtures/examples/mesh-terrain-chunks.hmml");

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    // --- offset_of ---

    #[test]
    fn offset_of_round_trips_with_line_index() {
        let idx = LineIndex::new(SAMPLE);
        // Every byte offset that is a char boundary should round-trip.
        for (o, _) in SAMPLE.char_indices() {
            let p = idx.position(o);
            assert_eq!(offset_of(SAMPLE, p), o, "offset {o} did not round-trip");
        }
    }

    #[test]
    fn offset_of_multiline_and_clamp() {
        let src = "ab\ncde\n\nf";
        assert_eq!(offset_of(src, pos(0, 0)), 0);
        assert_eq!(offset_of(src, pos(1, 0)), 3); // start of "cde"
        assert_eq!(offset_of(src, pos(1, 2)), 5);
        assert_eq!(offset_of(src, pos(3, 0)), 8); // "f"
        assert_eq!(offset_of(src, pos(3, 99)), 9); // column past EOL clamps
        assert_eq!(offset_of(src, pos(99, 0)), src.len()); // line past EOF clamps
    }

    #[test]
    fn offset_of_utf16() {
        // "é" = 1 UTF-16 unit / 2 bytes; "𝄞" = 2 UTF-16 units / 4 bytes.
        let src = "é𝄞x";
        assert_eq!(offset_of(src, pos(0, 0)), 0);
        assert_eq!(offset_of(src, pos(0, 1)), 2); // after é
        assert_eq!(offset_of(src, pos(0, 3)), 6); // after 𝄞
        assert_eq!(offset_of(src, pos(0, 4)), 7); // after x
    }

    // --- word_at ---

    #[test]
    fn word_mid_and_end() {
        let text = "get a proxy from ProxyPool now";
        let at = text.find("ProxyPool").unwrap();
        assert_eq!(word_at(text, at + 3).unwrap().text, "ProxyPool");
        // cursor just past the end of the word steps back onto it.
        let end = at + "ProxyPool".len();
        assert_eq!(word_at(text, end).unwrap().text, "ProxyPool");
    }

    #[test]
    fn word_none_on_whitespace() {
        assert_eq!(word_at("a   b", 2), None);
    }

    #[test]
    fn word_with_underscore_and_span() {
        let text = "  from_cache: bool";
        let w = word_at(text, 4).unwrap();
        assert_eq!(w.text, "from_cache");
        assert_eq!(&text[w.span.start..w.span.end], "from_cache");
    }

    // --- sections ---

    #[test]
    fn section_at_each_region() {
        assert_eq!(section_at(SAMPLE, 0), SectionKind::Header); // system:
        let m = SAMPLE.find("ChunkScheduler").unwrap();
        assert_eq!(section_at(SAMPLE, m), SectionKind::Modules);
        let d = SAMPLE.find("struct ChunkCoord").unwrap();
        assert_eq!(section_at(SAMPLE, d), SectionKind::Data);
        let b = SAMPLE.find("pull up to X chunk coords").unwrap();
        assert_eq!(section_at(SAMPLE, b), SectionKind::Behavior);
        let i = SAMPLE.find("at most X chunks").unwrap();
        assert_eq!(section_at(SAMPLE, i), SectionKind::Invariants);
    }

    #[test]
    fn section_header_line_belongs_to_its_section() {
        let hdr = SAMPLE.find("## data").unwrap();
        assert_eq!(section_at(SAMPLE, hdr), SectionKind::Data);
    }

    #[test]
    fn sections_partition_the_document() {
        let secs = sections(SAMPLE);
        // Header + four sections.
        let kinds: Vec<_> = secs.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SectionKind::Header,
                SectionKind::Modules,
                SectionKind::Data,
                SectionKind::Behavior,
                SectionKind::Invariants,
            ]
        );
        // Contiguous, covering [0, len).
        assert_eq!(secs.first().unwrap().1.start, 0);
        assert_eq!(secs.last().unwrap().1.end, SAMPLE.len());
        for w in secs.windows(2) {
            assert_eq!(w[0].1.end, w[1].1.start);
        }
    }

    // --- symbols ---

    #[test]
    fn symbols_from_sample() {
        let (doc, _) = analyze(SAMPLE);
        let syms = symbols(&doc);
        let names: Vec<&str> = syms.modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "ChunkScheduler",
                "HeightSampler",
                "ChunkMesher",
                "MeshCache"
            ]
        );
        let cs = module_named(&syms, "ChunkScheduler").unwrap();
        assert!(cs.responsibility.contains("pulls chunk coords"));
        assert_eq!(cs.path.as_ref().unwrap().value, "src/chunk/scheduler.rs");

        let tnames: Vec<&str> = syms.types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(tnames, ["ChunkCoord", "HeightTile", "ChunkMesh"]);
        let mesh = type_named(&syms, "ChunkMesh").unwrap();
        assert_eq!(mesh.shape, TypeShape::Struct);
        // A classic-dialect type carries no ECS role.
        assert_eq!(mesh.role, None);
        assert_eq!(
            &SAMPLE[mesh.name_span.start..mesh.name_span.end],
            "ChunkMesh"
        );
    }

    // --- completion_context ---

    fn ctx(text: &str, needle: &str, extra: usize) -> CompletionContext {
        let at = text.find(needle).unwrap() + extra;
        completion_context(text, at)
    }

    #[test]
    fn completion_contexts() {
        // section header line
        assert_eq!(ctx(SAMPLE, "## data", 3), CompletionContext::SectionHeader);
        // kind value
        assert_eq!(
            completion_context("kind: ", "kind: ".len()),
            CompletionContext::KindValue
        );
        // header key at start of an empty line
        assert_eq!(completion_context("", 0), CompletionContext::HeaderKey);
        // in:/out: type position
        assert_eq!(
            ctx(SAMPLE, "in:  Queue", "in:  ".len()),
            CompletionContext::Type
        );
        assert_eq!(
            ctx(SAMPLE, "out: Stream", "out: ".len()),
            CompletionContext::Type
        );
        // field type position
        let field = "## data\nstruct P {\n  price: ";
        assert_eq!(
            completion_context(field, field.len()),
            CompletionContext::Type
        );
        // behavior: first token → keyword; mid-prose → prose
        let beh = "## behavior\n- ";
        assert_eq!(
            completion_context(beh, beh.len()),
            CompletionContext::BehaviorKeyword
        );
        let beh2 = "## behavior\n- get a proxy ";
        assert_eq!(
            completion_context(beh2, beh2.len()),
            CompletionContext::BehaviorProse
        );
        // invariants → prose
        let inv = "## invariants\n- ";
        assert_eq!(
            completion_context(inv, inv.len()),
            CompletionContext::BehaviorProse
        );
    }

    #[test]
    fn source_block_keys() {
        let text = "system: x\nsource:\n  ";
        assert_eq!(
            completion_context(text, text.len()),
            CompletionContext::SourceKey
        );
    }

    // --- spec refs ---

    #[test]
    fn spec_ref_completion_contexts() {
        // Fresh annotation, partial system, and after the slash — in behavior,
        // invariants, and modules.
        let beh = "## behavior\n- do it @ spec ";
        assert_eq!(
            completion_context(beh, beh.len()),
            CompletionContext::SpecRef
        );
        let beh2 = "## behavior\n- do it @ spec goo";
        assert_eq!(
            completion_context(beh2, beh2.len()),
            CompletionContext::SpecRef
        );
        let inv = "## invariants\n- holds @ spec sys/";
        assert_eq!(
            completion_context(inv, inv.len()),
            CompletionContext::SpecRef
        );
        let m = "## modules\nFoo — does @ spec ";
        assert_eq!(completion_context(m, m.len()), CompletionContext::SpecRef);
        // Not an annotation: prose containing the word spec, or ref already done.
        let prose = "## behavior\n- follow the spec ";
        assert_eq!(
            completion_context(prose, prose.len()),
            CompletionContext::BehaviorProse
        );
        let after = "## behavior\n- do it @ spec sys/unit more";
        assert_eq!(
            completion_context(after, after.len()),
            CompletionContext::BehaviorProse
        );
    }

    #[test]
    fn spec_ref_at_finds_by_containment() {
        let src = "system: s\nunit: u\nkind: job\n\n\
## modules\nFoo — does things @ spec other-system/other-unit\n\n\
## data\nin: String\nout: String\n\n\
## behavior\n- do it @ spec bare-unit\n\n\
## invariants\n- holds\n";
        let (doc, _) = analyze(src);
        let at = src.find("other-system/other-unit").unwrap();
        let r = spec_ref_at(&doc, at + 5).expect("module ref");
        assert_eq!(r.value, "other-system/other-unit");
        // Cursor at the very end of the ref still hits it.
        let bare = src.find("bare-unit").unwrap();
        let r2 = spec_ref_at(&doc, bare + "bare-unit".len()).expect("step ref");
        assert_eq!(r2.value, "bare-unit");
        // Elsewhere: no ref.
        assert!(spec_ref_at(&doc, src.find("do it").unwrap()).is_none());
    }

    // --- type imports ---

    #[test]
    fn import_completion_contexts() {
        // Typing the ref (before the colon) → ImportRef.
        let a = "## data\nuse goo";
        assert_eq!(completion_context(a, a.len()), CompletionContext::ImportRef);
        let b = "## data\nuse terrain-erosion/mesh-terrain-chunks: ";
        assert_eq!(
            completion_context(b, b.len()),
            CompletionContext::ImportType
        );
        // A second type after a comma is still ImportType.
        let c = "## data\nuse foo/bar: Offer, ";
        assert_eq!(
            completion_context(c, c.len()),
            CompletionContext::ImportType
        );
        // A normal `in:`/field position is unaffected.
        let d = "## data\nin: ";
        assert_eq!(completion_context(d, d.len()), CompletionContext::Type);
    }

    #[test]
    fn import_ref_and_type_at() {
        let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\nuse other-system/thing: Alpha, Beta\nin: Alpha\nout: Beta\n\n\
## behavior\n- do it\n\n## invariants\n";
        let (doc, _) = analyze(src);
        // Cursor on the ref.
        let r = import_ref_at(&doc, src.find("other-system/thing").unwrap() + 3).unwrap();
        assert_eq!(r.value, "other-system/thing");
        // Cursor on the second imported type.
        let (spec, ty) = import_type_at(&doc, src.find("Beta").unwrap() + 1).unwrap();
        assert_eq!(spec.value, "other-system/thing");
        assert_eq!(ty.value, "Beta");
        // import_on_line finds the whole import.
        let imp = import_on_line(&doc, src.find("use other").unwrap() + 2).unwrap();
        assert_eq!(imp.types.len(), 2);
    }

    // --- find_mentions ---

    #[test]
    fn mentions_whole_word_only() {
        let (doc, _) = analyze(SAMPLE);
        let hits = find_mentions(&doc.behavior, "HeightSampler");
        assert_eq!(hits.len(), 1);
        assert_eq!(&SAMPLE[hits[0].start..hits[0].end], "HeightSampler");

        // "MeshCache" must not be matched by a prefix of it.
        let partial = find_mentions(&doc.behavior, "Mesh");
        for h in &partial {
            assert_eq!(&SAMPLE[h.start..h.end], "Mesh");
        }
    }

    #[test]
    fn mentions_nested_steps() {
        let (doc, _) = analyze(SAMPLE);
        // MeshCache is mentioned inside nested if/else branches.
        let hits = find_mentions(&doc.behavior, "MeshCache");
        assert_eq!(hits.len(), 2);
    }

    // --- Bevy dialect ---

    const PLUGIN: &str = include_str!("../../mml-parser/tests/fixtures/examples/enemy-waves.hmml");
    const SYSTEM: &str = include_str!("../../mml-parser/tests/fixtures/examples/spawn-enemies.hmml");

    #[test]
    fn sections_cover_the_bevy_kinds() {
        let kinds: Vec<_> = sections(PLUGIN).iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SectionKind::Header,
                SectionKind::Modules,
                SectionKind::World(Role::Component),
                SectionKind::World(Role::Resource),
                SectionKind::World(Role::Event),
                SectionKind::Systems,
                SectionKind::Behavior,
                SectionKind::Invariants,
            ]
        );
        // A `kind: system` file has an `## access` section instead.
        assert!(
            sections(SYSTEM)
                .iter()
                .any(|(k, _)| *k == SectionKind::Access)
        );
        // The classifiers agree with the section list.
        assert!(SectionKind::World(Role::Event).declares_types());
        assert!(SectionKind::Data.declares_types());
        assert!(SectionKind::Systems.has_access());
        assert!(SectionKind::Access.has_access());
        assert!(!SectionKind::Behavior.has_access());
    }

    #[test]
    fn symbols_carry_roles_and_systems() {
        let (doc, _) = analyze(PLUGIN);
        let syms = symbols(&doc);

        let system_names: Vec<&str> = syms.systems.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            system_names,
            ["tick_wave_timer", "spawn_enemies", "detect_cleared"]
        );
        let spawn = system_named(&syms, "spawn_enemies").unwrap();
        assert_eq!(spawn.schedule.as_deref(), Some("FixedUpdate"));

        // Every declared type knows its ECS role, and a marker keeps its shape.
        let wave = type_named(&syms, "Wave").unwrap();
        assert_eq!(wave.role, Some(Role::Component));
        assert_eq!(wave.detail(), "component struct");
        let spawner = type_named(&syms, "Spawner").unwrap();
        assert_eq!(spawner.shape, TypeShape::Marker);
        let state = type_named(&syms, "WaveState").unwrap();
        assert_eq!(state.role, Some(Role::Resource));
        let cleared = type_named(&syms, "WaveCleared").unwrap();
        assert_eq!(cleared.role, Some(Role::Event));

        let components: Vec<&str> = syms
            .with_role(Role::Component)
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(components, ["Wave", "EnemyHealth", "Spawner"]);
    }

    #[test]
    fn access_item_and_ordering_lookups() {
        let (doc, _) = analyze(PLUGIN);

        // Cursor on `WaveState` in spawn_enemies' `writes:` line.
        let off = PLUGIN
            .find(
                "writes:   WaveState
  spawns",
            )
            .unwrap()
            + "writes:   ".len();
        let (key, roles, item) = access_item_at(&doc, off).expect("access item");
        assert_eq!(key, "writes");
        assert_eq!(item.value, "WaveState");
        assert!(roles.contains(&Role::Resource));

        // Cursor on the `after:` target.
        let off = PLUGIN.find("after:    tick_wave_timer").unwrap() + "after:    ".len();
        assert_eq!(
            ordering_ref_at(&doc, off).map(|r| r.value.as_str()),
            Some("tick_wave_timer")
        );
        // An access item is not an ordering ref and vice versa.
        assert!(access_item_at(&doc, off).is_none());
    }

    #[test]
    fn access_mentions_answer_who_touches_this() {
        let (doc, _) = analyze(PLUGIN);
        let mut keys: Vec<&str> = access_mentions(&doc, "WaveState")
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        keys.sort_unstable();
        // tick_wave_timer writes it, spawn_enemies writes it, detect_cleared reads it.
        assert_eq!(keys, ["reads", "writes", "writes"]);

        // A component named in a query and a spawn list.
        let keys: Vec<&str> = access_mentions(&doc, "Spawner")
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(keys, ["query"]);
        assert!(access_mentions(&doc, "Nonexistent").is_empty());
    }

    #[test]
    fn access_completion_contexts() {
        // Attribute key position: indented, no colon yet.
        let a = "## systems\ngo — does it\n  ";
        assert_eq!(completion_context(a, a.len()), CompletionContext::AccessKey);
        // Every line of a `## access` section is an attribute, unindented.
        let b = "## access\n";
        assert_eq!(completion_context(b, b.len()), CompletionContext::AccessKey);

        // Value positions, one per accepted role set.
        let cases = [
            ("  schedule: ", AccessValue::Schedule),
            ("  after: ", AccessValue::Ordering),
            ("  set: ", AccessValue::Ordering),
            ("  run_if: ", AccessValue::Free),
            ("  query: ", AccessValue::Components),
            ("  spawns: ", AccessValue::Components),
            ("  reads: ", AccessValue::ComponentsOrResources),
            ("  writes: ", AccessValue::ComponentsOrResources),
            ("  emits: ", AccessValue::Events),
            ("  consumes: ", AccessValue::Events),
        ];
        for (line, want) in cases {
            let text = format!("## systems\ngo — does it\n{line}");
            assert_eq!(
                completion_context(&text, text.len()),
                CompletionContext::AccessValue(want),
                "for {line:?}"
            );
        }

        // A system declaration line itself offers nothing type-ish.
        let d = "## systems\ngo — does ";
        assert_eq!(completion_context(d, d.len()), CompletionContext::Other);
        // `@ spec` on a system entry still completes spec refs.
        let e = "## systems\ngo — does it @ spec ";
        assert_eq!(completion_context(e, e.len()), CompletionContext::SpecRef);
    }

    #[test]
    fn world_sections_complete_like_data() {
        let a = "## components\nstruct A {\n  x: ";
        assert_eq!(completion_context(a, a.len()), CompletionContext::Type);
        let b = "## resources\nuse ";
        assert_eq!(completion_context(b, b.len()), CompletionContext::ImportRef);
        let c = "## events\nuse other: ";
        assert_eq!(
            completion_context(c, c.len()),
            CompletionContext::ImportType
        );
    }

    #[test]
    fn access_value_role_mapping() {
        assert_eq!(AccessValue::of("mutates"), None);
        assert_eq!(
            AccessValue::of("query").unwrap().roles(),
            &[Role::Component]
        );
        assert_eq!(AccessValue::of("emits").unwrap().roles(), &[Role::Event]);
        assert_eq!(
            AccessValue::of("reads").unwrap().roles(),
            &[Role::Component, Role::Resource]
        );
        // Non-role keys constrain nothing.
        assert!(AccessValue::of("schedule").unwrap().roles().is_empty());
    }

    #[test]
    fn imports_are_found_across_world_sections() {
        let (doc, _) = analyze(SYSTEM);
        // `use enemy-waves: ...` in `## resources`, not `## data`.
        let off = SYSTEM.find("use enemy-waves: WaveConfig").unwrap() + "use ".len();
        assert_eq!(
            import_ref_at(&doc, off).map(|r| r.value.as_str()),
            Some("enemy-waves")
        );
        let off = SYSTEM.find("WaveConfig, WaveState").unwrap() + 1;
        let (spec, ty) = import_type_at(&doc, off).expect("imported type");
        assert_eq!(spec.value, "enemy-waves");
        assert_eq!(ty.value, "WaveConfig");
    }
}
