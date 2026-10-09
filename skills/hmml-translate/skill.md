---
description: Translate an edited .hmml spec into a minimal code change that makes the real system match it
allowed-tools: Read, Grep, Glob, Edit, Write, MultiEdit, Bash(hobby-mml-parser:*), Bash(git -C * diff:*), Bash(git -C * show:*), Bash(git -C * cat-file:*), Bash(git -C * ls-files:*), Bash(git -C * rev-parse:*), Bash(git -C * status:*), Bash(git -C * worktree list:*), Bash(git -C * log:*), Bash(git diff:*), Bash(ls:*), Bash(pwd:*), Bash(sbt *), Bash(cargo *), Bash(npm *), Bash(just *), Bash(uv *), Bash(mise exec *), mcp__tree-extract__build_graph, mcp__tree-extract__list_modules, mcp__tree-extract__get_neighbors, mcp__tree-extract__call_path, mcp__tree-extract__get_symbol, mcp__tree-extract__search_symbols, mcp__lsp-mcp__document_symbol, mcp__lsp-mcp__find_references, mcp__lsp-mcp__goto_definition, mcp__lsp-mcp__hover
---

Translate an **edited** `.hmml` spec into the **code change** that makes the real
system match it — spec → code, not prose → code. This is the payoff step of the
HMML loop: `hmml-extract` turns code into a `.hmml`, the human edits the
`.hmml` to express the change they want, and this skill turns that edit into a
minimal, reviewable diff against the real repo.

The default and best path is **delta-based**, and the delta is normally written
*into* the spec: the file holds the unit's **baseline** (the version
`hmml-extract` captured) plus **annotations** marking what this change adds and
removes — `# +` blocks and `# - <text>` lines, per grammar §10. Those annotations
are your work list; the unannotated lines around them describe code that already
exists and must not be touched. Translate **only the delta** — do not regenerate
the unit (regeneration produces noisy diffs and rewrites code the spec never
mentioned). For an unannotated spec, fall back to the git delta (specs live in
the notes repo `~/fun_repos/hobby-mimisbrunnr`, a git repo, so the last
*committed* `.hmml` is the baseline and the working-tree edit is the delta).
Full generation is the last resort, for a brand-new spec with neither.

Apply the change to the task's **worktree** checkout of the source repo, verify
it (compile/tests), report the diff, and **stop** — reviewing and committing the
code is the human's job (same contract as extract).

Read `~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/grammar.md` (the authoritative
hobby grammar, v2 — **not** the work one) before your first translation if you
are unsure of any rule below. The grammar cheat-sheet and worked examples at the
end of this file are the reference.

## Instructions

