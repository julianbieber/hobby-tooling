---
title: "Hobby HMML — language specification (v2.1, Bevy dialect)"
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
head, one review, one diff. You edit the `.hmml`; Claude translates spec → code.

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

## 1. File shape

An `.hmml` file is **line-oriented, UTF-8**, and has two parts in a fixed order:

1. A **header** (YAML-ish key/value block) at the top.
2. A sequence of **`##` sections**, in canonical order.

The canonical section order across both dialects is:

```
modules, data, components, resources, events, systems, access, behavior, invariants
```

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
| `kind` | yes | One of `endpoint`, `workflow`, `job`, `plugin`, `system`. |
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

### 4.2 Enums

Unit variants only, no payloads: `enum TileFormat { png, webp }`.

### 4.3 Type expressions

An identifier with optional generic arguments. A type must resolve to one of:

- **primitives** — `String`, `bool`, `i8`…`i64`, `u8`…`u64`, `f32`, `f64`
- **opaque atoms** — `Uuid`, `Money`, `DateTime`, `Entity`, `Duration`, `Vec2`,
  `Vec3`, `Vec4`, `Quat`, `Color`, `Timer`, `Rect`
- **containers** — `Option<T>`, `Vec<T>`, `Map<K,V>`, `Queue<T>`, `Stream<T>`,
  `Handle<T>`
- a struct/enum/marker defined or imported in this file

Anything else is an `undefined-type` error.

### 4.4 `in:` / `out:`

```
in:  TileRequest
out: Tile
```

Each is a single type expression. **Illegal in the Bevy dialect** — a Bevy unit's
interface is its access block, not an input and an output.

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

A module line, **system entry**, behavior step, or invariant may end with
`@ spec <unit>` — "more specification under that unit". At most one per line,
always the last annotation (after any `@ path`). Unresolved target → warning.

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

### 7.2 Which sections each kind takes

| kind | required | also allowed | rejected |
|---|---|---|---|
| `endpoint` / `workflow` / `job` | `modules`, `data`, `behavior`, `invariants` | — | every Bevy section |
| `plugin` | `systems`, `behavior`, `invariants` | `modules`, `data`, `components`, `resources`, `events` | `access` |
| `system` | `access`, `behavior`, `invariants` | `modules`, `data`, `components`, `resources`, `events` | `systems` |

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

**Imports:** `malformed-import`, `duplicate-import`, `unresolved-import` (warn).

**Behavior:** `dangling-else`, `dangling-on-error`, `malformed-binding`.

**Cross-spec:** `malformed-spec-ref`, `duplicate-spec-ref`, `unresolved-spec-ref`
(warn).

**Source linkage:** `unresolved-source-path` (warn).

**Bevy:** `malformed-system`, `system-name-not-snake` (warn), `duplicate-system`,
`malformed-marker`, `malformed-access`, `unknown-access-key`,
`duplicate-access-key`, `missing-schedule`, `unknown-schedule` (warn),
`undefined-access-item`, `access-role-mismatch`, `unresolved-system-ref` (warn).

**Semantic:** `undefined-type`, `duplicate-type`, `duplicate-field`,
`duplicate-module`.

The `unresolved-*` codes need a filesystem and are emitted only by the
filesystem-aware consumers (the LSP, `hobby-mml-parser --lint`, the index tool),
never by the pure parse/validate passes.

## 10. Worked examples

The canonical, always-parsing examples are the parser's fixtures — they are
compiled into the test suite, so they cannot drift:

| File | Shows |
|---|---|
| `mml-parser/tests/fixtures/examples/enemy-waves.hmml` | `kind: plugin` — all three world sections, a role-less `## data`, three systems, ordering, a `marker`, a `where:` binding |
| `mml-parser/tests/fixtures/examples/spawn-enemies.hmml` | `kind: system` — `## access`, imports across specs into both `## data` and a world section, a `-` query exclusion |
| `mml-parser/tests/fixtures/examples/get-map-tile.hmml` | `kind: endpoint` — `method`, param sources |
| `mml-parser/tests/fixtures/examples/mesh-terrain-chunks.hmml` | `kind: workflow` — `Queue`/`Stream` in/out |
| `mml-parser/tests/fixtures/examples/shared-data-imports.hmml` | `use` type imports |
| `mml-parser/tests/fixtures/examples/cross-spec-refs.hmml` | `@ spec` annotations |

## 11. Authoring checklist

- Filename equals the `unit:` header value, plus `.hmml`.
- `kind:` set, and only that kind's sections present — in canonical order.
- Indentation is 2 spaces, no tabs.
- **Classic kinds:** every type in a field / `in:` / `out:` resolves.
- **Bevy kinds:** every access-list name resolves to a declared, imported, or
  built-in item, in a role that fits its key; every system has a `schedule:`.
  Shapes no system accesses directly live in `## data`, not in a world section.
- Behavior control flow uses only the closed keyword set.
- Declared names are written verbatim in prose so they cross-link.
- Invariants are checkable claims, not restatements of the behavior.
- `hobby-mml-parser <file> --lint` exits clean.
