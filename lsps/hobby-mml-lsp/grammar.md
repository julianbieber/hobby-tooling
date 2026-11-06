---
title: "Hobby HMML — language specification (v2.4, Bevy dialect)"
created: 2026-07-25
tags:
  - spec
  - reference
---

# Hobby HMML — Mental Model Language

**This file is the source of truth for the HMML grammar.** The
`hobby-mml-parser` / `hobby-mml-lsp` binaries and the five `hmml-*` skills
implement exactly what is defined here.

HMML is the hobby dialect and its files are **`.hmml`**. The work dialect keeps
`.mml` and its own `mml-lsp`; the distinct extension is what lets both live on
one machine without Helix guessing. The binaries here stay `hobby-mml-*` — they
never collided with the work ones.

An `.hmml` file describes **one unit of behavior** — small enough to hold in one
head, one review, one diff. Its main use is reading a change before merge:
`/hmml-extract --against <ref>` describes the unit as the branch leaves it, with
what the diff changed marked `# -` / `# +` (§10).

> **v2.0 (2026-07-25) — the Bevy dialect.** Hobby projects are overwhelmingly
> Bevy games, and Bevy breaks MML's founding assumption. A v1 unit is a function:
> its contract is `in: T` / `out: T`. An ECS has no such thing — data flows
> through the World, so a **system**'s interface is the set of components,
> resources, and events it touches, and a **plugin**'s interface is what it
> registers into the `App`.
>
> v2 therefore adds two kinds (`plugin`, `system`) and five sections
> (`## components`, `## resources`, `## events`, `## systems`, `## access`), and
> makes the legal section set a function of `kind:`.
>
> **The change is additive.** `endpoint` / `workflow` / `job` keep the v1.3
> grammar unchanged, `in:` / `out:` included; every v1 spec is still a valid v2
> spec. The Bevy sections are illegal for the classic kinds and vice versa, so
> the two dialects cannot be mixed in one file.