1. **Identify the spec.** Specs live per-task in the notes repo at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml`,
   where `{project}`, `{topic}` and `{task}` are the first, second and third
   dirs under `~/fun_worktrees` (from `~/fun_worktrees/{project}/{topic}/{task}`;
   `{task}` verbatim including any blurb, falling back to the git branch) — the same detection
   `journal:resume` uses. The filename is the `unit:` header plus `.hmml`,
   **directly in the task dir** — there is no `<system>` subdirectory (the
   `system:` header is now an optional label). The human points at one `.hmml` (a
   path, or a `unit` name). Resolve it to the task dir and confirm the file
   exists. Validate it with `hobby-mml-parser <path> --lint` (grammar + linkage: an
   `unresolved-source-path` warning means a `source.paths`/`@ path` entry names
   a file that doesn't exist — surface it to the human rather than guessing; in
   full-generation mode intended-but-not-yet-created paths are expected to warn
   until the files are generated). If the binary is missing, validate by eye
   against the cheat-sheet. If the pointer is ambiguous, ask **one** clarifying
   question rather than guessing.

2. **Compute the delta and pick a mode.** First look **inside the spec** for
   annotations (`rg -n '^\s*# [+-]' <path>`).

   **Annotated spec** (any `# +` / `# -`) → **annotated-delta mode, the main
   path.** The delta is exactly what the markers say: a `# - <text>` line is
   baseline text commented out in place, i.e. behaviour / data / a system / an
   invariant this change **removes**; a `# +` marker means the line directly
   beneath it, plus any lines nested deeper than that line, is **added**
   (adjacent added siblings each carry their own marker, and a `struct`/`enum`
   head is covered through its closing brace); a `# -` immediately followed by a
   `# +` is one **modification**, not two independent edits. Everything
   unannotated is the baseline: it already exists in code, so **do not re-derive
   or rewrite it**. The spec's git state is irrelevant here — an annotated spec
   that is committed and clean is still a delta to apply, not a no-op — so skip
   the git modes below and go straight to step 3. Two guards:

   - **Sanity-check that the baseline is recoverable**: un-commenting the `# -`
     lines and dropping the `# +` blocks must yield a coherent spec of the code
     as it is today. `hobby-mml-parser <path> --lint` checks the mechanical half
     (`unrecoverable-baseline`, `empty-change-removal`, `empty-change-addition`);
     you check the half it cannot — a `# -` line with no counterpart in the code,
     a `# +` block whose behaviour the code already has. Either way the
     annotations are stale or hand-mangled: **stop and say so** rather than
     applying half a change.
   - **Never edit the spec**, including "settling" it by stripping annotations.
     The markers stay after the code lands — the spec is the record of the change
     — and only the human ever removes them.

   A legacy inline member delta (`+ field: Type` / `- field` inside a `@ <path>`
   type body, grammar §4.7) still means add / remove and you still apply it; new
   specs write it as `# +` / `# -` like everything else.

   **Unannotated spec** → fall back to the git delta. The baseline is the last
   committed spec and the delta is the human's working-tree edit on top of it.
   Say in your report that the spec was unannotated, since the annotated form is
   the one that survives a commit.

   ```sh
   # <path> = notes/{project}/{topic}/{task}/<unit>.hmml (relative to the notes repo)
   git -C ~/fun_repos/hobby-mimisbrunnr diff HEAD -- <path>   # the delta
   git -C ~/fun_repos/hobby-mimisbrunnr show HEAD:<path>      # the old spec
   ```

   Choose the mode from what you find:

   - **No committed baseline**
     (`git -C ~/fun_repos/hobby-mimisbrunnr cat-file -e HEAD:<path>`
     fails — untracked or never committed) **or no `source` block** →
     **full-generation mode** (step 9).
   - **Empty diff** → **stop, no-op.** Report "spec matches its last commit;
     nothing to translate." (This is the round-trip guarantee — see the no-op
     example.)
   - **Clean tree but the human asserts an edit** (they already committed it) →
     ask for an explicit base ref and diff against it
     (`git -C ~/fun_repos/hobby-mimisbrunnr diff <base>..HEAD -- <path>`).
   - **Non-empty diff** → **git delta mode** (the main path when there are no
     annotations). Split the diff into **(a) a header/`source` delta** and
     **(b) a body delta**.
     - If the diff looks like a **re-extraction** (whole sections replaced,
       `source.repo`/`paths` rewritten wholesale) rather than a targeted human
       edit, **stop and confirm intent** — translating a re-extraction would
       rewrite the world. The contract is: baseline = the committed extract,
       delta = the human's edits on top.
     - A **`source` change is re-targeting, not behavior.** If `source.repo` or
       `source.paths` changed, the old spec described *different code* than the
       new spec points at — confirm with the human and adjust which code you
       read/edit; never synthesize behavior from a source change.

   **Consistency gate — before applying any delta.** Read the *unedited* rest of
   the spec (the `## invariants`, the `## modules` responsibilities, and the
   sibling `## behavior` / `## data` the delta did not touch) and confirm the
   delta does not contradict it. A behavior edit that violates an unedited
   invariant — e.g. a step changed to "prefetch 10" while the invariant still
   says "prefetch 1" — or an enum/field edit a sibling step depends on, is an
   **internally inconsistent spec**: at most one side can match the code. **Stop
   and flag it; ask the human which they meant** rather than translating one
   half and silently breaking the other.

