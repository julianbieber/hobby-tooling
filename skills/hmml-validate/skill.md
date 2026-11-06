---
description: Validate the implementation against its current .hmml spec — report where code has drifted from the mental model, without editing anything
allowed-tools: Read, Grep, Glob, Bash(git -C * diff:*), Bash(git -C * show:*), Bash(git -C * cat-file:*), Bash(git -C * ls-files:*), Bash(git -C * rev-parse:*), Bash(git -C * status:*), Bash(git -C * worktree list:*), Bash(git -C * log:*), Bash(git diff:*), Bash(ls:*), Bash(pwd:*), Bash(cargo *), Bash(just *), Bash(mise exec *), Bash(hobby-mml-parser:*), mcp__tree-extract__build_graph, mcp__tree-extract__list_modules, mcp__tree-extract__get_neighbors, mcp__tree-extract__call_path, mcp__tree-extract__get_symbol, mcp__tree-extract__search_symbols, mcp__lsp-mcp__document_symbol, mcp__lsp-mcp__find_references, mcp__lsp-mcp__goto_definition, mcp__lsp-mcp__hover
---

Validate that the **implementation still matches its `.hmml` spec** — a read-only
**conformance / drift check**. `hmml-extract` turns code into a `.hmml`, the
human edits it or the code moves on — and over time code and spec drift apart. This skill answers **"does the code still do
what the current spec says?"** and reports *where and which way* they have
diverged.

It **never edits** — not the code, not the spec. Deciding which side is right is
the human's call, and that is the point: if the spec is the intent, the code is
changed to match; if the code is now the truth,
re-run `hmml-extract` to refresh the spec. This skill only tells you
*that* they diverged and *how*.

Read `~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/grammar.md` (the authoritative
hobby grammar, v2 — **not** the work one) before your first run if you are
unsure of any rule below.

## Drift vocabulary

Every finding is one of four verdicts. Use them exactly:

