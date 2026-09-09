//! The MML AST.
//!
//! Every node carries a [`Span`]. The tree is built to survive errors: any
//! section that fails to parse simply contributes fewer nodes (and diagnostics),
//! so the rest of the document is still represented. See `grammar.md` for the
//! authoritative shape.

use crate::diagnostic::Diagnostic;
use crate::span::{Span, Spanned};
use serde::Serialize;

/// A parsed `.hmml` document. `diagnostics` holds parse-time diagnostics only;
/// run [`crate::validate::validate`] to add semantic ones.
///
/// Which of the section fields are populated depends on the header `kind`
/// (grammar §7.2): the classic kinds (`endpoint`/`workflow`/`job`) use
/// [`Document::data`] for their whole data model, while the Bevy kinds
/// (`plugin`/`system`) use [`components`](Document::components) /
/// [`resources`](Document::resources) / [`events`](Document::events) plus
/// [`systems`](Document::systems) or [`access`](Document::access) — and may use
/// [`data`](Document::data) for the shapes that carry no ECS role at all
/// (grammar §4.6). A section used by the wrong kind still parses
/// (so the LSP keeps working mid-edit) but carries a
/// [`SECTION_NOT_ALLOWED`](crate::diagnostic::code::SECTION_NOT_ALLOWED)
/// diagnostic.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    pub header: Header,
    pub modules: Vec<Module>,
    pub data: DataSection,
    /// `## components` — entity data (grammar §8.1).
    pub components: WorldSection,
    /// `## resources` — global singletons (grammar §8.1).
    pub resources: WorldSection,
    /// `## events` — message payloads (grammar §8.1).
    pub events: WorldSection,
    /// `## systems` — one entry per system a `kind: plugin` registers (§8.2).
    pub systems: Vec<SystemDef>,
    /// `## access` — the lone access block of a `kind: system` file (§8.3).
    pub access: Option<AccessBlock>,
    pub behavior: Vec<Step>,
    /// `## api` — one entry per public function of a `kind: module` spec
    /// (grammar §11), each owning the bullet tree that describes it. Empty for
    /// every other kind, which writes `## behavior` in that slot instead.
    pub api: Vec<ApiEntry>,
    /// `where:` symbol bindings at the end of `## behavior` (grammar §5.3).
    pub bindings: Vec<Binding>,
    pub invariants: Vec<Invariant>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Document {
    /// The three world sections in canonical order, for consumers that treat
    /// components / resources / events uniformly (validation, symbols, index).
    pub fn world_sections(&self) -> [&WorldSection; 3] {
        [&self.components, &self.resources, &self.events]
    }

    /// Every type definition in the document, whichever dialect declared it —
    /// `## data` plus the three world sections.
    pub fn all_type_defs(&self) -> impl Iterator<Item = &TypeDef> {
        self.data.types.iter().chain(
            self.world_sections()
                .into_iter()
                .flat_map(|w| w.types.iter()),
        )
    }

    /// Every `use` import in the document, from `## data` and the three world
    /// sections alike.
    pub fn all_imports(&self) -> impl Iterator<Item = &Import> {
        self.data.imports.iter().chain(
            self.world_sections()
                .into_iter()
                .flat_map(|w| w.imports.iter()),
        )
    }

    /// Every `use extern` import in the document (grammar §4.8), from `## data`
    /// and the three world sections alike.
    pub fn all_extern_imports(&self) -> impl Iterator<Item = &ExternImport> {
        self.data.extern_imports.iter().chain(
            self.world_sections()
                .into_iter()
                .flat_map(|w| w.extern_imports.iter()),
        )
    }

    /// Every type declaration that references real code (grammar §4.7), paired
    /// with the ECS role of the section it sits in — `None` for `## data`.
    /// This is what the filesystem-aware checks walk.
    pub fn rust_refs(&self) -> Vec<(&TypeDef, Option<Role>)> {
        let mut out: Vec<(&TypeDef, Option<Role>)> = self
            .data
            .types
            .iter()
            .filter(|t| t.is_reference())
            .map(|t| (t, None))
            .collect();
        for section in self.world_sections() {
            out.extend(
                section
                    .types
                    .iter()
                    .filter(|t| t.is_reference())
                    .map(|t| (t, Some(section.role))),
            );
        }
        out
    }

    /// Every name `## data` brings into scope — declared and imported alike.
    /// In the Bevy dialect these are the role-less shapes (grammar §4.6): legal
    /// as a field type, illegal in an access list.
    pub fn data_names(&self) -> impl Iterator<Item = &Spanned<String>> {
        self.data
            .types
            .iter()
            .map(TypeDef::name)
            .chain(self.data.imports.iter().flat_map(|i| i.types.iter()))
            .chain(self.data.extern_imports.iter().flat_map(|i| i.types.iter()))
    }

    /// The ECS role of every declared and imported world name. Used to check
    /// that an access-list entry fits the key it appears under. `## data` names
    /// are deliberately absent — the section assigns no role, which is what
    /// makes them unusable in an access list.
    pub fn roles(&self) -> std::collections::HashMap<&str, Role> {
        let mut out = std::collections::HashMap::new();
        for section in self.world_sections() {
            for name in section.names() {
                out.entry(name.value.as_str()).or_insert(section.role);
            }
        }
        out
    }

    /// Every access block in the document: one per `## systems` entry, plus the
    /// `## access` block of a `kind: system` file. The [`Spanned`] name is the
    /// system's own name where there is one, and the header `unit` otherwise —
    /// it is what diagnostics anchor to.
    pub fn access_blocks(&self) -> Vec<(Option<&Spanned<String>>, &AccessBlock)> {
        let mut out: Vec<(Option<&Spanned<String>>, &AccessBlock)> = self
            .systems
            .iter()
            .map(|s| (Some(&s.name), &s.access))
            .collect();
        if let Some(access) = &self.access {
            out.push((None, access));
        }
        out
    }
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
    /// A Bevy `Plugin`: what it registers into the `App` and in what order.
    Plugin,
    /// One Bevy system split out of a plugin, with its own access block.
    System,
    /// One module's public surface — the types it owns and the functions it
    /// exposes, each described in full (grammar §11). The detail layer beneath
    /// a behavioural unit's `## modules` line.
    Module,
}