3. **Resolve the source and choose the write target.** `source.repo` resolves to
   `~/fun_repos/<repo>` — but that is the **shared checkout; never edit it.** The
   edit target is the task's **worktree**:

   - Locate a worktree checkout of `source.repo` — prefer the current working
     directory if it is one, else look at `~/fun_worktrees/<project>/<topic>/<task>`
     (the task dir is the worktree; there is no repo level below it)
     (`git -C <path> worktree list` / `ls`). Read the code to change **from that
     worktree** so what you read is exactly what you edit (no HEAD drift).
   - If the only checkout available is `~/fun_repos/<repo>`, **do not edit it.**
     Ask the human to point at or create a worktree (`hwt new <task> -p <project> -t <topic>` from the
     task dir); if they decline, switch to the **patch-emit fallback** (step
     8b).
   - Verify each `source.paths` entry resolves to a real file in the target
     checkout.

4. **Locate the code behind each changed section.** Route each changed
   `## modules` line via its **`@ path`** — that is the reliable anchor; the
   PascalCase `Name` need not equal the code symbol. Then pin the exact symbols
   the delta touches:

   - **`tree-extract`** — if no graph exists (`.project-graph.json` absent),
     `build_graph` first; then `get_symbol` on the entry point and
     `get_neighbors` / `call_path` to find the functions the changed steps map
     to.
   - **`lsp-mcp`** (`document_symbol`, `goto_definition`, `find_references`,
     `hover`) for precise signatures and to resolve a type to its definition.

