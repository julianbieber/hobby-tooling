---
description: Scaffold a skeleton .mml (Mental Model Language) spec for a new feature — a blank template or a first-pass draft from a rough idea — for me to hand-edit
allowed-tools: Read, Grep, Glob, Write, Bash(ls:*), Bash(pwd:*), Bash(hobby-mml-parser:*), Bash(git -C * rev-parse:*), Bash(git -C * status:*)
---

Scaffold a skeleton `.mml` spec for a **new** unit of behaviour — one that has
no code yet — so I can edit it into the shape I want before
`mml-translate` turns it into code. This is the "building a new feature"
entry to the MML loop (its sibling `mml-extract` is the "adjusting an
existing feature" entry: it goes code → `.mml`; this skill goes idea → `.mml`).

There are two paths, and you pick from what I give you:

- **Blank template** — I want to write the spec by hand. Your job is minimal:
  drop a correctly-structured empty spec at the right path so I start
  well-formed.
- **Draft from a rough idea** — I describe the feature in a sentence or two and
  you draft a first-pass `.mml` (best-guess modules / data / behavior /
  invariants) that I then edit down. **The draft is a starting point, explicitly
  not final** — a thinking aid to react to, not a spec to trust. Err toward too
  few behavior steps that I add to over an over-specified guess.

Write the file and **stop** — reviewing, editing, and committing it are my job
(same contract as extract and translate). The spec becomes real when I edit it
and commit it as the baseline `mml-translate` diffs against.

Read `~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/grammar.md` (the authoritative
hobby grammar, v2 — **not** the work one) before your first scaffold if you are
unsure of any rule below. The grammar cheat-sheet and worked examples at the end
of this file are the reference — match their **altitude** and formatting.

**Bevy is the common case here.** Most hobby projects are Bevy games, and a Bevy
unit has no input and output — data flows through the World. If the feature is a
plugin or a system, scaffold `kind: plugin` / `kind: system` with the Bevy
sections, not `## data` with `in:`/`out:`.

## Instructions

1. **Pick the entry path.** If I just want a file to fill in myself → **blank
   template** (step 3). If I give a rough description of what the unit should do
   → **draft from the idea** (step 4). When it is genuinely unclear which I
   want, ask **one** question rather than guessing.

2. **Pin down the header.** Every scaffold needs these header values; take them
   from what I said or ask **one** clarifying question:

   - `unit` — a unique identifier (kebab-case): verb-first for endpoints
     (`get-product-identifier-mapping`); the message/job name for workflows/jobs
     (`reload-shader-batch`, `prune-stale-chunk-cache`). This is the
     filename (`<unit>.mml`), so it must be unique within the task dir.
   - `kind` — exactly one of `endpoint` | `workflow` | `job` | `plugin` |
     `system`. A Bevy feature is a `plugin`; a single system worth its own file
     is a `system`. When I describe a game feature and say nothing about the
     shape, assume `plugin`.
   - `system` — optional free-text label for the conceptual product grouping
     (e.g. `bevy-survivor`, `terrain-erosion`). It is **not** a path component and
     no longer needs to be kebab-case; omit it if there is nothing useful to
     say.

   The file lives at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.mml`,
   where `{topic}` and `{task}` are the first and second directories under
   `~/fun_worktrees` on the current worktree path
   (`~/fun_worktrees/{project}/{topic}/{task}`) — the same task/topic detection
   `resume-journal` uses. Use `{task}` verbatim (including any blurb
   suffix); fall back to the git branch name if not under `~/fun_worktrees`. There
   is no `<system>` directory — the file sits directly in the task dir and its
   name is the `unit:` header value + `.mml`.

3. **Blank template.** Copy the skill asset matching the `kind`. Only `skill.md`
   is symlinked into the config dir, so read the asset from the repo by its full
   path:

   | `kind` | asset |
   |---|---|
   | `endpoint` / `workflow` / `job` | `~/fun_repos/hobby-tooling/skills/mml-new/template.mml` |
   | `plugin` | `~/fun_repos/hobby-tooling/skills/mml-new/template-plugin.mml` |
   | `system` | `~/fun_repos/hobby-tooling/skills/mml-new/template-system.mml` |

   Copy it to `{topic}/{task}/<unit>.mml`, then replace only the placeholder
   header values (`my-system` / `my-unit` / the sample `kind`) with the real
   ones. **Leave the section bodies as the template's comment prompts** — the
   point is a pleasant, well-formed file I fill in by hand, with the LSP guiding
   me. Delete any *optional* Bevy section the feature genuinely has nothing to
   say in (a plugin with no events needs no `## events`), but keep the required
   ones. Skip to step 7.

4. **Draft from the idea.** Turn my one-or-two-sentence idea into a best-guess
   spec at mental-model altitude — the steps I would narrate to a colleague, not
   an implementation. Populate the sections the `kind` takes:

   - **`## modules`** — the behavioural collaborators you'd expect the unit to
     have (`PascalName — responsibility`). Guess a handful (~4); do not invent a
     sprawling design. Omit `@ path` — there is no code yet. For a Bevy kind
     this is optional and usually just the plugin type; the systems go in
     `## systems`.
   - **`## data`** *(classic kinds only)* — `struct`/`enum` defs for the shapes
     the idea implies, then
     `in:` / `out:`. Every type must resolve to a primitive, an opaque atom
     (`Uuid`/`Money`/`DateTime`), a container (`Option`/`Vec`/`Map`/`Queue`/
     `Stream`), or a struct/enum you define here — an undefined type is a hard
     grammar error. Workflows/jobs usually take `Queue<T>` / emit `Stream<T>`;
     endpoints use a request struct / response type. If a shape the idea needs
     is already defined by another unit in the **same task dir** (check the
     sibling `.mml` files), bring it in with a `use <unit>: Type` line at the
     top of `## data` rather than redeclaring it.
   - **`## components` / `## resources` / `## events`** *(Bevy kinds)* — the
     shapes the idea implies, split by ECS role: entity data, global singletons,
     message payloads. A zero-sized tag is `marker Name`, not an empty struct.
     Skip a section entirely when the idea implies nothing for it. Do **not**
     declare Bevy's own types (`Transform`, `Time`, …) — they are built in.
     Reuse across the task dir with `use <unit>: Type` at the top of the section
     that gives it its role.
   - **`## systems`** *(kind: plugin)* — guess ~2–4 systems, each a snake_case
     name, a one-line responsibility, and an access block. `schedule:` is
     required; add `after:` where the ordering is part of the idea, and the
     `query:` / `reads:` / `writes:` / `spawns:` / `emits:` lists the idea
     implies. Omit keys the system does not use — an empty list is noise.
     Err toward too few systems that I split, over an over-decomposed guess.
   - **`## access`** *(kind: system)* — the same block, unindented, for the one
     system this file specs.
   - **`## behavior`** — nested `-` bullets, **2 spaces per level (spaces
     only)**. Control flow uses only the closed keyword set (`if …:` / `else:` /
     `for each …:` / `while …:` / `try:` / `on error …:`, trailing `:`, body
     nested); everything else is prose. Reference declared module/struct/enum
     names **verbatim** so the LSP cross-links them. Use literal `X` / `Y` / `N`
     for abstract quantities rather than baking in a number. When a step
     delegates to an **already-specced** unit (check the sibling `.mml` files in
     the same task dir), end it with `@ spec <unit>` instead of restating that
     unit's behaviour; never ref a spec that doesn't exist.
   - **`## invariants`** — flat `-` bullets, each a checkable property the idea
     implies (a cap, an ordering guarantee, an idempotency claim).

5. **Mark it a draft.** For the draft path, make the first line a comment so it
   is unmistakably provisional:

   ```
   # DRAFT — best-guess scaffold from a rough idea; edit me down before translating.
   ```

6. **`source` — only when the target is known.** A brand-new unit usually has no
   code yet, so leave `source` out. If the idea names the repo and files the new
   code should live in, add the block with the **intended** new paths
   (translate's full-generation mode creates files there); otherwise leave a
   comment prompt:

   ```
   # source: (add repo + intended paths once the target is decided)
   ```

7. **Sanity-check the structure.** Confirm: header has `unit`/`kind` (kind in
   the closed set, `system` optional) and the filename equals `<unit>.mml`; the
   four `##` sections are present **in order** (`modules`, `data`, `behavior`,
   `invariants`); indentation is uniform 2-space; behavior control flow uses
   only the closed keyword set; every `## data` type resolves.

8. **Write, lint, stop.** Write to
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.mml`,
   then run `hobby-mml-parser <written-path> --lint` and fix every diagnostic until it
   exits clean (a skeleton without a `source` block lints clean — the structure
   checks still apply; if you filled in `source`/`@ path`s, an
   `unresolved-source-path` warning means the path is wrong — fix the path,
   don't drop the reference). If the binary is missing, validate by eye against
   the cheat-sheet. Do **not** run `git add` / `git commit` in the notes repo.
   Report the path you wrote and the next step: **I** edit the draft, commit it
   as the baseline, then run `/mml-translate <unit>` to turn it into
   code.

## Grammar cheat-sheet

- File = header, then `##` sections in canonical order: `modules`, `data`,
  `components`, `resources`, `events`, `systems`, `access`, `behavior`,
  `invariants`. **Which are legal depends on `kind`:**
  - `endpoint`/`workflow`/`job` — exactly `modules`, `data`, `behavior`,
    `invariants`, all required (a body may be empty except `behavior`).
  - `plugin` — `systems`, `behavior`, `invariants` required; `modules`,
    `components`, `resources`, `events` optional; no `data`, no `access`.
  - `system` — `access`, `behavior`, `invariants` required; `modules`,
    `components`, `resources`, `events` optional; no `data`, no `systems`.
- Header keys: `unit` (req, identifier), `kind` (req: `endpoint`|`workflow`|
  `job`|`plugin`|`system`), `system` (opt: free-text label, not a path
  component), `source` (opt: `repo` + `paths` list).
- `#` at column 0 is a comment; `##` at column 0 is always a section header.
- Indent = spaces only, 2 spaces per nesting level.
- `## modules`: `PascalName — responsibility [@ repo-relative-path]`.
- `## data`: `struct Name { field: Type, ... }`, `enum Name { Variant, ... }`
  (unit variants, no payloads), then `in: <Type>` / `out: <Type>`. Types:
  primitives, `Uuid`/`Money`/`DateTime`, `Option<T>`/`Vec<T>`/`Map<K,V>`/
  `Queue<T>`/`Stream<T>`, or a defined struct/enum. Undefined type = hard error.
- Type imports (grammar §4.5): `## data` may start with `use unit: Type, …`
  lines that bring a data model defined by another unit into scope. The target
  is a bare `<unit>` resolving to a sibling `<unit>.mml` in the same task dir.
  Reuse a shared type this way instead of redeclaring it; resolves like
  `@ spec`. Unresolved target/type = warning.
- `## behavior`: nested `-` bullets; control-flow keywords
  `if…: / else: / for each…: / while…: / try: / on error…:` (trailing `:`, body
  nested); everything else prose; declared names cross-link.
- `## invariants`: flat `-` bullets, each a checkable claim.
- Cross-spec refs (grammar §5.4): a module line, **system entry**, behavior step,
  or invariant may end with `@ spec <unit>` — "more specification under" that
  unit's `.mml`. At most one per line, always the last annotation (after an
  `@ path`); the target is a bare `<unit>` resolving to a sibling `<unit>.mml` in
  the same task dir. Unresolved target = warning, not error.

**Bevy dialect (grammar §8):**

- `## components` / `## resources` / `## events`: the `## data` type grammar
  minus `in:`/`out:`, plus `marker Name` (a zero-sized component, `## components`
  only). **The section assigns the ECS role**, for `use` imports too. The same
  name may not be declared in two of them.
- Bevy's own types need no declaration — `Transform`, `Time`, `AssetServer`,
  `AppExit` and friends are built in with their roles.
- `## systems`: `snake_name — responsibility [@ path] [@ spec unit]` at column 0,
  each followed by an indented access block. `## access` is the same block,
  unindented, for a `kind: system` file.
- Access keys (each at most once): `schedule:` (**required**), `set:`, `after:`,
  `before:`, `run_if:`, `query:`, `reads:`, `writes:`, `spawns:`, `despawns:`,
  `emits:`, `consumes:`.
- `query:` is `A + B - C` (`+` requires, `-` excludes) and takes **components**;
  `spawns:`/`despawns:` components; `emits:`/`consumes:` **events**;
  `reads:`/`writes:` either a component or a **resource**. Wrong role = error;
  unresolvable name = error.
- Schedules: `Update`, `FixedUpdate`, `Startup`, `PostUpdate`, … plus
  `OnEnter(X)`/`OnExit(X)`. An unknown label is a warning, not an error.
- `after:`/`before:` should name a system or `set:` in the same file (warning
  otherwise) — checked only for `kind: plugin`.

## Worked examples

### Draft from a rough idea — a scheduled `job`

I say: *"a nightly job that purges price-history older than the retention
window, per contract, in batches."* You draft (note the `# DRAFT` marker, no
`source` yet, and behavior kept deliberately sparse for me to sharpen):