/// Every `kind:` value, in the order the LSP offers them.
pub const KIND_VALUES: &[&str] = &["endpoint", "workflow", "job", "plugin", "system", "module"];

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "endpoint" => Some(Kind::Endpoint),
            "workflow" => Some(Kind::Workflow),
            "job" => Some(Kind::Job),
            "plugin" => Some(Kind::Plugin),
            "system" => Some(Kind::System),
            "module" => Some(Kind::Module),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Endpoint => "endpoint",
            Kind::Workflow => "workflow",
            Kind::Job => "job",
            Kind::Plugin => "plugin",
            Kind::System => "system",
            Kind::Module => "module",
        }
    }

    /// Whether this kind's `## data` may carry `in:` / `out:` (grammar §4.4).
    /// A Bevy kind's interface is its access block; a module's is its
    /// functions' parameters and return types (§11). Only the classic kinds
    /// have a slot for an input and an output type.
    pub fn takes_io(self) -> bool {
        matches!(self, Kind::Endpoint | Kind::Workflow | Kind::Job)
    }

    /// The name this kind spells its third section with: `api` for a module
    /// (grammar §11), `behavior` for everything else. The two are alternative
    /// spellings of one slot, which is why neither is legal on the other's
    /// kinds.
    pub fn steps_section(self) -> &'static str {
        match self {
            Kind::Module => SECTION_API,
            _ => SECTION_BEHAVIOR,
        }
    }

    /// Whether this kind models a Bevy ECS unit, and therefore carries its
    /// interface in the component/resource/event/system sections rather than in
    /// `## data`'s `in:`/`out:`. A Bevy kind may still declare role-less shapes
    /// in `## data` (grammar §4.6).
    pub fn is_bevy(self) -> bool {
        matches!(self, Kind::Plugin | Kind::System)
    }

    /// The sections this kind must declare (grammar §7.2). Absent ones are a
    /// [`MISSING_SECTION`](crate::diagnostic::code::MISSING_SECTION) error.
    pub fn required_sections(self) -> &'static [&'static str] {
        match self {
            Kind::Plugin => &["systems", "behavior", "invariants"],
            Kind::System => &["access", "behavior", "invariants"],
            Kind::Module => MODULE_SECTIONS,
            _ => LEGACY_SECTIONS,
        }
    }

    /// The sections this kind may declare — a superset of
    /// [`required_sections`](Kind::required_sections). Anything else is a
    /// [`SECTION_NOT_ALLOWED`](crate::diagnostic::code::SECTION_NOT_ALLOWED)
    /// error.
    pub fn allowed_sections(self) -> &'static [&'static str] {
        match self {
            Kind::Plugin => &[
                "modules",
                "data",
                "components",
                "resources",
                "events",
                "systems",
                "behavior",
                "invariants",
            ],
            Kind::System => &[
                "modules",
                "data",
                "components",
                "resources",
                "events",
                "access",
                "behavior",
                "invariants",
            ],
            Kind::Module => MODULE_SECTIONS,
            _ => LEGACY_SECTIONS,
        }
    }
}

