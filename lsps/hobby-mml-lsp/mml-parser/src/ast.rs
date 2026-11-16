//! The HMML v3 AST: one PR delta.
//!
//! Every body line of every section is kept in [`Document::lines`], in file
//! order, with its gutter, content depth, annotations and parent in the
//! indentation tree. The section structures ([`Unit`], [`TypeDef`],
//! [`Module`], [`TestEntry`], …) refer to those lines by index.

use crate::diagnostic::Diagnostic;
use crate::span::{Span, Spanned};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Document {
    pub header: Header,
    pub sections: Vec<Section>,
    pub intent: Vec<usize>,
    pub data: Vec<TypeDef>,
    pub units: Vec<Unit>,
    pub api: Vec<Module>,
    pub tests: Vec<TestEntry>,
    pub lines: Vec<Line>,
    #[serde(skip)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Header {
    pub entries: Vec<HeaderEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeaderEntry {
    pub key: Spanned<String>,
    pub value: Spanned<String>,
    pub span: Span,
}

impl Header {
    pub fn get(&self, key: &str) -> Option<&Spanned<String>> {
        self.entries
            .iter()
            .find(|e| e.key.value == key)
            .map(|e| &e.value)
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.get(key).map(|v| v.value.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionKind {
    Intent,
    Data,
    Unit,
    Api,
    Tests,
}

impl SectionKind {
    pub fn rank(self) -> u8 {
        match self {
            SectionKind::Intent => 0,
            SectionKind::Data => 1,
            SectionKind::Unit => 2,
            SectionKind::Api => 3,
            SectionKind::Tests => 4,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            SectionKind::Intent => "intent",
            SectionKind::Data => "data",
            SectionKind::Unit => "unit",
            SectionKind::Api => "api",
            SectionKind::Tests => "tests",
        }
    }
}

/// A `##` section: its header line and the whole extent through its last line.
#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub kind: SectionKind,
    pub header: Span,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Gutter {
    Context,
    Added,
    Removed,
    Changed,
}

impl Gutter {
    pub fn from_char(c: char) -> Option<Gutter> {
        match c {
            ' ' => Some(Gutter::Context),
            '+' => Some(Gutter::Added),
            '-' => Some(Gutter::Removed),
            '~' => Some(Gutter::Changed),
            _ => None,
        }
    }

    pub fn as_char(self) -> char {
        match self {
            Gutter::Context => ' ',
            Gutter::Added => '+',
            Gutter::Removed => '-',
            Gutter::Changed => '~',
        }
    }

    pub fn is_change(self) -> bool {
        self != Gutter::Context
    }

    /// The commit side a diff link on a line with this gutter counts on.
    pub fn side(self) -> Side {
        if self == Gutter::Removed {
            Side::Base
        } else {
            Side::Head
        }
    }

    pub fn on_head(self) -> bool {
        self != Gutter::Removed
    }

    pub fn on_base(self) -> bool {
        self != Gutter::Added
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Base,
    Head,
}

/// What a body line is, decided by the section that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Intent,
    Step,
    InvariantsLabel,
    Invariant,
    TypeHeader,
    Field,
    TypeClose,
    Module,
    Fn,
    Test,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub span: Span,
    pub gutter: Gutter,
    pub depth: usize,
    pub section: SectionKind,
    pub role: Role,
    pub bullet: bool,
    /// The content as `head` leaves it: after the bullet, without
    /// annotations. For a `~` line, the reconstructed new sentence.
    pub text: String,
    /// Byte offset of `text`'s first byte in the source (for a `~` line, of
    /// the shared prefix, which is also where the old wording starts).
    pub text_start: usize,
    pub tilde: Option<Tilde>,
    pub ann: Annotations,
    pub parent: Option<usize>,
}

impl Line {
    /// The source span of `len` bytes at offset `rel` of [`Line::text`].
    pub fn sub_span(&self, rel: usize, len: usize) -> Span {
        match &self.tilde {
            Some(t) if rel >= t.prefix_len => {
                let start = t.new_clause.start + (rel - t.prefix_len);
                Span::new(start, (start + len).min(t.new_clause.end))
            }
            _ => Span::new(self.text_start + rel, self.text_start + rel + len),
        }
    }

    /// The span of the whole content, from the bullet to the last annotation.
    pub fn content_span(&self) -> Span {
        Span::new(self.span.start + 2, self.span.end)
    }
}

/// A `~` line split into its two versions.
#[derive(Debug, Clone, Serialize)]
pub struct Tilde {
    pub old: String,
    pub new: String,
    pub prefix_len: usize,
    pub old_clause: Span,
    pub new_clause: Span,
    pub arrow: Span,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Annotations {
    pub anchor: Option<Spanned<String>>,
    pub link: Option<DiffLink>,
    pub api: Vec<Spanned<String>>,
    pub guards: Option<Vec<Spanned<String>>>,
    pub span: Option<Span>,
}

impl Annotations {
    pub fn is_empty(&self) -> bool {
        self.span.is_none()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffLink {
    pub file: Spanned<String>,
    pub line: Option<Spanned<u32>>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitStatus {
    Present,
    New,
    Removed,
}

/// What runs a Bevy unit: a schedule for a system, an event for an observer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Trigger {
    Schedule(Spanned<String>),
    Observer(Spanned<String>),
}

impl Trigger {
    pub fn keyword(&self) -> &'static str {
        match self {
            Trigger::Schedule(_) => "in",
            Trigger::Observer(_) => "on",
        }
    }

    pub fn target(&self) -> &Spanned<String> {
        match self {
            Trigger::Schedule(t) | Trigger::Observer(t) => t,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Unit {
    pub name: Spanned<String>,
    pub status: UnitStatus,
    pub trigger: Option<Trigger>,
    pub what: String,
    pub link: Option<DiffLink>,
    pub header: Span,
    pub span: Span,
    pub steps: Vec<usize>,
    pub invariants: Vec<usize>,
}

impl Unit {
    pub fn link_side(&self) -> Side {
        if self.status == UnitStatus::Removed {
            Side::Base
        } else {
            Side::Head
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeKind {
    Struct,
    Enum,
}

/// The ECS role a `## data` type is registered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EcsRole {
    Component,
    Resource,
    Event,
    Message,
    State,
}

impl EcsRole {
    pub const ALL: [EcsRole; 5] = [
        EcsRole::Component,
        EcsRole::Resource,
        EcsRole::Event,
        EcsRole::Message,
        EcsRole::State,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            EcsRole::Component => "component",
            EcsRole::Resource => "resource",
            EcsRole::Event => "event",
            EcsRole::Message => "message",
            EcsRole::State => "state",
        }
    }

    pub fn from_keyword(word: &str) -> Option<EcsRole> {
        EcsRole::ALL.into_iter().find(|r| r.keyword() == word)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub name: Spanned<String>,
    pub ty: Option<Spanned<String>>,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeDef {
    pub kind: TypeKind,
    pub role: Option<EcsRole>,
    pub name: Spanned<String>,
    pub line: usize,
    pub span: Span,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub name: Spanned<String>,
    pub responsibility: String,
    pub line: usize,
    pub span: Span,
    pub fns: Vec<ApiFn>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiFn {
    /// As written, including any `Type::` qualifier.
    pub name: Spanned<String>,
    pub signature: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestKind {
    Single(String),
    Group(u32),
}

#[derive(Debug, Clone, Serialize)]
pub struct TestEntry {
    pub kind: TestKind,
    pub label: Spanned<String>,
    pub description: String,
    pub line: usize,
}

impl Document {
    pub fn line_at(&self, offset: usize) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| l.span.start <= offset && offset <= l.span.end)
    }

    pub fn anchor_line(&self, name: &str) -> Option<usize> {
        self.lines.iter().position(|l| {
            l.ann
                .anchor
                .as_ref()
                .is_some_and(|a| a.value == name && matches!(l.role, Role::Step | Role::Invariant))
        })
    }

    /// The `## api` entry an `@ api Module::fn` (or `Module::Type::fn`) ref
    /// names. A fn written `fn Module::f` also answers to `Module::f`.
    pub fn resolve_api(&self, path: &str) -> Option<(&Module, &ApiFn)> {
        let (module, rest) = path.split_once("::")?;
        let m = self.api.iter().find(|m| m.name.value == module)?;
        let qualified = format!("{module}::{rest}");
        let f = m
            .fns
            .iter()
            .find(|f| f.name.value == rest)
            .or_else(|| m.fns.iter().find(|f| f.name.value == qualified))?;
        Some((m, f))
    }

    pub fn type_def(&self, name: &str) -> Option<&TypeDef> {
        let mut defs = self.data.iter().filter(|t| t.name.value == name);
        let first = defs.next()?;
        let on_head = |t: &TypeDef| self.lines.get(t.line).is_some_and(|l| l.gutter.on_head());
        if on_head(first) {
            return Some(first);
        }
        defs.find(|t| on_head(t)).or(Some(first))
    }

    pub fn module(&self, name: &str) -> Option<&Module> {
        self.api.iter().find(|m| m.name.value == name)
    }

    /// Every diff link in the file with the commit side it counts on.
    pub fn links(&self) -> Vec<(&DiffLink, Side)> {
        let mut out: Vec<(&DiffLink, Side)> = Vec::new();
        for u in &self.units {
            if let Some(l) = &u.link {
                out.push((l, u.link_side()));
            }
        }
        for line in &self.lines {
            if let Some(l) = &line.ann.link {
                out.push((l, line.gutter.side()));
            }
        }
        out.sort_by_key(|(l, _)| l.span.start);
        out
    }

    /// The diff link at `offset`, with the side it counts on.
    pub fn link_at(&self, offset: usize) -> Option<(&DiffLink, Side)> {
        self.links()
            .into_iter()
            .find(|(l, _)| l.span.start <= offset && offset <= l.span.end)
    }

    pub fn children(&self, idx: usize) -> impl Iterator<Item = usize> + '_ {
        self.lines
            .iter()
            .enumerate()
            .filter(move |(_, l)| l.parent == Some(idx))
            .map(|(i, _)| i)
    }
}