```mml
# DRAFT — best-guess scaffold from a rough idea; edit me down before translating.
system: bevy-survivor
unit: prune-stale-chunk-cache
kind: job
# source: (add repo + intended paths once the target is decided)

## modules
PurgeScheduler    — scheduled entry point: enumerates contracts and drives the purge
PriceHistoryStore — deletes price-history rows older than the cutoff, in batches
PurgeReporter     — records how many rows were deleted per contract

## data
struct Contract {
  contract_id: Uuid,
  retention_days: u32,
}
struct PurgeResult {
  contract_id: Uuid,
  deleted_rows: u64,
}

in:  Stream<Contract>
out: Stream<PurgeResult>

## behavior
- for each Contract:
  - compute the cutoff as now minus the contract's retention window
  - while price-history rows older than the cutoff remain:
    - delete the next batch of at most X rows via PriceHistoryStore
  - record a PurgeResult for the contract via PurgeReporter
- emit each PurgeResult downstream

## invariants
- rows newer than the retention window are never deleted
- each batch deletes at most X rows, so the job never holds a long-running transaction
- a contract with no stale rows produces a PurgeResult with deleted_rows = 0
```

Then stop and tell me to edit it — maybe retention is global, not per-contract;
maybe the reporter isn't its own module. That reaction is the point of the
draft.