/// The `## behavior` section name — the third section of every kind but
/// `module`.
pub const SECTION_BEHAVIOR: &str = "behavior";
/// The `## api` section name — the third section of a `kind: module` (§11).
pub const SECTION_API: &str = "api";

/// The four sections of a `kind: module` spec (grammar §11) — required and
/// allowed alike. `## api` takes the slot `## behavior` holds elsewhere.
pub const MODULE_SECTIONS: &[&str] = &["modules", "data", "api", "invariants"];

/// The four sections of the classic (non-Bevy) dialect — required and allowed
/// alike, and the fallback schema when `kind:` is missing or unparsable.
pub const LEGACY_SECTIONS: &[&str] = &["modules", "data", "behavior", "invariants"];

/// Canonical section order across both dialects (grammar §1). A section that
/// appears after one with a higher index warns as out-of-order.
pub const SECTION_ORDER: &[&str] = &[
    "modules",
    "data",
    "components",
    "resources",
    "events",
    "systems",
    "access",
    "api",
    "behavior",
    "invariants",
];

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

/// The separator between a spec ref's unit and a member of it:
/// `unit::function`, `unit::Type::method` (grammar §5.4).
pub const MEMBER_SEP: &str = "::";

/// Split a raw spec ref into `(system, unit)`, dropping any `::member` path so
/// the result always names a *file*. A bare ref (no `/`) has no system — it
/// resolves within the referencing file's own directory.
pub fn split_spec_ref(raw: &str) -> (Option<&str>, &str) {
    let (unit, _member) = split_member(raw);
    match unit.split_once('/') {
        Some((system, unit)) => (Some(system), unit),
        None => (None, unit),
    }
}

/// Split a raw spec ref into its unit part and its optional member path — the
/// `spawn_wave` of `wave-table::spawn_wave`, or the `Wave::advance` of
/// `wave-table::Wave::advance` (grammar §5.4).
pub fn split_member(raw: &str) -> (&str, Option<&str>) {
    match raw.split_once(MEMBER_SEP) {
        Some((unit, member)) => (unit, Some(member)),
        None => (raw, None),
    }
}

/// The member path of a spec ref, if it names one.
pub fn spec_ref_member(raw: &str) -> Option<&str> {
    split_member(raw).1
}