5. **Map the delta to code changes.** The spec is **intent, not a literal
   template** — fit the existing code's idioms. Invert the grammar's
   type-mapping tables (HMML `i64` → the source language's 64-bit int, `Vec<T>` →
   its list type, an `enum` → its closed value set, etc.). Per section:

   - **`use` import added/removed** (any section that takes one) → *not* a data-model change to
     implement here. An imported type is the owning unit's contract: code
     against the shared code type it already maps to (the `use`'d unit's
     struct/DTO), and **never redeclare that type** in this unit's source.
     Adding a `use` for a type that replaces a local `struct` means the local
     definition (and its code duplicate, if this unit had one) should collapse
     onto the shared type — a refactor toward the owner, not a new type. A
     removed `use` whose type is still referenced will fail validation; treat
     that as a spec error to flag.
   - **A type carrying `@ <path>`** (grammar §4.7) → **the type already exists;
     never generate it.** The declaration is a *reference*, and its body — if it
     has one at all — is **partial**, not a definition of it. This is the one
     place where reading a spec section literally corrupts working code, so read
     it exactly:
     - a `# +` member → **add** that field to the real struct at `@ <path>`,
       and fix the construction sites the new field breaks. (A legacy inline
       `+ field: Type` means the same and is still read.)
     - a `# -` member → **remove** that field, and the code that reads it
       (legacy inline `- field` likewise).
     - an **unmarked** field → an assertion that it is already there. Change
       nothing. If it is missing, the spec is describing code that does not
       exist — flag it, do not "helpfully" add the field.
     - a field the body **does not mention** → **leave it exactly as it is.** A
       reference body is never the complete field list, so a field's absence
       from the spec says nothing whatsoever about the code. Deleting fields to
       match a partial body is the failure this syntax exists to prevent.
     - **no body at all** → nothing to do. The spec is naming a type, not
       asking for a change.
     - The `@ <path>` moving is a **re-target**, like a `source` change: the
       type moved (or the spec was wrong about where it lived), which is not a
       request to move it. Confirm with the human.
     - After the change, the `#[derive(…)]` must still match the section it sits
       under; if the section moved too, that is a role change, below.
   - **`use extern`** (grammar §4.8) → a type from a dependency. Import it,
     never define it. Adding one means the code should `use` that crate's type;
     removing one whose name is still referenced is a spec error to flag.
   - **`## data`** struct/field change → the struct/DTO/type in code. An `enum`
     variant added/removed → the code's closed value set (Scala sealed
     trait/`enum`, Python `Enum`/`Literal[...]`, a Rust `enum`) **and** the
     `match`/`when` sites that must now handle it. A type's value set can be
     used **outside** the unit's `source.paths` (a client signature, a test
     generator, another module's `match`); **grep the whole source repo** for
     the type's usages, and *report* out-of-unit impact sites even when you do
     not edit them — a spec-scoped edit is blind to them otherwise.
   - **`## behavior`** step added (`# +`) / removed (`# -`) / reordered → the corresponding logic,
     **routed to the module named in the step** via its `@ path`. Be
     conservative:
     - a **reworded** step with no semantic change → **no code change**;
     - a **removed** step → remove only code *unambiguously* owned by that step;
       the spec is lossy (extract drops detail), so absence ≠ dead code — when
       in doubt, **flag it for the human** instead of deleting;
     - **reorder** only when the order is genuinely semantic.
   - **`## api`** (a `kind: module` spec, grammar §11) — the strongest mapping
     in the language, and the one that makes the code resemble the spec:
     - a **`fn` entry added** (`# +`, which covers its whole nested tree) → add
       that public function to the module, with exactly that signature, and
       implement it from its bullets;
     - a **signature changed** (`# -` old line + `# +` new one) → change the
       signature and every call site (`find_references`), including out-of-unit
       ones — report those you do not edit;
     - a **bullet changed inside an entry** → change that function's body, and
       only that function's;
     - a **`fn` entry removed** (`# -`) → remove the public function if nothing
       still calls it; when callers remain, flag rather than delete;
     - **the bullets drive the body 1:1 in behaviour, not in structure.** You
       decide how the body is factored: extracting a private helper for a bullet
       (or for a nested tree) is correct and expected — a module spec describes
       the public surface only, so a helper is never "undescribed drift". Say in
       the report which helpers you introduced.
     - Route the whole entry via its `@ path` when it has one; otherwise via the
       module spec's `source.paths`.
   - **`## modules`** responsibility change → may mean moving code between
     files.
   - **`## components` / `## resources` / `## events`** (Bevy) → the derived
     type. Added type → a new `#[derive(Component)]` / `#[derive(Resource)]` /
     `#[derive(Event)]` struct, registered in the plugin's `build` where
     registration is required (events need `add_event`; components and resources
     usually need `init_resource` / `insert_resource` only for resources).
     A `marker Name` is a **unit struct** (`#[derive(Component)] struct Name;`),
     not an empty braced one. Moving a type between world sections is a **role
     change** — a component becoming a resource means it stops being attached to
     entities and starts being a singleton; that is a real refactor, not a
     rename. Never emit a Rust type for a Bevy built-in (`Transform`, `Time`, …)
     — those names in a spec mean the engine's type.
   - **`## data`** (Bevy) → a plain Rust `struct`/`enum` with **no** ECS derive
     and no registration in `build`. A type moving between `## data` and a world
     section is the same **role change** as above, in the other direction:
     adding the derive makes it something systems can access, removing it means
     the type is now reached only through its owner. Do the move, and check the
     access lists that named it.
   - **`## systems`** (Bevy) — route each entry to its Rust fn via its
     `@ path`, and map the access block key by key:
     - `schedule:` → which `app.add_systems(<Schedule>, …)` call it sits in.
       A changed schedule is a **move between `add_systems` calls**, not an edit
       to the fn.
     - `after:` / `before:` / `set:` → `.after(..)` / `.before(..)` /
       `.in_set(..)` (or the position within a `.chain()`).
     - `run_if:` → `.run_if(..)`, with the condition as prose to realise.
     - `query:` → the `Query<…>` parameter: `+` terms are the fetched/`With<..>`
       set, `-` terms are `Without<..>`.
     - `reads:` / `writes:` → `Res<T>` vs `ResMut<T>` for resources, `&T` vs
       `&mut T` inside the query for components. **A component moving from
       `reads:` to `writes:` is a real signature change** — it changes the
       system's parallelism, so make it even though the body may not need edits.
     - `spawns:` / `despawns:` → `commands.spawn((..))` / `.despawn()`.
     - `emits:` / `consumes:` → `EventWriter<E>` / `EventReader<E>`, plus the
       `add_event::<E>()` registration if the event is new.
     - A system entry added → a new fn **and** its `add_systems` registration.
       Removed → remove both, subject to the same "absence ≠ dead code" caution
       as a removed behavior step.
     - A system entry gaining `@ spec <unit>` is a **boundary marker**, not a
       work item — that system now has its own spec and its own translate run.
   - **`## access`** (`kind: system`) → exactly as above, for the one fn this
     spec covers. Its registration lives in the plugin's spec, not here: if the
     schedule or ordering changed, **report** that the plugin's `add_systems`
     call needs the matching edit rather than editing another unit's code.
   - **`## invariants`** change → assertions/guards, or a test that encodes the
     property. For Bevy, a component-exclusivity invariant ("no entity carries
     both A and B") is naturally a debug assertion or a test spawning the
     forbidden combination, not a runtime branch.
   - **header `method:` / field `@source` change** (endpoints) → the route
     definition and/or the controller's parameter binding (path vs query vs body
     vs header); a `method` change is a different verb/route, not a body change.
   - **`@ spec` cross-ref** → a boundary marker, not a work item. The referenced
     unit has its own spec and its own translate runs — **never re-implement or
     edit the referenced unit's code** from this spec. A **member** ref
     (`@ spec unit::function`, §11) is the same boundary, one notch sharper: it
     names the exact function to call, so the step becomes a call to *that*
     function with the signature its `## api` entry declares — and if the
     signature does not fit the call, that is a spec conflict to flag, not a
     signature to change from here. A ref *added* to a step usually means
     "delegate here": wire the hand-off (enqueue to that queue, call that
     endpoint, schedule that job) and nothing more. A ref *removed* changes
     nothing by itself; only the step's own prose delta matters. Read the
     referenced `.hmml` (a sibling `.hmml` in the **same task dir**; the ref is a
     bare `<unit>`) when you need the collaborator's contract — for a behavioural
     target its `in:`/`out:` types or access block are the interface to code
     against; for a `unit::function` ref into a module spec it is that entry's
     `## api` signature, since a module spec has no `in:`/`out:` at all.
   - **`where:` binding value change** → this is the payoff for value-only
     edits: a symbol's abstract meaning is unchanged, but its concrete value
     moved, so route the change to the binding's **`@ config-source`** (the
     env-var default, constant, or parameter default named after the `@`) and
     change only that literal — e.g. `Y = 3 @ max_sessions` → `Y = 5` means set
     `max_sessions`'s default to 5. A binding with no `@ config-source` is
     unlocated: find the value's site from the behavior/invariant that
     references the symbol, or flag.