### Blank template — hand-authoring

I say: *"give me a blank endpoint spec for bevy-survivor called
`spawn-wave` to fill in myself."* You copy `template.mml`, set the
header, and leave the section prompts intact:

```mml
# MML spec template — copied to notes/{project}/{topic}/{task}/<unit>.mml and filled in.
# Lines starting with # at column 0 are comments and are ignored by the parser.
# ...

system: bevy-survivor
unit: spawn-wave
kind: endpoint
# source:                # optional — fill in once the target is known

## modules
# One line per behavioural collaborator:  PascalName — responsibility [@ repo-relative-path]

## data
# struct/enum defs, then in:/out:

## behavior
# Nested `-` bullets at mental-model altitude; 2 spaces per level.
- describe the first step here

## invariants
# One `-` bullet per property that must always hold.
```

The bodies stay as comment prompts (behavior keeps the one placeholder bullet so
the file is well-formed) — I fill them in, commit, then translate.

### Draft from a rough idea — a Bevy `plugin`

I say: *"enemy waves — a timer spawns a wave of enemies at the spawn points,
and something fires when the wave is cleared."* You draft (note: no `## data`,
no `in:`/`out:`; `Transform` and `Time` are used without being declared; the
behavior bullets are the system names):

```mml
# DRAFT — best-guess scaffold from a rough idea; edit me down before translating.
system: bevy-survivor
unit: enemy-waves
kind: plugin
# source: (add repo + intended paths once the target is decided)

## components
struct Wave {
  index: u32,
}
struct EnemyHealth {
  hp: f32,
}
marker Spawner

## resources
struct WaveConfig {
  interval_secs: f32,
  base_count: u32,
}
struct WaveState {
  current: u32,
  elapsed: f32,
  due: bool,
}

## events
struct WaveCleared {
  index: u32,
}

## systems
tick_wave_timer — advances the wave clock and marks the next wave due
  schedule: FixedUpdate
  reads:    WaveConfig, Time
  writes:   WaveState
spawn_enemies — spawns the next wave at each spawn point
  schedule: FixedUpdate
  after:    tick_wave_timer
  run_if:   WaveState.due
  query:    Transform + Spawner
  reads:    WaveConfig
  writes:   WaveState
  spawns:   EnemyHealth, Wave
detect_cleared — emits WaveCleared once no enemies of the current wave remain
  schedule: Update
  query:    Wave + EnemyHealth
  reads:    WaveState
  emits:    WaveCleared

## behavior
- tick_wave_timer:
  - advance WaveState.elapsed by the frame delta from Time
  - if elapsed exceeds WaveConfig.interval_secs:
    - mark WaveState due and reset elapsed
- spawn_enemies:
  - for each Spawner:
    - spawn N enemies at the spawner's Transform
- detect_cleared:
  - if no entity carries a Wave for the current index:
    - emit WaveCleared for that index

where:
  N = 8 @ WaveConfig.base_count

## invariants
- WaveState.current only ever increases
- exactly one WaveCleared is emitted per wave index
```

Then stop and tell me to edit it — maybe the timer belongs in the spawn system;
maybe wave size is a curve, not a base count; maybe `detect_cleared` should run
in `PostUpdate`. That reaction is the point of the draft.