/// Whether `raw` is a well-formed spec ref: `system/unit` or `unit`, each
/// segment matching the header ident rule (grammar §2)
/// `[A-Za-z][A-Za-z0-9._-]*`, optionally followed by a `::member` path of one
/// or two identifier segments (`::function` or `::Type::method`, §5.4).
pub fn is_valid_spec_ref(raw: &str) -> bool {
    let (unit, member) = split_member(raw);
    let unit_ok = unit.matches('/').count() <= 1
        && unit.split('/').all(|seg| {
            seg.starts_with(|c: char| c.is_ascii_alphabetic())
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        });
    let member_ok = match member {
        None => true,
        Some(path) => {
            let segments: Vec<&str> = path.split(MEMBER_SEP).collect();
            (1..=2).contains(&segments.len()) && segments.iter().all(|seg| is_member_ident(seg))
        }
    };
    unit_ok && member_ok
}

/// Whether `s` names a function or a receiver type in a member path:
/// `[A-Za-z_][A-Za-z0-9_]*`, which covers a snake_case function and a
/// PascalCase type alike.
fn is_member_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---- data ------------------------------------------------------------------

/// `## data`. In the classic dialect it is the whole data model, `in:`/`out:`
/// included. In the Bevy dialect it holds the shapes that are *not* components,
/// resources, or events (grammar §4.6) — a field type, a config struct, an enum
/// a component is built from — and `in:`/`out:` are rejected there.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct DataSection {
    /// `use <system>/<unit>: Type, …` type imports (grammar §4.5), which pull
    /// type definitions from another unit into this file's scope, role-less.
    pub imports: Vec<Import>,
    /// `use extern <crate::path>: Type, …` imports (grammar §4.8) — types from a
    /// dependency, which no path in this checkout can name.
    pub extern_imports: Vec<ExternImport>,
    pub types: Vec<TypeDef>,
    /// `in:` — the unit's input type. Classic kinds only.
    pub input: Option<TypeExpr>,
    /// `out:` — the unit's output type. Classic kinds only.
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

/// A `use extern <crate::path>: Type, Type` line (grammar §4.8): types that come
/// from a dependency rather than from this checkout.
///
/// It brings names into scope and takes the declaring section's ECS role,
/// exactly like an [`Import`] — and offers **nothing else**. There is no file to
/// resolve, no derive to check against, and no fields. That asymmetry is why it
/// is a separate keyword rather than a path that happens not to resolve: the
/// guarantee differs, so the syntax should too.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExternImport {
    /// The `::`-separated crate path (`avian3d::prelude`), first segment being
    /// the crate. Never resolved against the filesystem.
    pub path: Spanned<String>,
    pub types: Vec<Spanned<String>>,
    pub span: Span,
}

impl ExternImport {
    /// The crate name — the first `::` segment, which is what docs.rs is
    /// addressed by.
    pub fn crate_name(&self) -> &str {
        self.path
            .value
            .split("::")
            .next()
            .unwrap_or(&self.path.value)
    }

    /// The docs.rs URL for one of this import's types. Uses rustdoc's search
    /// form rather than a guessed `struct.<Name>.html`: a `use extern` line
    /// carries no item kind, and a wrong guess is a 404 while a search always
    /// lands somewhere useful.
    pub fn docs_url(&self, ty: &str) -> String {
        let krate = self.crate_name();
        format!("https://docs.rs/{krate}/latest/{krate}/?search={ty}")
    }
}