6. **Apply the change** to the target worktree with `Edit`/`MultiEdit` (or
   `Write` for new files in full-generation mode). Keep the diff **minimal and
   reviewable** — touch only what the delta implies.

7. **Verify.** Run the touched repo's build/test per its `CLAUDE.md`, launched
   with `mise exec --cd <worktree>` so project tool versions resolve:

   - Scala/sbt: `sbt --client compile`, `sbt --client test`
   - Python/uv+just: `just check` / `just test`
   - Rust/cargo: `cargo build`, `cargo test`, `cargo clippy`
   - TS/Angular: `npm run build`, `npm run test:app:ci`

   Fix compile errors your change introduced. Report failures you cannot resolve
   rather than papering over them.

8. **Report and stop.**

   a. Show the worktree `git diff` and a one-line summary of what changed and
   why (which spec delta drove which code edit). In annotated mode, walk the
   markers: every `# +` and `# -`, and the code edit it produced — or, for a
   `# -` you declined to delete because the ownership was ambiguous, an explicit
   "not applied, needs your call". An unmentioned marker reads as done.

   b. **Fallback** — if no writable worktree was available, write a `.patch`
   (against the source checkout) into the task's scratch/notes dir, report its
   path, and note that **verification was skipped** (a patch can't be
   compiled/tested).

   **Never `git add` / `git commit`** — not in the worktree, the notes repo, or
   the tooling repo. Do **not** commit the spec or touch other specs; there is
   no index to regenerate. Reviewing and committing the code is the human's job.