> **v2.1 (2026-07-29) — `## data` in the Bevy dialect.** Not every shape in a
> Bevy feature is a component, a resource, or an event. A spawn pattern, a
> config record nested inside a resource, an enum a component is built from —
> these are plain Rust types, and v2.0 had nowhere to put them: declaring one in
> `## components` claimed it was a component, and leaving it out made every
> field that used it an `undefined-type` error.
>
> A Bevy kind may therefore now declare `## data` too, for the **role-less**
> shapes ([§4.6](#46--data--in-the-bevy-dialect)). `in:` / `out:` stay illegal
> there — the section is a type namespace in this dialect, not an interface —
> and a `## data` name may not appear in an access list, because the whole point
> of a role is that an access list is checkable against it.

> **v2.2 (2026-08-04) — types that already exist.** A spec's type namespace
> could be populated two ways: declare the type here, or import it from a
> sibling `.hmml`. Most of a real feature's types are in neither place — they
> are in the checkout's Rust, or in a dependency. All three workarounds were
> wrong: **redeclaring** the type creates a second definition that drifts (and
> reads to `/hmml-translate` as "make this exist", inviting a diff that deletes
> real fields), **omitting** it makes every field naming it an `undefined-type`,
> and **adding it to the built-in lists** claims Bevy defines it.
>
> v2.2 adds the missing vocabulary — *declared elsewhere* — in two forms, one
> per guarantee:
>
> - **`@ <path>` on a declaration** ([§4.7](#47-referencing-a-rust-type--path)):
>   the type is in this checkout, so the reference is **verified** — the type
>   must be there, its `#[derive(…)]` must match the section it was filed under,
>   and goto-definition lands on the real struct. The body becomes optional, and
>   when present it is a **delta**: `+` adds a member, `-` removes one, unmarked
>   asserts one already exists, and anything not listed stays as it is.
> - **`use extern <crate::path>:`** ([§4.8](#48-importing-from-a-dependency-use-extern)):
>   the type is in a dependency, so nothing is verifiable. It brings names into
>   scope with the section's role and offers nothing else; goto opens its docs.rs
>   page.
>
> Two keywords rather than one because the guarantee differs and should be
> legible at a glance. Additive: every v2.1 spec is still a valid v2.2 spec.

> **v2.3 (2026-08-28) — the spec holds both versions.** A spec used to be edited
> in place to describe the change a task intends, which threw away the thing the
> change is a change *to*: read afterwards, the file said what the code should
> do and nothing said what it did. Recovering the delta meant git archaeology in
> the notes repo, and an edit committed there stopped being a delta at all.
>
> v2.3 adds **change annotations** ([§10](#10-change-annotations)): the file
> keeps the **baseline** — the unit as it is before the change — and the change
> is written on top of it as `# +` / `# -` comment markers. Both versions are in
> one file, the delta survives being committed, and `/hmml-translate` is told
> exactly what to add and remove instead of inferring it.
>
> They are ordinary comments (§1), so this is a **convention, not grammar**: an
> annotated spec parses and validates exactly like any other. What is new is
> vocabulary, plus four checks that the markers still describe a real change.
>
> **§10 is the one change spelling, and it supersedes the v2.2 member delta.**
> A `@ <path>` body's inline `+` / `-` said the same thing in a second way, and
> the two could not both be right about what an unmarked line means: §10 needs
> unannotated text to be the baseline, and `+ streak: u32` is intent. The inline
> form still parses, so every v2.2 spec is still valid — but a new spec writes a
> member change the way it writes every other change
> ([§4.7](#47-referencing-a-rust-type--path)).

> **v2.4 (2026-09-09) — the detail layer (`kind: module`).** The behavioural
> kinds describe a unit at mental-model altitude: a handful of collaborators and
> a dozen steps. That is right for *what does this plugin do*, and it stays. It
> says nothing about what a collaborator can actually **do** — `WaveRng — draws
> the spawn offsets` and no more — so the moment a change is about a module's
> API rather than a unit's flow, the spec has nothing to offer and translation
> falls back to prose → code.
>
> v2.4 adds the layer underneath: `kind: module` ([§11](#11-the-module-dialect-kind-module)),
> whose third section is **`## api`** — one `fn` signature per public function,
> each owning the bullets that describe it. With it come the
> **`unit::function`** / `unit::Type::method` member form of an `@ spec` ref
> ([§5.4](#54-cross-spec-references--spec)) and four containers so a signature
> can stay faithful to the code it describes: `Future`, `Result`, `Either`,
> `Tuple` ([§4.3](#43-type-expressions)).
>
> Additive: every v2.3 spec is still valid. The new kind is opt-in, and `## api`
> on a non-module — or `## behavior` on a module — is a
> `section-not-allowed-for-kind` error rather than a silent reinterpretation.

## 1. File shape

An `.hmml` file is **line-oriented, UTF-8**, and has two parts in a fixed order:

1. A **header** (YAML-ish key/value block) at the top.
2. A sequence of **`##` sections**, in canonical order.

The canonical section order across both dialects is:

```
modules, data, components, resources, events, systems, access, api, behavior, invariants
```

`## api` and `## behavior` are alternative spellings of one slot — a spec has
one or the other, never both ([§11](#11-the-module-dialect-kind-module)).

Which of them are **required**, and which are even **allowed**, depends on the
header `kind` — see [§7](#7-kinds-and-the-section-schema).

Rules that apply everywhere:

- **Blank lines** are allowed between blocks and ignored.
- **Comments** start with `#` at the first non-space column and run to end of
  line. A `#` inside a value (e.g. in a behavior step) is *not* a comment. A
  `##` at column 0 is a section header, never a comment.
- **Indentation is spaces only** (no tabs). One nesting level = **2 spaces**.
- Each section header appears at most once. Out of canonical order is a
  warning; duplicated is an error.

## 2. Header

A block of `key: value` lines before the first `##` section.

| Key | Required | Meaning |
|---|---|---|
| `unit` | yes | Unique identifier (kebab-case). **This is the filename** (`<unit>.hmml`). |
| `kind` | yes | One of `endpoint`, `workflow`, `job`, `plugin`, `system`, `module`. |
| `system` | yes | Free-text label for the conceptual product grouping (`bevy-survivor`). Not a path component. |
| `method` | no | HTTP verb; `kind: endpoint` only. |
| `base` | no | Directory that relative `source.paths` and `@ path` entries resolve against. |
| `source` | no | `repo:` + a `paths:` list — where the real code lives. |

Identifiers match `[A-Za-z][A-Za-z0-9._-]*`.

```
system: bevy-survivor
unit: enemy-waves
kind: plugin
source:
  repo: bevy_survivor
  paths:
    - src/waves/mod.rs
```

## 3. `## modules`

One line per behavioural collaborator:

```
PascalName — responsibility [@ repo-relative-path] [@ spec <unit>]
```

The separator is ` — ` (em dash) or ` -- `. Names should be PascalCase (a
warning otherwise). Optional for the Bevy kinds, where `## systems` carries most
of what `## modules` used to — keep it for the non-system collaborators
(the plugin type itself, an asset loader, a helper).

## 4. `## data`

The type-declaration section. Both dialects have it, and its body grammar is the
same in each; what differs is what it means and what else it may hold. For
`endpoint` / `workflow` / `job` it is the whole data model, `in:` / `out:`
included — unchanged from v1.3. For `plugin` / `system` it holds the shapes that
carry no ECS role at all ([§4.6](#46--data--in-the-bevy-dialect)).

### 4.1 Structs

```
struct Tile {
  bytes: Vec<u8>,
  format: TileFormat,
  zoom: u8 @path,
}
```

Field names are snake_case or camelCase. The optional `@path` / `@query` /
`@body` / `@header` suffix marks an endpoint parameter's source.

The declaration line is `struct Name [@ <path>] [{ … }]`. The optional
`@ <path>` makes it a **reference** to a type that already exists in real code
([§4.7](#47-referencing-a-rust-type--path)) — which is what lets the body be
omitted, and what turns a body that *is* present into a delta. A `struct` with
neither a body nor a path says nothing, and is a `missing-type-body` error.

### 4.2 Enums

Unit variants only, no payloads: `enum TileFormat { png, webp }`. The same
`@ <path>` and optional body apply.

### 4.3 Type expressions

An identifier with optional generic arguments. A type must resolve to one of:

- **primitives** — `String`, `bool`, `i8`…`i64`, `u8`…`u64`, `f32`, `f64`
- **opaque atoms** — `Uuid`, `Money`, `DateTime`, `Entity`, `Duration`, `Vec2`,
  `Vec3`, `Vec4`, `Quat`, `Color`, `Timer`, `Rect`
- **containers** — `Option<T>`, `Vec<T>`, `Map<K,V>`, `Queue<T>`, `Stream<T>`,
  `Handle<T>`, `Future<T>`, `Result<T,E>`, `Either<L,R>`, `Tuple<A,B>`

  The last four exist for `## api` signatures
  ([§11](#11-the-module-dialect-kind-module)): a module spec is read next to the
  code it describes, so a fallible or effectful return is written the way the
  code writes it — `Result<Wave, TableError>` — rather than normalized down to
  `Wave` plus an `on error` bullet. A tuple is spelled as the container
  `Tuple<A, B>`, not `(A, B)`, so the type grammar stays `Ident<args…>`.
- a struct/enum/marker defined or imported in this file

Anything else is an `undefined-type` error.

### 4.4 `in:` / `out:`

```
in:  TileRequest
out: Tile
```

Each is a single type expression. **Illegal in the Bevy dialect** — a Bevy unit's
interface is its access block, not an input and an output — and **illegal in a
`kind: module`**, whose inputs and outputs are its functions' parameters and
return types ([§11](#11-the-module-dialect-kind-module)).

### 4.5 Importing types (`use`)

```
use enemy-waves: Wave, EnemyHealth
```

Brings type **names** (not bodies) from another unit into this file's scope. The
target is a bare `<unit>` resolving to a sibling `<unit>.hmml` in the same task
directory. Legal at the top of `## data` **and** of each world section
([§8.1](#81--components----resources----events)).

Malformed ref / empty type list / non-PascalCase name → `malformed-import`
(error). Importing a type twice, or one also defined locally →
`duplicate-import` (error). A well-formed import whose target file or type does
not exist → `unresolved-import` (warning, filesystem-aware tools only). The
section an import lands in decides the imported name's **role**, never whether
it has to resolve — every `use` line is checked the same way.

### 4.6 `## data` in the Bevy dialect

**`plugin` / `system` only**, and optional. A Bevy feature is full of types that
are not components, resources, or events: a field type, an enum a component is
built from, a config record nested inside a resource. They belong here.

```
## data
enum SpawnPattern { ring, scatter, line }
struct SpawnPoint {
  offset: Vec2,
  pattern: SpawnPattern,
}

## resources
struct WaveConfig {
  interval_secs: f32,
  points: Vec<SpawnPoint>,
}
```

Three rules, all of them the consequence of one idea — **`## data` assigns no
role**:

- **`in:` / `out:` are illegal** (`malformed-struct`). Here the section is a type
  namespace, not an interface; a Bevy unit's interface is its access block.
- **`marker` is illegal** (`malformed-marker`) — a marker *is* a component, so
  it belongs in `## components`.
- **A `## data` name may not appear in an access list.** Naming one under
  `query:` / `reads:` / `emits:` / … is an `access-role-mismatch`, worded to say
  the type exists but has no role, so the fix is to move the declaration rather
  than write it.

The type namespace is shared with the world sections, so declaring the same name
in `## data` and in `## components` is a `duplicate-type` error: a shape is
role-less or it has a role, never both. `use` imports work here too, and an
import into `## data` is role-less exactly like a local declaration.

A field of a component may name a `## data` type freely — that is the whole
point of the section.

### 4.7 Referencing a Rust type (`@ <path>`)

A type declaration may end its head with `@ <repo-relative-path>`, saying **this
type already exists, over there**:

```
## components
struct Wave @ src/waves/mod.rs
struct EnemyHealth @ src/waves/mod.rs {
  hp: f32,
  remaining: u32,
}
marker Spawner @ src/waves/mod.rs
```

The path resolves exactly like a module's `@ path` ([§3](#3--modules)): against
the declared `base:`, then the `source.repo` checkout roots, then the spec's own
directory, then the workspace root. It is legal in **both dialects** and in
every type-declaring section — a `## data` reference is role-less like any other
`## data` name.

**The body is optional, and when present it is partial.** It lists the members
this spec has something to say about; anything it does not mention **stays as it
is**. Every listed member is an **assertion** that the real type already has it.

Changing the member set is a change like any other, so it is written with the
`# +` / `# -` markers of [§10](#10-change-annotations):

```
struct EnemyHealth @ src/waves/mod.rs {
  hp: f32,
# - remaining: u32,
# +
  streak: u32,
}
```

That falls out of what a marker is. `# +` and `# -` are comments, and the parser
skips comments, so an added member never reaches the assertion check below and an
in-progress spec is never noisy about work that has not landed — which is exactly
the property the v2.2 inline markers were written to provide, reached by the
route every other section already uses.

**The v2.2 inline form is legacy.** A member line may still open with `+` or `-`
and still means add / remove, so v2.2 specs keep working and
the parser still reads them; new specs do not write them. Two spellings
for one statement is one too many, and the inline one is the spelling that made
§10's rule — unannotated text is the baseline — untrue inside a body. A `+` or
`-` on a type that declares its own shape stays a `malformed-field` error: there
is nothing to change.

The filesystem-aware tools check four things, all **warnings**:

- the file declares no such type → `rust-type-not-found`
- its `#[derive(…)]` does not match the section's role → `rust-role-mismatch`
- the spec says `struct` and the code says `enum`, or `marker` names a struct
  with fields → `rust-shape-mismatch`
- a member the body asserts is not there → `rust-member-unknown`

A path that does not resolve produces one `unresolved-source-path` warning and
none of the above: nothing about a file that is not there is knowable, so one
warning is honest and four would be noise.

The derive check is the one worth the whole feature. In Bevy a type's role
**is** its derive, so filing a `#[derive(Resource)]` type under `## components`
— the classic spec error — is caught at the moment it is cheap to fix. A
reference is worth more than a copy precisely because a copy cannot be checked.

Bevy renamed buffered events, so `## events` accepts `Event`, `Message`,
`BufferedEvent`, or `EntityEvent`; they are the same statement about the type.

### 4.8 Importing from a dependency (`use extern`)

A Bevy game leans on components it does not own — avian's `Collider`, a
plugin's marker. No path in this checkout names them:

```
## components
use extern avian3d::prelude: Collider, RigidBody
```

Same rules as a `use` import ([§4.5](#45-importing-types-use)): names join the
type namespace, **the section assigns the role**, importing a name twice or one
also defined locally is `duplicate-import`. And nothing else — there is no file
to resolve, no derive to check, no members. Only the path's shape is checkable
(`crate` or `crate::module::path`), which is `malformed-extern-import`.

That asymmetry is why it is a second keyword rather than a path that happens not
to resolve. `@ <path>` means *verified*; `use extern` means *taken on trust*, and
a reader should not have to resolve a path to tell which they are looking at.

`use extern` is told from an ordinary import by token count: `use extern foo: T`
has a target after the keyword, whereas `use extern: T` is an import from a
sibling unit that happens to be called `extern`. Requiring the second token
keeps a legal unit name from being stolen.

Goto-definition on an external type opens its **docs.rs** page in the editor's
browser — for a type with no file on this machine, its documentation is the
nearest thing to a definition there is.

## 5. `## behavior`

Nested `-` bullets; indentation is nesting (2 spaces per level); mostly free
prose.

### 5.1 Soft control-flow keywords (closed set)

`if …:` / `else:` / `for each …:` / `while …:` / `try:` / `on error …:` — each
with a trailing `:` and its body nested one level deeper. Everything else is
prose.

### 5.2 Cross-references

Declared names — modules, systems, structs, enums, markers — written verbatim in
prose are cross-linked by the LSP. Link-if-known, silent-if-unknown: a mistyped
name is a missing link, not an error.

**Convention for `kind: plugin`:** make the top-level bullets the system names,
with each system's logic nested beneath.

```
## behavior
- tick_wave_timer:
  - advance WaveState.elapsed by the frame delta from Time
  - if elapsed exceeds WaveConfig.interval_secs:
    - mark WaveState due and reset elapsed
```

### 5.3 `where:` symbol bindings (optional)

A `where:` line at column 0 ends the bullets and opens a binding block:

```
where:
  N = 8 @ WaveConfig.base_count
```

Use literal `X` / `Y` / `N` in prose for abstract quantities and bind them here.

### 5.4 Cross-spec references (`@ spec`)

A module line, **system entry**, behavior step, `## api` entry, or invariant may
end with `@ spec <unit>` — "more specification under that unit". At most one per
line, always the last annotation (after any `@ path`).

A ref may name a **member** of the target: `@ spec <unit>::<function>`, or
`@ spec <unit>::<Type>::<method>` for a method — one `## api` entry of a
`kind: module` spec ([§11](#11-the-module-dialect-kind-module)). Unit segments
follow the header ident rule ([§2](#2-header)); member segments are
`[A-Za-z_][A-Za-z0-9_]*` and there are at most two of them. A malformed ref is
an error.

`::` rather than `.` or `#`: `.` already means field access in behavior prose
([§5.2](#52-cross-references)) and is a legal ident character, and `::` extends
to `Type::method` without a second rule. A `use` line names a spec file and
imports **types** out of it, so a member on its ref is a `malformed-import`
error ([§4.5](#45-importing-types-use)).

**Two severities, because the two failures differ.** A well-formed ref whose
target **file** does not exist is a warning (`unresolved-spec-ref`) — the target
spec may be planned but not written yet. A ref whose file *does* exist but
defines no such member is an **error** (`unknown-spec-member`): the file is right
there to check the name against, so the name is simply wrong. That is what makes
a member ref worth navigating by — and it means whatever writes an overview's
member refs must write the module specs in the same pass.

The LSP completes members after `@ spec <unit>::`, jumps to the named function's
own line on goto-definition, hovers a member ref as that one entry (signature
plus bullets), and answers find-references from an `## api` entry's own name with
"who calls this function?". A member request matches only the refs naming that
member; a unit request matches every ref into the unit.

## 6. `## invariants`

Flat `-` bullets, each a checkable property: a cap, an ordering guarantee, an
idempotency claim, a component-exclusivity rule.

## 7. Kinds and the section schema

### 7.1 Kinds

| `kind:` | Models |
|---|---|
| `endpoint` | One HTTP endpoint |
| `workflow` | One message/queue workflow |
| `job` | One scheduled job |
| `plugin` | One Bevy `Plugin` — what it registers, and in what order |
| `system` | One Bevy system, split out of a plugin |
| `module` | One module's public surface, function by function ([§11](#11-the-module-dialect-kind-module)) |

### 7.2 Which sections each kind takes

| kind | required | also allowed | rejected |
|---|---|---|---|
| `endpoint` / `workflow` / `job` | `modules`, `data`, `behavior`, `invariants` | — | every Bevy section |
| `plugin` | `systems`, `behavior`, `invariants` | `modules`, `data`, `components`, `resources`, `events` | `access` |
| `system` | `access`, `behavior`, `invariants` | `modules`, `data`, `components`, `resources`, `events` | `systems` |
| `module` | `modules`, `data`, `api`, `invariants` | — | `behavior` and every Bevy section |

A required section that is absent is a `missing-section` error. A grammatical
section that is wrong for the kind is a `section-not-allowed-for-kind` error —
it still parses, so the LSP keeps working mid-edit.

Note the asymmetry: the classic kinds must declare all four sections (v1
behaviour, preserved), while the Bevy sections are simply **omitted** when they
have nothing to say. A plugin that registers no events has no `## events`, and
one whose every shape has a role has no `## data`.

`## data` is the one section both dialects take, and it means something
different in each ([§4](#4--data), [§4.6](#46--data--in-the-bevy-dialect)) — the
`in:` / `out:` lines are what stay dialect-specific, not the section header.

When `kind:` is missing or unparsable the classic four-section schema applies,
so a file being typed top-down does not first look like a broken plugin.

### 7.3 Plugin per file, or system per file?

Both. Start with one `kind: plugin` file per feature, systems described inline.
When one system's behavior outgrows a few nested bullets, split it into its own
`kind: system` file and point at it from the plugin's system entry:

```
spawn_enemies — spawns the next wave at each spawn point @ spec spawn-enemies
```

The split file imports the shapes it touches rather than redeclaring them
([§4.5](#45-importing-types-use)).

## 8. The Bevy sections

### 8.1 `## components` / `## resources` / `## events`

Body grammar is [§4](#4--data)'s type grammar — `struct`, `enum`, `use` — minus
`in:`/`out:`, plus one addition:

```
marker Spawner
```

`marker Name` declares a **zero-sized component**. It is legal only in
`## components`; markers are common enough in Bevy that spelling one as an empty
struct reads badly.

**The section assigns the role.** A type declared *or imported* into
`## components` is a component in this file, one in `## resources` is a resource,
one in `## events` is an event, and one in `## data` has no role at all
([§4.6](#46--data--in-the-bevy-dialect)). Role is what makes an access list
checkable, and it is why the same name may not be declared in two of these
sections (`duplicate-type`).

For a type that references real code ([§4.7](#47-referencing-a-rust-type--path))
the section's claim is checked against the code's `#[derive(…)]` — the one place
where "the section assigns the role" can be wrong out loud, and the reason a
reference beats a copy.

Put a type here only if a system actually accesses it as a component, resource,
or event. A shape that merely *appears inside* one of those — a field type, an
enum it is built from — goes in `## data`.

```
## components
struct Wave {
  index: u32,
  remaining: u32,
}
marker Spawner

## resources
struct WaveState {
  current: u32,
  due: bool,
}

## events
struct WaveCleared {
  index: u32,
}
```

**Bevy's own types need no declaration.** `Transform`, `Time`, `AssetServer`,
`AppExit` and their friends are known to the parser with their roles; restating
them in every spec would be noise. The lists live in `mml-parser/src/validate.rs`
(`BUILTIN_COMPONENTS`, `BUILTIN_RESOURCES`, `BUILTIN_EVENTS`) — extend them
rather than declaring an engine type locally. A local declaration wins over a
built-in of the same name.

### 8.2 `## systems`

Zero or more entries. Each is a declaration line at column 0 —

```
<snake_name> — <responsibility> [@ <path>] [@ spec <unit>]
```

— optionally followed by a 2-space-indented **access block**. The line shape is
exactly a module line ([§3](#3--modules)); the schedule and everything else Bevy
lives in the block, so `@` keeps its two existing meanings and no more.

Names should be snake_case, matching the Rust fn (a warning otherwise).
Duplicate names are an error.

```
## systems
tick_wave_timer — advances the wave clock @ src/waves/timer.rs
  schedule: FixedUpdate
  reads:    WaveConfig, Time
  writes:   WaveState
spawn_enemies — spawns the next wave at each spawn point @ spec spawn-enemies
  schedule: FixedUpdate
  after:    tick_wave_timer
  run_if:   WaveState.due
  query:    Transform + Spawner
  spawns:   EnemyHealth, Wave
```

### 8.3 The access block

A closed set of `key: value` lines. Each key may appear once.

| Key | Value | Meaning |
|---|---|---|
| `schedule:` | one label | Which schedule it runs in. **Required.** |
| `set:` | idents | System-set membership (declares the set). |
| `after:` / `before:` | idents | Ordering against systems or sets. |
| `run_if:` | free text | Run condition. |
| `query:` | `A + B - C` | Matched component set: `+` requires, `-` excludes. |
| `reads:` / `writes:` | idents | Components and resources accessed. |
| `spawns:` / `despawns:` | idents | Components on entities created / destroyed. |
| `emits:` / `consumes:` | idents | Events written / read. |

**Roles are enforced.** `query:`, `spawns:`, `despawns:` take components;
`emits:`, `consumes:` take events; `reads:` and `writes:` take either a component
or a resource. A name in the wrong place is `access-role-mismatch`; a name that
resolves to nothing at all is `undefined-access-item`.

`query:` names the matched set; a component also listed in `writes:` is mutably
accessed. Resources appear only under `reads:` / `writes:`.

**Schedules.** The built-in labels are `PreStartup`, `Startup`, `PostStartup`,
`First`, `PreUpdate`, `StateTransition`, `RunFixedMainLoop`, `FixedFirst`,
`FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate`, `FixedLast`, `Update`,
`PostUpdate`, `Last`, `Main`, `SpawnScene`, plus `OnEnter(X)` / `OnExit(X)` /
`OnTransition(…)` recognised by shape. Anything else is a **warning**, not an
error — custom schedules are legitimate.

**Ordering refs.** `after:` / `before:` names should match a system or a `set:`
declared in the same file; otherwise a **warning**. Only checked for
`kind: plugin` — a standalone `kind: system` file legitimately orders against
systems specced elsewhere, so its refs are never flagged.

### 8.4 `## access`

The `kind: system` form: exactly the [§8.3](#83-the-access-block) block, written
unindented as its own section.

```
## access
schedule: FixedUpdate
set:      WaveSpawning
after:    tick_wave_timer
query:    Transform + Spawner - SpawnBudget
reads:    WaveConfig, Time
writes:   WaveState
spawns:   EnemyHealth, Wave
```

## 9. Diagnostics

Stable codes, matched exactly by the skills and the LSP. Defined in
`mml-parser/src/diagnostic.rs`.

**Header:** `missing-header-key`, `bad-kind`, `bad-method`, `unknown-header-key`,
`malformed-header-line`.

**Structure:** `missing-section`, `duplicate-section`, `section-out-of-order`,
`unknown-section`, `section-not-allowed-for-kind`, `tab-indent`.

**Modules:** `malformed-module`, `module-name-not-pascal`.

**Data:** `malformed-struct`, `malformed-field`, `malformed-type`,
`bad-param-source`, `unterminated-block`, plus `malformed-marker` for a `marker`
line in `## data` (a marker is a component; see [§4.6](#46--data--in-the-bevy-dialect)).

**Imports:** `malformed-import`, `duplicate-import`, `unresolved-import` (warn),
`malformed-extern-import`.

**Rust references** ([§4.7](#47-referencing-a-rust-type--path)):
`missing-type-body`, `malformed-type-path`, plus the filesystem-aware
`rust-type-not-found` (warn), `rust-role-mismatch` (warn),
`rust-shape-mismatch` (warn), `rust-member-unknown` (warn).

**Behavior:** `dangling-else`, `dangling-on-error`, `malformed-binding`.

**Cross-spec:** `malformed-spec-ref`, `duplicate-spec-ref`, `unresolved-spec-ref`
(warn), `unknown-spec-member` (error, filesystem-aware only).

**Api entries** ([§11](#11-the-module-dialect-kind-module)):
`malformed-api-entry`, `duplicate-api-entry`, `empty-api` (warn).

**Source linkage:** `unresolved-source-path` (warn).

**Bevy:** `malformed-system`, `system-name-not-snake` (warn), `duplicate-system`,
`malformed-marker`, `malformed-access`, `unknown-access-key`,
`duplicate-access-key`, `missing-schedule`, `unknown-schedule` (warn),
`undefined-access-item`, `access-role-mismatch`, `unresolved-system-ref` (warn).

**Semantic:** `undefined-type`, `duplicate-type`, `duplicate-field`,
`duplicate-module`.

**Change annotations** ([§10](#10-change-annotations)):
`empty-change-removal` (warn), `empty-change-addition` (warn),
`redundant-change-bullet` (warn), `unrecoverable-baseline` (warn). All four are
warnings and none is an error — an annotated spec is a valid spec, and these
report a delta that has stopped describing a real change.

The `unresolved-*` codes need a filesystem and are emitted only by the
filesystem-aware consumers (the LSP and `hobby-mml-parser --lint`),
never by the pure parse/validate passes.

## 10. Change annotations

An `.hmml` describes one unit **as it is**. When a task changes that unit, the
change is written *on top of* that version rather than replacing it: the file
keeps the **baseline** — the version the change starts from — and two comment
markers, `# +` and `# -`, say what the change adds and removes.

This is a **convention, not grammar.** Both markers are ordinary comments
([§1](#1-file-shape)), so an annotated spec parses and validates exactly like any
other, and a tool that ignores comments simply sees the baseline.

- `# - <text>` — a line the change **removes**. Comment the original out **in
  place**, keeping its text verbatim at its original indentation; the marker's
  `-` stands in for the bullet the line had, where it had one.
- `# +` — a marker introducing **added** content: the line directly beneath it,
  together with any lines nested deeper than that line (its children). Several
  added siblings each take their own marker — one marker covers one line's worth
  of addition, the way a `+` in a diff does, so what is added is never inferred
  from where a run happens to stop.

```
## behavior
- for each Spawner:
  # - spawn one enemy per tick
  # +
  - spawn WaveConfig.rate enemies per tick
  # +
  - emit a WaveSpawned event
```

A marker covering a line with children takes the whole subtree, so nested work
needs one marker, not one per line:

```
# +
- while the spawner's share is not exhausted:
  - take the next SpawnPoint from WaveConfig
```

A `struct` / `enum` head is covered through its closing brace, since the brace
sits at the head's own indentation and the subtree rule would stop one line
short of it.

The rules that make the convention worth having:

1. **Baseline recoverability.** Dropping every `# +` block and un-commenting
   every `# -` line must reproduce the baseline **exactly**. So a baseline line
   is never deleted or reworded in place — it is commented out with `# -` and
   the replacement added with `# +`. The file alone therefore carries both
   versions and the whole delta, with no git archaeology, and the delta survives
   being committed. This is a **checkable** property, and
   `hobby-mml-parser` / the LSP check it: reconstructing the baseline and
   finding that it no longer parses or validates is `unrecoverable-baseline`.
2. **A modification is a removal plus an addition** — `# -` the old line, `# +`
   the new one. There is no in-place edit.
3. **One task's change at a time.** Annotations describe one change, not a
   history of them; a spec is never a changelog. The next task starts from a
   fresh extract of the code in its own task directory, so a second change never
   lands on top of a first one's markers.
4. **The annotations stay.** Nothing strips them once the implementation
   matches: the spec is the record of what that change was, and un-annotating it
   would leave a file indistinguishable from a plain baseline. The unannotated
   lines are the version the change started from — not, after it lands, the code
   as it is today.

Annotations apply in **every** section, and to every line shape: a removed
struct field, an added component, a new `## systems` entry with its whole access
block, a changed behavior step, an added invariant. In `## api`
([§11](#11-the-module-dialect-kind-module)) a marker on a `fn` line covers the
**whole entry**, its nested bullets included, the way it covers a control-flow
step's subtree — so a changed signature is a `# -` of the old `fn` line plus a
`# +` of the new one with its bullets beneath. There is no second spelling
for any of them — including a `@ <path>` type body, whose v2.2 inline `+` / `-`
markers §10 supersedes ([§4.7](#47-referencing-a-rust-type--path)).

In the tooling: `/hmml-extract --against <ref>` derives the delta from a
branch's diff (`/task:build` runs it once the change is implemented, before
GATE 2); plain extraction writes no annotations, and a human may annotate by
hand; `/hmml-review` pressure-tests a hand-written delta against the rest;
`/hmml-validate` expects `# +` claims to be present in the code and `# -`
behaviour to be gone from it.

## 11. The module dialect (`kind: module`)

The behavioural kinds describe a **flow**: a handful of collaborators and a
dozen steps. A `kind: module` spec is the layer underneath — **one module's
public surface**, function by function, linked from the overview's `## modules`
line.

```
system: bevy-survivor
unit: wave-table
kind: module
source:
  repo: bevy_survivor
  paths:
    - src/waves/table.rs

## modules
WaveRng — draws the spawn offsets @ src/waves/rng.rs @ spec wave-rng

## data
use wave-rng: WaveRng
struct WaveEntry {
  index: u32,
  count: u32,
}

## api
fn entry_for(index: u32) -> Option<WaveEntry> @ src/waves/table.rs:31
  - look the index up in the table
  - if the index is past the last authored wave:
    - answer nothing, and let the caller scale the last entry

fn WaveEntry::offsets(rng: WaveRng) -> Vec<Vec2>
  - for each spawn slot in the entry:
    - draw a jitter for it @ spec wave-rng::WaveRng::next_u32
    - place the slot on the entry's pattern, displaced by the jitter

## invariants
- entry_for never answers an entry whose index differs from the one asked for
```

### 11.1 Sections

| Section | Holds |
|---|---|
| `## modules` | the collaborators this module **calls**, each linkable with `@ spec` |
| `## data` | the public types this module **owns** — no `in:` / `out:` ([§4.4](#44-in--out)) |
| `## api` | one entry per public function, its behaviour nested underneath |
| `## invariants` | properties of the module, as [§6](#6--invariants) |

`## api` sits in the slot a behavioural kind writes `## behavior` in. Writing the
other dialect's spelling — `## behavior` on a module, `## api` on anything else —
is a `section-not-allowed-for-kind` error: the two sections hold structurally
different content, so reinterpreting one as the other would silently discard it.

A module is **not** a Bevy kind. It declares no components, resources, events,
systems or access block; those describe what the World holds and who touches it,
and a module is a body of code with an API.

### 11.2 The `fn` entry

```
fn [<Type>::]<name>(<param>: <Type>, …) [-> <Type>] [@ <path>] [@ spec <ref>]
  - <bullet>
  - <bullet>
```

- **`fn`** opens the line at **column 0**. Both halves of that are checked: a
  bullet not nested under an entry is a `malformed-api-entry` error (every bullet
  in `## api` describes some function), and so is a `fn` line indented *under*
  another entry — otherwise the step parser takes it for a bullet and the
  function vanishes from the api with nothing to say so.
- **`<name>`** is snake_case; a leading **`<Type>::`** makes the entry a
  **method** on a type this module's `## data` declares or imports. The module
  itself is never the receiver — a function of the module is a plain `fn`,
  because the file *is* the module. An entry is addressed by its `name`, or
  `Type::name` for a method ([§5.4](#54-cross-spec-references--spec)), and those
  must be unique within the file (`duplicate-api-entry`).
- **Parameters** are `name: Type`, comma-separated. **`-> Type`** is optional (a
  function that returns nothing omits it). Every type resolves per
  [§4.3](#43-type-expressions).
- **`@ path`** (optional) locates the function in the source, repo-relative and
  optionally `:line`, exactly as a module's `@ path` does ([§3](#3--modules)) —
  so goto-definition reaches the real function.
- **`@ spec <ref>`** stays the last annotation on the line.
- **The bullets** are ordinary steps ([§5](#5--behavior)): the same closed
  control-flow set, the same 2-space nesting, the same trailing `@ spec` refs. A
  bullet that calls a collaborator carries `@ spec <unit>::<function>` at the
  step where the call happens, which is what makes the call graph readable from
  the spec.
- An `## api` section with no entries is a warning (`empty-api`): a module spec's
  whole job is to carry the public surface.

### 11.3 What a module spec deliberately leaves out

- **Private functions.** The bullets describe what the *public* functions do; if
  the implementation wants a private helper, it factors one out.
  So "the code resembles the spec" is a claim about behaviour and about the
  public surface — **not** that every bullet is a statement or every nested tree
  its own function. Conformance checking tolerates helper extraction rather than
  reporting it as drift.
- **Module state and injected dependencies.** `## modules` names the
  collaborators; how they are held is implementation.
- **`where:` bindings.** They belong to `## behavior`, so a threshold inside an
  `## api` bullet is not bindable.
- **Completeness is not lintable.** "Every public function is present" is an
  authoring rule ([§13](#13-authoring-checklist)), not a parser check — the
  parser cannot see the code. A public function missing from the spec is reported
  by `/hmml-validate` as a warning.

### 11.4 Which module gets a spec

The ones the overview's `## modules` section lists **that have a public surface**
— a module spec is 1:1 with a behavioural collaborator, not with a file, type, or
directory. That keeps the overview's `@ spec` module-line ref as the single spine
between the two layers, and `/hmml-extract` writes both in one pass so a member
ref never dangles ([§5.4](#54-cross-spec-references--spec)).

A collaborator that exposes nothing gets **no** spec and no `@ spec` ref: there is
nothing for `## api` to hold, and a spec written anyway would be an empty one
(`empty-api`). That is the normal case for a binary crate, where every function
is private to it, and for a collaborator that is one private helper of the unit —
the implementation §11.3 leaves out, one level up. It still belongs in
`## modules`, because it still does work in the behaviour.

## 12. Worked examples

The canonical, always-parsing examples are the parser's fixtures — they are
compiled into the test suite, so they cannot drift:

| File | Shows |
|---|---|
| `mml-parser/tests/fixtures/examples/enemy-waves.hmml` | `kind: plugin` — all three world sections, a role-less `## data`, three systems, ordering, a `marker`, a `where:` binding |
| `mml-parser/tests/fixtures/examples/spawn-enemies.hmml` | `kind: system` — `## access`, imports across specs into both `## data` and a world section, a `-` query exclusion, a Rust reference with a partial body carrying a `# +` change annotation, and a `use extern` import |
| `mml-parser/tests/fixtures/examples/get-map-tile.hmml` | `kind: endpoint` — `method`, param sources |
| `mml-parser/tests/fixtures/examples/mesh-terrain-chunks.hmml` | `kind: workflow` — `Queue`/`Stream` in/out |
| `mml-parser/tests/fixtures/examples/shared-data-imports.hmml` | `use` type imports |
| `mml-parser/tests/fixtures/examples/cross-spec-refs.hmml` | `@ spec` annotations |
| `mml-parser/tests/fixtures/examples/wave-table.hmml` | `kind: module` — `## api`, a method entry, a member ref into a second module spec |
| `mml-parser/tests/fixtures/examples/wave-rng.hmml` | `kind: module` — the target of that member ref, entries carrying `@ path` |

## 13. Authoring checklist

- Filename equals the `unit:` header value, plus `.hmml`.
- `kind:` set, and only that kind's sections present — in canonical order.
- Indentation is 2 spaces, no tabs.
- **Classic kinds:** every type in a field / `in:` / `out:` resolves.
- **`kind: module`:** `## api` lists **every** public function, standalone or
  method; each entry is a real signature whose types resolve; a bullet that calls
  a collaborator carries `@ spec <unit>::<function>`; `## data` holds the public
  types this module owns, and the units that use them `use`-import from here
  rather than redeclaring them; no `in:` / `out:`, no private functions, no
  module state — those are implementation.
- **Bevy kinds:** every access-list name resolves to a declared, imported, or
  built-in item, in a role that fits its key; every system has a `schedule:`.
  Shapes no system accesses directly live in `## data`, not in a world section.
- **A type this unit does not own is referenced, not restated** — `@ <path>` for
  code in this checkout ([§4.7](#47-referencing-a-rust-type--path)),
  `use extern` for a dependency
  ([§4.8](#48-importing-from-a-dependency-use-extern)). A referenced type's body
  lists only what changes.
- **A change in flight is annotated, not applied in place** — every baseline
  line either untouched or commented out with `# -`, every addition under its
  own `# +` ([§10](#10-change-annotations)).
- Behavior control flow uses only the closed keyword set.
- Declared names are written verbatim in prose so they cross-link.
- Invariants are checkable claims, not restatements of the behavior.
- `hobby-mml-parser <file> --lint` exits clean.