/// Whether `s` is a well-formed `::`-separated Rust path — the target of a
/// `use extern` line. Each segment matches `[A-Za-z_][A-Za-z0-9_]*`; a single
/// segment (a bare crate name) is fine.
pub fn is_valid_crate_path(s: &str) -> bool {
    !s.is_empty()
        && s.split("::").all(|seg| {
            seg.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TypeDef {
    Struct {
        name: Spanned<String>,
        /// The field list, or `None` when the `{ … }` body was omitted
        /// entirely — legal only for a type that references real code
        /// (grammar §4.7). `Some(vec![])` is an explicit empty `{}`, which is a
        /// different statement and stays distinguishable.
        fields: Option<Vec<Field>>,
        /// `@ <path>` — the real code this type is declared in (grammar §4.7).
        /// When set, the body is a **delta**, not a definition.
        path: Option<Spanned<String>>,
        span: Span,
    },
    Enum {
        name: Spanned<String>,
        variants: Option<Vec<Variant>>,
        path: Option<Spanned<String>>,
        span: Span,
    },
    /// `marker Name` — a zero-sized component (grammar §8.1). Legal only in the
    /// Bevy world sections; markers are common enough in Bevy that spelling one
    /// as an empty `struct` reads badly.
    Marker {
        name: Spanned<String>,
        path: Option<Spanned<String>>,
        span: Span,
    },
}

impl TypeDef {
    pub fn name(&self) -> &Spanned<String> {
        match self {
            TypeDef::Struct { name, .. }
            | TypeDef::Enum { name, .. }
            | TypeDef::Marker { name, .. } => name,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            TypeDef::Struct { span, .. }
            | TypeDef::Enum { span, .. }
            | TypeDef::Marker { span, .. } => *span,
        }
    }

    /// The `@ <path>` annotation, when this declaration references a type that
    /// already exists in real code (grammar §4.7).
    pub fn path(&self) -> Option<&Spanned<String>> {
        match self {
            TypeDef::Struct { path, .. }
            | TypeDef::Enum { path, .. }
            | TypeDef::Marker { path, .. } => path.as_ref(),
        }
    }

    /// Whether this declaration points at existing code rather than defining a
    /// new shape. A reference's body lists only the members it wants **added**
    /// (`+`), **removed** (`-`), or asserted to exist (unmarked); everything it
    /// does not mention stays as it is.
    pub fn is_reference(&self) -> bool {
        self.path().is_some()
    }
}

/// What a member line in a referenced type's body asks for (grammar §4.7).
/// Meaningless — and rejected — in a type that declares its own shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Delta {
    /// Unmarked: the member is asserted to already exist as written. The only
    /// variant that a filesystem-aware check can contradict.
    #[default]
    Keep,
    /// `+ name: Type` — add this member to the real type.
    Add,
    /// `- name` — remove this member from the real type.
    Remove,
}

impl Delta {
    pub fn marker(self) -> &'static str {
        match self {
            Delta::Keep => "",
            Delta::Add => "+",
            Delta::Remove => "-",
        }
    }
}

/// One `enum` variant. Carries a [`Delta`] for the same reason a [`Field`] does:
/// a referenced enum's body adds and removes variants rather than restating the
/// whole set.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Variant {
    pub name: Spanned<String>,
    pub delta: Delta,
}

// ---- Bevy world sections (grammar §8.1) ------------------------------------

/// The ECS role of a type definition, taken from the section that declares it.
/// Role is what makes an access list checkable: a component cannot be emitted,
/// an event cannot be queried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Component,
    Resource,
    Event,
}

impl Role {
    pub fn section(self) -> &'static str {
        match self {
            Role::Component => "components",
            Role::Resource => "resources",
            Role::Event => "events",
        }
    }

    pub fn noun(self) -> &'static str {
        match self {
            Role::Component => "component",
            Role::Resource => "resource",
            Role::Event => "event",
        }
    }
}

/// One of `## components` / `## resources` / `## events`. Body grammar is the
/// `## data` type-definition grammar minus `in:`/`out:`, plus `marker`. A `use`
/// import here takes the section's role, so a type imported into
/// `## components` is a component in this file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorldSection {
    pub role: Role,
    pub imports: Vec<Import>,
    pub extern_imports: Vec<ExternImport>,
    pub types: Vec<TypeDef>,
}