9. **Full-generation fallback (no committed baseline).** With no baseline to
   diff (a hand-written or freshly scaffolded spec, possibly with no `source`
   yet), generate the whole unit from the spec into the files named by
   `source.paths` (creating new files for a new unit), fitting the target repo's
   idioms, then verify (step 7) and report (step 8). This overlaps
   `hmml-new`'s "new unit" territory; prefer delta mode whenever a
   baseline exists.

   **Caution — full-generation is destructive at file granularity.** A file in
   `source.paths` often holds code *beyond* this unit (a second endpoint in the
   same controller, a sibling function, an unrelated type). Regenerating the
   file from the spec would **delete that other code**, because the spec only
   models this unit. Never overwrite a file that contains code outside the unit:
   create new files where the unit is genuinely new, and where you must touch a
   shared file, **flag the sibling code and stop** rather than clobber it. This
   is exactly why delta mode is the default — use full-generation only for a
   truly new unit whose files it alone owns.

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
  (opt), `source` (opt: `repo` + `paths` list). `system`/`unit` are identifiers.
- `#` at column 0 is a comment; `##` at column 0 is always a section header.
- Indent = spaces only, 2 spaces per nesting level.
- `## modules`: `PascalName — responsibility [@ repo-relative-path]`.
- `## data`: `struct Name { field: Type [@ source], ... }`,
  `enum Name { Variant, ... }` (unit variants, no payloads), then `in: <Type>` /
  `out: <Type>`. Types: primitives, `Uuid`/`Money`/`DateTime`,
  `Option<T>`/`Vec<T>`/`Map<K,V>`/`Queue<T>`/`Stream<T>`/`Handle<T>`/
  `Future<T>`/`Result<T,E>`/`Either<L,R>`/`Tuple<A,B>`, or a defined
  struct/enum. Undefined type = hard error. A field's optional `@ source` is
  `path`|`query`|`body`|`header` (endpoints).
- Type imports (grammar §4.5): `## data` may start with
  `use unit: Type, …` lines that bring a data model defined by another unit into
  scope. The target is a bare `<unit>` resolving to a sibling `<unit>.hmml` in
  the same task dir. Reuse a shared
  type this way instead of redeclaring it; resolves like `@ spec`. Unresolved
  target/type = warning.
- Referencing existing code (grammar §4.7): a type declaration may end with
  `@ <repo-relative-path>` — `struct Wave @ src/waves/mod.rs` — meaning the type
  already exists there. The body then becomes **optional**, and when present it
  is **partial**: a listed member asserts it already exists, anything not listed
  **stays as it is**, and a member change is written with §10's `# +` / `# -`.
  The v2.2 inline `+ field: Type` / `- field` is legacy but still read with the
  same meaning. Reference a type this unit does not own instead of restating it. The
  filesystem-aware tools check the type is there, that its `#[derive(…)]` fits
  the section's role, and that unmarked members exist — all warnings.
- Dependency types (grammar §4.8): `use extern avian3d::prelude: Collider` for a
  type from another crate. Same scope and role rules as `use`, and nothing else:
  no file to resolve, no derive check, no members. Goto opens its docs.rs page.
- `## behavior`: nested `-` bullets; control-flow keywords
  `if…: / else: / for each…: / while…: / try: / on error…:` (trailing `:`, body
  nested); everything else prose; declared names cross-link. An optional
  trailing `where:` block binds symbols: `Symbol = value [@ config-source]`.