- **CONFORMS** — the spec claim is present in the code (allowing for reworded-
  but-equivalent prose and the grammar's deliberate lossiness).
- **SPEC-AHEAD** — the spec asserts something the code does **not** do (a
  behavior step with no code, an invariant with no enforcement, a struct field
  or module `@ path` that no longer exists). *This is usually the real drift* —
  either the code changed and the spec was not updated, or a spec edit was never
  implemented.
- **CODE-AHEAD** — the code does something the spec does **not** mention. The
  spec is a **lossy mental-model view by design**, so most undescribed code is
  expected and **must not be reported**. Flag CODE-AHEAD *only* for a
  behaviorally significant collaborator or branch (a new module doing real work,
  a new enum variant, a new failure path) — never routine helpers, logging, or
  fields the spec intentionally omitted.
- **CANNOT-VERIFY** — the claim needs a runtime/dynamic check you could not run
  (missing test infra, no discoverable enforcement site). Say so; do not guess.

**Referenced types are where drift shows up first** (grammar §4.7). A
declaration ending in `@ <path>` claims a type exists in real code, so it is
checkable in ways prose is not — and `hobby-mml-parser --lint` finds most of it
for you. Map each finding to a verdict:

- `rust-type-not-found` — the type was renamed or moved → **SPEC-AHEAD**.
- `rust-role-mismatch` — its `#[derive(…)]` no longer matches the section (a
  component became a resource) → **SPEC-AHEAD**, and a real design change worth
  calling out, not a typo.
- `rust-member-unknown` — a field the spec asserts is gone → **SPEC-AHEAD**.
- A member under a `# +` / `# -` annotation is part of the change, so it is read
  by the annotation rules below, not by these. (A legacy inline `+`/`-` member,
  grammar §4.7, is read the same way: applied → **CONFORMS** with a hygiene note
  that it can be rewritten as a §10 marker; not applied → outstanding work, not
  drift. The linter is silent on both, so this one is yours to read.)
- A field the code has and the reference body omits is **not** CODE-AHEAD. A
  reference body is partial by definition, so omission carries no claim.

**Annotated specs.** A spec may carry a change as annotations on top of the
baseline — derived from a diff by `/hmml-extract --against`, or written by hand — `# - <text>` for what the change removes, `# +` for the
block it adds (grammar §10). Read them; they change what each verdict *means*:

- an unannotated line is a claim about the code as it is → the verdicts above
  apply unchanged;
- a `# +` claim is intent. Before implementation, its absence from the code is
  **expected** — report it as SPEC-AHEAD and say plainly that it is not yet
  implemented, not a code bug. After implementation it must be present, and
  its absence is real unfinished work;
- a `# -` line is behaviour the change **removes**. It is a comment, so it is not
  a claim about today's code: after implementation, finding that behaviour still
  in the code is a **failure to conform** — call it out as "removal not applied",
  not as CODE-AHEAD.

Open the report with a one-line census (`N` removals, `M` additions annotated)
and split the findings into *baseline* and *delta*, so "the change landed" and
"the rest of the unit still holds" are answerable separately. Never strip the
annotations — they stay in the spec after the change lands, so it keeps
representing the change.

## Instructions

1. **Identify the spec and fix the reference version.** Specs live **per task**
   in the notes repo, directly in the task's notes directory (no `<system>`
   subdirectory). Detect `{project}`/`{topic}`/`{task}` the same way `journal:resume`
   does: from the worktree path `~/fun_worktrees/{project}/{topic}/{task}`, they
   are the first, second and third dirs (`{task}` verbatim, including any blurb
   suffix); fall back to the git branch name if the path is not under
   `~/fun_worktrees`. Resolve the human's pointer (a path, or a bare `unit`) to
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml`
   (the filename is the `unit:` header plus `.hmml`; the `system:` header is an
   optional label only) and confirm it exists. "The current version of the hmml"
   is the **on-disk** spec (the file as it is now). Record its git state and let
   it colour the report:

   ```sh
   git -C ~/fun_repos/hobby-mimisbrunnr status --short -- notes/{project}/{topic}/{task}/<unit>.hmml
   git -C ~/fun_repos/hobby-mimisbrunnr diff HEAD -- notes/{project}/{topic}/{task}/<unit>.hmml   # pending spec edits
   ```

   - **Clean (matches HEAD)** → you are validating code against the committed
     spec. The normal case.
   - **Uncommitted spec edits present** → you are validating against a spec that
     *itself* has a pending change. That change is almost always a **planned**
     edit not yet in code — expect it to show up as SPEC-AHEAD, and say
     explicitly that the fix is its implementation, not a code bug.

   Then check the spec is **structurally well-formed**: run
   `hobby-mml-parser <path> --lint` (grammar + linkage — an `unresolved-source-path`
   warning is direct drift evidence, see step 2); if the binary is missing,
   validate by eye against the cheat-sheet. A spec that does not parse cannot be
   conformance-checked — report that and stop. If it ends in an ambiguous
   pointer, ask **one** clarifying question rather than guessing.

2. **Resolve the source (read-only).** `source.repo` resolves to
   `~/fun_repos/<repo>`; if the header has a `base:` key, `source.paths` are
   relative to that subdirectory. Reading the shared `~/fun_repos/<repo>`
   checkout is **fine here** — validate never writes — but if the human is
   working in a worktree of that repo, prefer it so HEAD matches what they see.
   Verify each `source.paths` entry resolves to a real file; a broken link is
   itself a **SPEC-AHEAD** conformance failure (record it under modules/linkage).

3. **Check each section, routing each line to its code.** Route each
   `## modules` line via its **`@ path`** anchor, then pin symbols with
   `tree-extract` (`build_graph` if `.project-graph.json` is absent, then
   `get_symbol` / `get_neighbors` / `call_path`) and `lsp-mcp`
   (`document_symbol`, `goto_definition`, `find_references`, `hover`). Assign a
   verdict per claim:

   - **`## modules`** — each `@ path` resolves and the file's role still matches
     the stated responsibility. A deleted/renamed/gutted file, or a module whose
     responsibility no longer describes the code, is **SPEC-AHEAD**.
   - **`## data`** — each `struct`/`enum` maps to a real type; compare fields
     (added / removed / renamed / retyped) and enum variants to the code's
     closed value set. **Respect deliberate lossiness**: never flag opaque-atom
     internals (`Money`, `Uuid`) or intentionally-omitted fields. Flag only a
     field/variant the spec *asserts* that the code no longer has
     (**SPEC-AHEAD**), or a code variant/field significant to behavior that the
     spec lacks (**CODE-AHEAD**). A `struct` marked with a `# synthesized:`
     comment has **no real code type by design** (extract emits it for
     loose-param inputs) — never flag it SPEC-AHEAD for not existing.
   - **`use` imports** (any section that takes them) — each `use unit: Type` must resolve, against
     the sibling `<unit>.hmml` in the same task directory, to a type that owning
     unit's spec actually defines (an `unresolved-import` warning means it does
     not — the owning spec was renamed, the type removed, or the import points
     at the wrong unit). An imported type is checked against the code the same
     way a local `struct` is (its real code type is the owning unit's), so a
     drifted shared type surfaces once, on the owner. Flag an import of a type
     the code no longer shares as **SPEC-AHEAD**.
   - **`## behavior`** — for each step, confirm the described decision/action is
     still present in the routed code. A step with no corresponding code is
     **SPEC-AHEAD**; a behaviorally significant code branch with no step is
     **CODE-AHEAD**. Honor **altitude**: a reworded-but-equivalent step
     **CONFORMS** — do not nitpick prose. Watch specifically for a step that
     **contradicts** the code (spec says `<`, code does `<=`; spec says "reply
     502", code replies 400) — that is the highest-value SPEC-AHEAD finding.
   - **`## components` / `## resources` / `## events`** (Bevy) — each declared
     type maps to a real type carrying the **matching derive**: a component to
     `#[derive(Component)]`, a resource to `#[derive(Resource)]`, an event to
     `#[derive(Message)]` (or `Event` / `BufferedEvent` / `EntityEvent` in code
     on an older Bevy — grammar §4.7 treats them as one role). A **role mismatch is drift even when the type exists** —
     a spec resource whose code type derives `Component` is **SPEC-AHEAD**, and
     it is the highest-value finding in this section, because the spec's whole
     access model is built on roles. A `marker Name` must be a unit struct; a
     marker that has grown fields in code is **CODE-AHEAD**. Fields compare as in
     `## data`. Never flag a Bevy built-in (`Transform`, `Time`, …) for having no
     local declaration — it is not supposed to have one.
   - **`## data`** (Bevy) — the same field/variant comparison, plus one role
     claim of its own: a `## data` type asserts the code type carries **none** of
     `Component` / `Resource` / `Message` (or `Event`). A code type that has grown one is
     **CODE-AHEAD** (it became part of the world; the spec should move it to that
     section). The converse — a world-section type whose code derive is gone —
     is the SPEC-AHEAD case above. Do not flag a `## data` type for being absent
     from every access list; that is exactly what having no role means.
   - **`## systems`** (Bevy) — for each entry, find the Rust fn (via `@ path`
     where present) **and its registration**, then compare the access block
     against the fn's real signature:
     - `schedule:` vs the `app.add_systems(<Schedule>, …)` the fn is registered
       in. A system in the spec with **no registration at all** is SPEC-AHEAD and
       worth calling out loudly — it means dead code that never runs.
     - `after:` / `before:` / `set:` vs `.after(..)` / `.before(..)` /
       `.in_set(..)` and any `.chain()` position; `run_if:` vs `.run_if(..)`.
     - `query:` vs the `Query<…>` params: `+` terms against the fetched and
       `With<..>` set, `-` terms against `Without<..>`.
     - `reads:` / `writes:` vs `Res<T>`/`ResMut<T>` and `&T`/`&mut T`. **A spec
       `reads:` where the code takes `&mut`/`ResMut` is SPEC-AHEAD** — the spec
       understates the system's access, which is exactly the claim a reader would
       rely on when reasoning about conflicts.
     - `spawns:` / `despawns:` vs `commands.spawn`/`despawn`; `emits:` /
       `consumes:` vs `MessageWriter<E>` / `MessageReader<E>` (`EventWriter` /
       `EventReader` on pre-0.17 Bevy).
     - A **fn registered in the plugin that the spec's `## systems` omits** is
       CODE-AHEAD — and unlike an omitted helper, it is worth reporting: an
       unlisted system is a whole unit of frame behavior missing from the model.
   - **`## access`** (`kind: system`) — the same comparison for the one fn.
     Its `schedule:`/ordering live in the **plugin's** registration, so check
     them there and report a mismatch against this spec.
   - **`## invariants`** — for each, find the enforcing mechanism (a constant
     or bound, a run condition, a guard clause, a `Without<..>` filter, an
     assertion). An invariant with no discoverable enforcement is **SPEC-AHEAD /
     at-risk** — the guarantee may silently not hold. You *may* exercise it
     dynamically with a read-only test run (best-effort, per the repo's
     `CLAUDE.md`, launched via `mise exec --cd <checkout>`); if it cannot run
     here, mark it **CANNOT-VERIFY**, do not fail
     it.
   - **header `method:` / field `@source`** (endpoints) — the `method` must
     match the route's HTTP verb, and each field's
     `@path`/`@query`/`@body`/`@header` must match where the controller actually
     reads that parameter. A mismatch (spec says `@query`, code binds it from
     the path) is **SPEC-AHEAD**.
   - **`@ spec` cross-refs** — each ref (bare `<unit>`) must resolve to a
     sibling `<unit>.hmml` in the **same task directory** as this spec. A
     dangling ref is **SPEC-AHEAD** (the referenced spec was renamed/removed, or
     never written). A **member** ref (`@ spec unit::function`) must also name an
     `## api` entry that spec defines — `hobby-mml-parser --lint` reports a
     missing one as an `unknown-spec-member` **error**, and it is SPEC-AHEAD
     here too. Do **not** validate the referenced unit's own conformance here —
     that is its own validate run; only check that delegating to that unit still
     matches what the code does (the step really does hand off to that
     queue/endpoint/job).
   - **`## api`** (a `kind: module` spec, grammar §11) — the verdict is **per
     entry**, not per section: one row per `fn`, so "which function drifted" is
     answerable directly. For each entry:
     - the function **exists** and is **public** in the routed code (a spec entry
       the code no longer exposes is **SPEC-AHEAD**);
     - its **signature** matches — parameter names/types in order and the return
       type, read through the type-mapping tables `hmml-extract` uses, and
       ignoring `&`, `&mut` and lifetimes. A renamed parameter is prose-level; a
       changed *type* or arity is **SPEC-AHEAD**;
     - its **bullets** still describe what the body does, at the altitude rule
       above (a reworded-but-equivalent bullet CONFORMS);
     - its **call graph** holds: every collaborator a bullet references with
       `@ spec <unit>::<function>` is actually called by that function
       (`call_path` / `get_neighbors`), and a call the bullets do not mention
       *to another module in `## modules`* is **CODE-AHEAD**. A call into a
       private helper of this module is **not** drift — see below;
     - **private helpers are never drift.** A module spec describes the public
       surface only, so a private function the code has and the spec does not is
       correct by design: the code may factor a bullet (or a nested tree) into a
       helper. Never report one as CODE-AHEAD; follow it and judge
       the *behaviour* it implements against the bullet that owns it;
     - **completeness is a warning, not a verdict on the code.** A public
       function in the code with no `## api` entry is **CODE-AHEAD** — the spec's
       own rule is that the whole surface is present — but report it as one line
       per missing function, not as a failed section: a module with sixty
       accessors should not read as sixty drifts. Say plainly that the fix is a
       re-extract, not a code change.
   - **`where:` bindings** — for each `Symbol = value [@ config-source]`, read
     the value at the `@ config-source` site (env-var default, constant,
     parameter default) and confirm it equals the bound `value`. A drifted value
     (spec says `Y = 3`, code default is now `5`) is **SPEC-AHEAD** (or
     CODE-AHEAD if the code is the intent) — this is the check that makes value
     drift visible.

   Also **cross-check the spec against itself** — a behavior step that
   contradicts an unedited `## invariant` or `## modules` responsibility is an
   *internal* inconsistency; report it as **SPEC-AHEAD (self-contradiction)**
   because at most one side can match the code.

4. **Report a conformance verdict and stop.** Emit a per-section table:
   `section → verdict → specifics (what, where, which direction)` — for a
   `kind: module` spec, `## api` expands to **one row per `fn` entry**, plus a
   final row listing public functions missing from the spec. Then an
   overall verdict — **CONFORMS** (every claim holds) or **DRIFTED** (with the
   count and the most important items first). For each drift, recommend the
   remediation **without performing it**:

   - **SPEC-AHEAD** and the spec is the intent → change the code to match.
   - **CODE-AHEAD** and the code is now the truth → re-run
     `/hmml-extract`, review, and recommit the refreshed spec as the new
     baseline.

   **Never `Edit`/`Write`, never `git add`/`git commit`** — not the code, the
   spec, or the tooling repo. This skill emits a verdict; acting on it is the
   human's job.

## Grammar cheat-sheet

- File = header, then `##` sections in canonical order: `modules`, `data`,
  `components`, `resources`, `events`, `systems`, `access`, `behavior`,
  `invariants`. **Which are legal depends on `kind`:**
  - `endpoint`/`workflow`/`job` — exactly `modules`, `data`, `behavior`,
    `invariants`, all required (a body may be empty except `behavior`).
  - `plugin` — `systems`, `behavior`, `invariants` required; `modules`, `data`,
    `components`, `resources`, `events` optional; no `access`.
  - `system` — `access`, `behavior`, `invariants` required; `modules`, `data`,
    `components`, `resources`, `events` optional; no `systems`.
  - `module` — exactly `modules`, `data`, `api`, `invariants`; no `behavior`,
    no Bevy section.
- Header keys: `system` (req), `unit` (req), `kind` (req: `endpoint`|`workflow`|
  `job`|`plugin`|`system`|`module`), `method` (opt, endpoints: `GET`|`POST`|…),
  `base`
  (opt: subdir the `source.paths` resolve against), `source` (opt: `repo` +
  `paths` list). `system`/`unit` are identifiers.
- `#` at column 0 is a comment; `##` at column 0 is always a section header.
- Indent = spaces only, 2 spaces per nesting level.
- `## modules`: `PascalName — responsibility [@ repo-relative-path]`.
- `## data`: `struct Name { field: Type [@ source], ... }`,
  `enum Name { Variant, ... }` (unit variants, no payloads), then `in: <Type>` /
  `out: <Type>`. Types: primitives, `Uuid`/`Money`/`DateTime`,
  `Option<T>`/`Vec<T>`/`Map<K,V>`/`Queue<T>`/`Stream<T>`/`Handle<T>`/
  `Future<T>`/`Result<T,E>`/`Either<L,R>`/`Tuple<A,B>`, or a defined
  struct/enum, or a type brought in by a `use` import. A field's optional
  `@ source` is `path`|`query`|`body`|`header` (endpoints).
- Type imports (grammar §4.5): `## data` may start with `use unit: Type, …`
  lines that bring a data model defined by another unit into scope. Resolves
  like `@ spec` (a sibling `<unit>.hmml` in the same task directory); unresolved
  target/type = warning.
- Referencing existing code (grammar §4.7): a type declaration may end with
  `@ <repo-relative-path>` — `struct Wave @ src/waves/mod.rs` — meaning the type
  already exists there. The body then becomes **optional**, and when present it
  is a **delta**: `+ field: Type` adds a field, `- field` removes one, an
  unmarked field asserts one already exists, and anything not listed **stays as
  it is**. Reference a type this unit does not own instead of restating it. The
  filesystem-aware tools check the type is there, that its `#[derive(…)]` fits
  the section's role, and that unmarked members exist — all warnings.
- Dependency types (grammar §4.8): `use extern avian3d::prelude: Collider` for a
  type from another crate. Same scope and role rules as `use`, and nothing else:
  no file to resolve, no derive check, no members. Goto opens its docs.rs page.
- `## behavior`: nested `-` bullets; control-flow keywords
  `if…: / else: / for each…: / while…: / try: / on error…:`; else free prose;
  declared names cross-link. Symbols `X`/`Y`/`N` are abstract quantities, and an
  optional trailing `where:` block binds them:
  `Symbol = value [@ config-source]`.
- `## invariants`: flat `-` bullets, each a checkable claim.
- Cross-spec refs (grammar §5.4): a module line, **system entry**, behavior step,
  `## api` entry, or invariant may end with `@ spec unit` (bare unit) — "more
  specification under" that unit's `.hmml` — or `@ spec unit::function` /
  `@ spec unit::Type::method` naming one `## api` entry of a module spec. At most
  one per line, always the last annotation (after an `@ path`); it resolves to a
  sibling `<unit>.hmml` in the same task directory. A missing target file =
  warning; a member the target file does not define = **error**.

**Detail dialect (grammar §11):**

- `kind: module` writes `## api` where every other kind writes `## behavior`;
  writing the other spelling is a `section-not-allowed-for-kind` error.
- Entries are `fn [Type::]name(param: Type, …) [-> Type] [@ path] [@ spec ref]`
  at **column 0**, each owning the bullets indented beneath it — ordinary steps,
  same closed keyword set.
- `## data` holds the public types the module owns. `in:`/`out:` and `where:`
  are errors on this kind. An empty `## api` is an `empty-api` warning.
- Four containers exist for signatures: `Future`, `Result`, `Either`, `Tuple`.

**Bevy dialect (grammar §8):**

- `## components` / `## resources` / `## events`: the `## data` type grammar
  minus `in:`/`out:`, plus `marker Name` (a zero-sized component, `## components`
  only). **The section assigns the ECS role**, for `use` imports too — so the
  role is a checkable claim about the code's derive.
- `## data` in a Bevy spec (grammar §4.6): the **role-less** shapes — a field
  type, an enum a component is built from, a nested config record. Same body
  grammar, but no `in:`/`out:` and no `marker`, and a `## data` name may not
  appear in an access list (`access-role-mismatch`). One type namespace across
  data + world sections, so a name declared in two of them is `duplicate-type`.
  A `## data` type claims the *absence* of a `Component`/`Resource`/`Message`
  (or `Event`) derive; a code type that has one is drifted out of `## data`.
- Bevy's own types (`Transform`, `Time`, `AssetServer`, `AppExit`, …) are built
  in and are never declared locally.
- `## systems`: `snake_name — responsibility [@ path] [@ spec unit]` at column 0,
  each followed by an indented access block. `## access` is the same block,
  unindented, for a `kind: system` file.
- Access keys: `schedule:` (required), `set:`, `after:`, `before:`, `run_if:`,
  `query:` (`A + B - C`), `reads:`, `writes:`, `spawns:`, `despawns:`, `emits:`,
  `consumes:`. `query:`/`spawns:`/`despawns:` take components,
  `emits:`/`consumes:` events, `reads:`/`writes:` either a component or a
  resource.

The spec is a **lossy view**: absence of detail in the spec is not drift. Only a
spec claim the code contradicts (SPEC-AHEAD), or a significant code behavior the
spec omits (CODE-AHEAD), is drift.

## Worked examples

### Clean conformance (the read-only no-op)

The human extracts a unit, commits it, and later runs validate with no code
changes since. Every module `@ path` resolves, structs/enums match, every
behavior step maps to code, and every invariant has an enforcement site. Report
**CONFORMS** for all four sections and an overall **CONFORMS** — the code still
matches its mental model. Recommend nothing.

### SPEC-AHEAD — an invariant the code no longer upholds

`mesh-terrain-chunks.hmml` invariant *"at most 4 chunks are meshed per
frame"*, but the `MAX_MESHES_PER_FRAME` constant in `src/chunk/mesher.rs` is now
`16`. Route via the `ChunkMesher @ src/chunk/mesher.rs` anchor, find where the
mesh queue is drained, see `16 ≠ 4`. Report **SPEC-AHEAD** on that invariant
(and note it also contradicts the `## behavior` "take at most 4 from the queue"
step — a self-consistent spec that the *code* broke). Overall **DRIFTED**.
Recommend: if 4 is still the intent, change the code back to it; if the
code intentionally moved to 16, re-extract to refresh the spec. Do not touch
either.

### CODE-AHEAD — a value set the spec is missing

`reload-shader-batch.hmml` has `enum ReloadOutcome { swapped, unchanged,
rejected }`, but the Rust enum in `src/watch/reload.rs` gained `deferred`, and
the `match` in the reload system now queues deferred shaders for the next frame.
The code produces a value the spec's closed set does not list — a behaviorally
significant **CODE-AHEAD**. Report **DRIFTED** on `## data`; recommend
re-extracting to add `deferred` to the spec (the code is the truth here). A merely-renamed private helper
or an added log line in the same file is **not** reported — that is expected
lossiness.