impl WorldSection {
    pub fn new(role: Role) -> WorldSection {
        WorldSection {
            role,
            imports: Vec::new(),
            extern_imports: Vec::new(),
            types: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.imports.is_empty() && self.extern_imports.is_empty() && self.types.is_empty()
    }

    /// Every name this section brings into scope — defined, imported from a
    /// sibling spec, and imported from a dependency alike. All three take the
    /// section's role; that is the whole of what the section decides.
    pub fn names(&self) -> impl Iterator<Item = &Spanned<String>> {
        self.types
            .iter()
            .map(TypeDef::name)
            .chain(self.imports.iter().flat_map(|i| i.types.iter()))
            .chain(self.extern_imports.iter().flat_map(|i| i.types.iter()))
    }
}

// ---- Bevy systems (grammar §8.2) -------------------------------------------

/// One `## systems` entry: a module-shaped declaration line plus an indented
/// [`AccessBlock`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SystemDef {
    pub name: Spanned<String>,
    pub responsibility: Spanned<String>,
    pub path: Option<Spanned<String>>,
    /// Trailing `@ spec <unit>`: the system is specced in full in its own file.
    pub spec: Option<Spanned<String>>,
    pub access: AccessBlock,
    pub span: Span,
}

/// The closed set of access-block keys (grammar §8.3), in canonical order —
/// also the LSP's completion list.
pub const ACCESS_KEYS: &[&str] = &[
    "schedule", "set", "after", "before", "run_if", "query", "reads", "writes", "spawns",
    "despawns", "emits", "consumes",
];

/// A system's world access: what schedules and orders it, and what it touches.
/// This is the ECS replacement for `in:` / `out:` — data flows through the
/// World, so a system's interface is the set of things it reads and writes.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct AccessBlock {
    /// `schedule:` — the schedule label it runs in (`Update`, `FixedUpdate`,
    /// `OnEnter(Playing)`, …). Required.
    pub schedule: Option<Spanned<String>>,
    /// `set:` — system-set membership.
    pub sets: Vec<Spanned<String>>,
    /// `after:` / `before:` — ordering against systems or sets.
    pub after: Vec<Spanned<String>>,
    pub before: Vec<Spanned<String>>,
    /// `run_if:` — a run condition, kept as free text.
    pub run_if: Option<Spanned<String>>,
    /// The declaration line this block hangs off — a `## systems` entry line, or
    /// the `## access` section header. Where block-level diagnostics anchor.
    pub anchor: Span,
    /// `query:` — the matched component set.
    pub query: Option<QueryFilter>,
    /// `reads:` / `writes:` — components and resources accessed.
    pub reads: Vec<Spanned<String>>,
    pub writes: Vec<Spanned<String>>,
    /// `spawns:` / `despawns:` — components on entities created / destroyed.
    pub spawns: Vec<Spanned<String>>,
    pub despawns: Vec<Spanned<String>>,
    /// `emits:` / `consumes:` — events written / read.
    pub emits: Vec<Spanned<String>>,
    pub consumes: Vec<Spanned<String>>,
    pub span: Span,
}

impl AccessBlock {
    /// The access lists that name world items, paired with the key they came
    /// from and the roles that key accepts. Drives validation and hover.
    pub fn world_lists(&self) -> Vec<(&'static str, &[Role], &[Spanned<String>])> {
        const RW: &[Role] = &[Role::Component, Role::Resource];
        const C: &[Role] = &[Role::Component];
        const E: &[Role] = &[Role::Event];
        let mut out: Vec<(&'static str, &[Role], &[Spanned<String>])> = vec![
            ("reads", RW, &self.reads),
            ("writes", RW, &self.writes),
            ("spawns", C, &self.spawns),
            ("despawns", C, &self.despawns),
            ("emits", E, &self.emits),
            ("consumes", E, &self.consumes),
        ];
        if let Some(q) = &self.query {
            out.push(("query", C, &q.with));
            out.push(("query", C, &q.without));
        }
        out
    }