- `## invariants`: flat `-` bullets, each a checkable claim.
- Cross-spec refs (grammar §5.4): a module line, **system entry**, behavior step,
  `## api` entry, or invariant may end with `@ spec unit` — "more specification
  under" that unit's `.hmml` — or `@ spec unit::function` /
  `@ spec unit::Type::method` naming one `## api` entry of a module spec. At most
  one per line, always the last annotation (after an `@ path`); it resolves to a
  sibling `.hmml` in the same task dir. A missing target file = warning; a member
  the target file does not define = **error**.

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
  only) → a Rust unit struct. **The section assigns the ECS role**, for `use`
  imports too — a world-section type gets the matching derive.
- `## data` in a Bevy spec (grammar §4.6): the **role-less** shapes — a field
  type, an enum a component is built from, a nested config record. Same body
  grammar, but no `in:`/`out:` and no `marker`, and a `## data` name may not
  appear in an access list (`access-role-mismatch`). One type namespace across
  data + world sections, so a name declared in two of them is `duplicate-type`.
  A `## data` type is a plain Rust `struct`/`enum` with **no** ECS derive.
- Bevy's own types are built in and never generated: `Transform`, `Time`,
  `AssetServer`, `AppExit`, ….
- `## systems`: `snake_name — responsibility [@ path] [@ spec unit]` at column 0,
  each followed by an indented access block. `## access` is the same block,
  unindented, for a `kind: system` file.
- Access keys: `schedule:` (**required**), `set:`, `after:`, `before:`,
  `run_if:`, `query:` (`A + B - C`), `reads:`, `writes:`, `spawns:`,
  `despawns:`, `emits:`, `consumes:`. Roles are enforced: `query:`/`spawns:`/
  `despawns:` take components, `emits:`/`consumes:` events, `reads:`/`writes:`
  either a component or a resource.

## Worked examples

These show the shape of a translation: a **spec delta** (what the human changed
in the `.hmml`) and the **code change** it drives. The reference specs live in
the task dir under
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`.

### Behavior-step delta → minimal routed diff

The human annotates one `## behavior` step in the task dir's
`mesh-terrain-chunks.hmml` — the water-plane test:

```hmml
  - else:
    - sample a HeightTile from HeightSampler at the resolution implied by lod
    # - if the tile is entirely below the water plane:
    # +
    - if the tile is entirely at or below the water plane:
      - emit an empty ChunkMesh (no triangles) so the coord is not re-requested
```

Translation: annotated-delta mode, one modification (`# -` then `# +`) touching
a single step owned by `HeightSampler` / `ChunkMesher`. Route via the module's
`@ path` (`src/chunk/mesher.rs`), find the comparison with `tree-extract` /
`lsp-mcp`, and make the one-line change (`<` → `<=`) in the **worktree**.
Nothing else in the unit is annotated, so nothing else is touched. Compile/test,
show the diff, walk the one marker pair, stop. This is the acceptance case — a
minimal, correct diff in exactly one module.

### Data enum-variant delta → closed-value-set change

The human adds a variant to a `## data` enum in the task dir's
`reload-shader-batch.hmml`:

```hmml
enum ReloadOutcome {
  swapped,
  unchanged,
  rejected,
  skipped,
# +
  deferred,
}
```

Translation: a **data delta**. Add the variant to the Rust `enum ReloadOutcome`
behind `src/watch/reload.rs`, and — because the grammar treats an enum as a set
every site must handle — update the `match` sites that switch on it so the new
variant is not silently dropped. Do not invent behavior
the spec doesn't describe; if a site genuinely needs a decision the spec is
silent on, flag it for the human.

### No-op (the round-trip guarantee)

The human runs translate on a spec they extracted and committed but did **not**
edit. `git -C ~/fun_repos/hobby-mimisbrunnr diff HEAD -- <path>` is
**empty**. Stop immediately with zero code edits and report "spec matches its
last commit; nothing to translate." Extract → (no edit) → translate is a genuine
no-op **by construction**: empty spec diff ⇒ empty code diff. This holds only in
delta mode with the extracted spec committed as the baseline — never run
full-generation mode on an unedited freshly-extracted spec (it will
re-synthesize code and drift).