    /// The ordering references — `after:` / `before:` — that should name a
    /// system or set declared somewhere. `set:` is excluded: it *declares*
    /// membership rather than referencing an existing name.
    pub fn ordering_refs(&self) -> impl Iterator<Item = &Spanned<String>> {
        self.after.iter().chain(self.before.iter())
    }
}

/// A `query:` value — `Transform + Spawner - Dead`. `+` (or a bare leading
/// term) adds a required component; `-` excludes one.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct QueryFilter {
    pub with: Vec<Spanned<String>>,
    pub without: Vec<Spanned<String>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    pub name: Spanned<String>,
    /// `None` only for a `- name` removal, which needs no type — the field
    /// being removed already has one, in the code.
    pub ty: Option<TypeExpr>,
    /// Endpoint parameter source (`path`/`query`/`body`/`header`); grammar §4.1.
    /// `None` when the field carries no `@source` annotation.
    pub source: Option<Spanned<String>>,
    /// Whether this line adds, removes, or asserts the field (grammar §4.7).
    /// Always [`Delta::Keep`] in a type that declares its own shape.
    pub delta: Delta,
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

impl TypeExpr {
    /// The type as written — `Map<String, Vec<Offer>>` — rebuilt from the tree,
    /// for hover and for an `## api` signature.
    pub fn render(&self) -> String {
        if self.args.is_empty() {
            return self.ident.value.clone();
        }
        let args: Vec<String> = self.args.iter().map(TypeExpr::render).collect();
        format!("{}<{}>", self.ident.value, args.join(", "))
    }
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

// ---- api (grammar §11) -----------------------------------------------------

/// One entry of a module spec's `## api` section: a public function's
/// signature plus the bullet tree describing what it does.
///
/// The bullets are ordinary [`Step`]s — same control-flow forms, same nesting,
/// same trailing `@ spec` refs — so a step that calls a collaborator carries
/// the ref at the point of the call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApiEntry {
    /// The receiver type of a method: the `Wave` of `fn Wave::advance(…)`.
    /// `None` for a standalone function.
    pub receiver: Option<Spanned<String>>,
    pub name: Spanned<String>,
    pub params: Vec<Param>,
    /// The return type after `->`. `None` when the function returns nothing.
    pub ret: Option<TypeExpr>,
    /// Trailing ` @ <path>` locating the function in the source, repo-relative
    /// and optionally `:line` — the annotation a module line carries (§3).
    pub path: Option<Spanned<String>>,
    /// Trailing `@ spec` annotation (§5.4) — the last annotation on the line.
    pub spec: Option<Spanned<String>>,
    /// The nested bullets describing the function's behaviour.
    pub steps: Vec<Step>,
    pub span: Span,
}

impl ApiEntry {
    /// How a `@ spec unit::<member>` ref addresses this entry: the bare
    /// function name, or `Type::method` for a method (grammar §5.4).
    pub fn member(&self) -> String {
        match &self.receiver {
            Some(receiver) => format!("{}{MEMBER_SEP}{}", receiver.value, self.name.value),
            None => self.name.value.clone(),
        }
    }

    /// The span of the entry's whole name, receiver included — what a cursor
    /// must sit in to be "on" this function.
    pub fn member_span(&self) -> Span {
        match &self.receiver {
            Some(receiver) => Span::new(receiver.span.start, self.name.span.end),
            None => self.name.span,
        }
    }

    /// The signature as written, rebuilt for hover and completion —
    /// `fn spawn_wave(index: u32) -> Vec<Entity>`.
    pub fn signature(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name.value, p.ty.render()))
            .collect();
        let ret = match &self.ret {
            Some(ty) => format!(" -> {}", ty.render()),
            None => String::new(),
        };
        format!("fn {}({}){ret}", self.member(), params.join(", "))
    }
}

/// One parameter of an [`ApiEntry`]: `name: Type`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Param {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
    pub span: Span,
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
