---
description: Analyze one unit of existing code and emit a .hmml (Mental Model Language) spec
allowed-tools: Read, Grep, Glob, Write, Bash(ls:*), Bash(pwd:*), Bash(hobby-mml-parser:*), Bash(git -C * rev-parse:*), Bash(git rev-parse:*), mcp__tree-extract__build_graph, mcp__tree-extract__list_modules, mcp__tree-extract__get_neighbors, mcp__tree-extract__call_path, mcp__tree-extract__get_symbol, mcp__tree-extract__search_symbols, mcp__lsp-mcp__document_symbol, mcp__lsp-mcp__find_references, mcp__lsp-mcp__goto_definition, mcp__lsp-mcp__hover
---

Analyze one unit of an existing system — an endpoint, a message/queue workflow,
or a scheduled job — and emit a valid `.hmml` spec capturing my mental model of
it (modules / data / behavior / invariants). Emit the file and stop; editing it
is the human's job — that is the point of the artifact.

A `.hmml` is a **lossy mental-model view, not a regeneration source.** Dropping
infrastructure the mental model doesn't dwell on — DI wiring, config loading,
metrics, serializers — is *correct*, not a miss: aim for the collaborators and
decisions a human would narrate, and accept that the code has more than the spec
shows. (The round-trip eval, task 08, confirms full-regeneration from a spec is
lossy by design; the spec's job is to be edited and diffed, not to rebuild the
unit.)

Read `~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/grammar.md` (the authoritative
hobby grammar, v2 — **not** the work one) before your first extraction if you
are unsure of any rule below. The grammar cheat-sheet and worked examples at the
end of this file are the reference — match their **altitude** and formatting.
The always-parsing examples live in
`~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/mml-parser/tests/fixtures/examples/`.

**Bevy is the common case here.** Most hobby repos are Bevy games, and a Bevy
unit does not have an input and an output — data flows through the World. If the
code you are extracting is a `Plugin` or a system, use `kind: plugin` /
`kind: system` and the Bevy sections ([step 4b](#4b-extracting-bevy-code)), not
`## data` with `in:`/`out:`.

## Instructions

1. **Identify the unit.** The human points at one unit (e.g. "the
   `/tiles/{z}/{x}/{y}` endpoint", "the shader reload batch worker", "the
   chunk mesher") or a file/function. You trace the rest. Pin down:

   - `system` — the conceptual product grouping (kebab-case, e.g.
     `bevy-survivor`, `terrain-erosion`). **Not** the git repo name.
   - `unit` — a unique-within-system identifier (kebab-case): verb-first for
     endpoints (`get-map-tile`); the message/job name for
     workflows/jobs (`reload-shader-batch`, `mesh-terrain-chunks`); the plugin's
     feature name for Bevy (`enemy-waves`), or the fn name for a single system
     (`spawn-enemies`).
   - `kind` — exactly one of `endpoint` | `workflow` | `job` | `plugin` |
     `system`. A `impl Plugin for X` is a `plugin`; one system worth its own
     file is a `system`; everything else is a classic kind.
   - the **entry point**: the controller action + route for an endpoint, the
     consumer/`process`-function for a workflow, the `main`/scheduled function
     for a job. This is where the behavior walk starts and where `in:`/`out:`
     come from.

   If the human's pointer is ambiguous (which of two overlapping units, which
   system name), ask **one** clarifying question rather than guessing.

2. **Locate the source repo and map the structure.** Resolve the code to a local
   checkout under `~/fun_repos/<repo>` (the `source.repo` value; confirm the dir
   exists). Then build the module map — prefer the graph, fall back to reads:

   a. **`tree-extract` MCP** for the module map. If no graph exists for the repo
   (`.project-graph.json` absent), `build_graph` first. Then `list_modules`,
   `get_symbol` on the entry point, and `get_neighbors` / `call_path` to walk
   the unit's call graph and find the classes/modules it actually touches. Keep
   only modules *in this unit's* graph — not the whole repo.

   b. **`lsp-mcp`** (`document_symbol`, `goto_definition`, `find_references`,
   `hover`) for precise signatures and to resolve a type to its definition.

   c. **Direct reads** (`Read`, `Grep`) for the behavior walk — the graph tells
   you *what* calls what; the source tells you *what it does*.

3. **`## modules`** — one line per module the unit touches:
   `Name — responsibility @ path`.

   - A module is a **behavioral collaborator** — something that *does* work in
     the `## behavior`. Do **not** list a pure data type / wire format (model it
     in `## data` instead) or a one-shot helper (a mapper, a serializer, a
     formatter) as its own module; fold those into the module or behavior step
     that uses them. Aim for the fewest modules that still tell the story — the
     worked examples have ~4; if you are past ~7, you are probably transcribing
     the file list rather than naming collaborators.
   - `Name`: PascalCase, referenceable, unique in the file. Prefer the real
     class/module name; collapse a cluster of tiny collaborators into one named
     module when that matches the mental model (e.g. one `MeshCache` even if the
     code splits eviction/lookup/insert across files) and say so in the
     responsibility.
   - `responsibility`: one line of prose, what it is *for* — not a method list.
   - `—` is a space–em-dash–space separator (`--` is also accepted, but emit the
     em dash to match the examples).
   - ` @ path` is optional but fill it whenever the module maps to a concrete
     file; path is **repo-relative** to the `source.repo` checkout.
   - Align the `—` separators with padding spaces as in the examples.

4. **`## data`** — the in/out data model as a Rust-like subset. Definitions
   first, then `in:` / `out:`. **Classic kinds only** — for `plugin` / `system`
   skip to [step 4b](#4b-extracting-bevy-code).

   - Derive `struct`s from the request/response and the types that flow through
     the entry point. Field names `snake_case`; translate source types into the
     HMML subset:
     - **When the input is loose params, not a single type** (e.g. a GET whose
       args are query/path parameters, so there is no request DTO in code), you
       may **synthesize** a request `struct` for the mental model — but emit a
       column-0 `# synthesized: <why>` comment on the line above it, so
       translate and validate know the type is conceptual and must **not** be
       materialized as a real code type (or flagged as drift for not existing).
     - primitives `String bool i8..i64 u8..u64 f32 f64`; opaque atoms `Uuid`,
       `Money`, `DateTime`; containers `Option<T>`, `Vec<T>`, `Map<K,V>`.
     - Rust maps almost 1:1; `&str`/`String→String`, `usize→u64`,
       `HashMap<K,V>→Map<K,V>`. Zig `[]const u8→String`, `?T→Option<T>`,
       `[]T→Vec<T>`. Python `str→String`, `int→i64`, `list[T]→Vec<T>`,
       `dict→Map<K,V>`, `Optional[T]→Option<T>`.
   - Emit an `enum` (unit variants, **no payloads**, any case) for a closed
     value set — e.g. `enum TileFormat { png, webp }`,
     `enum ReloadOutcome { swapped, unchanged, rejected, skipped }`.
   - `in:` and `out:` are one type expression each. Workflows/jobs use
     `Queue<T>` / `Stream<T>`; endpoints use the request struct / response type.
     Align `in:`/`out:` as in the examples.
   - **Endpoints:** set the `method:` header (`GET`/`POST`/…) and annotate the
     `in:` struct's fields with their wire source — `zoom: u8 @path`,
     `format: TileFormat @query`, `body_field: T @body` — so the spec records
     *how* the endpoint is called, not only its input type. Read them off the
     route + controller signature (path params, query params, JSON body).
   - Every type used must be a primitive, an opaque atom, a container, or a
     struct/enum you defined here — an undefined type is a hard error in the
     grammar. Don't over-model: capture the shape that matters to the behavior,
     not every field of every DTO.
   - **Reuse before redeclaring**: before defining a struct/enum, check whether
     a sibling unit in the same task dir already specs that same data model
     (scan the other `.hmml` files in the task dir). If so, bring it in with a
     `use unit: Type` line at the top of `## data` instead of copying the
     definition — the shared type then has one home and can't drift. Import from
     the unit that *owns* the type (defines it locally), not one that itself
     imports it.

### 4b. Extracting Bevy code

For `kind: plugin` / `kind: system` there is no `## data` and no `in:`/`out:`.
Read the access set off the code instead — it is the interface.

- **`## components` / `## resources` / `## events`** — one section per role.
  Source them from the derives:
  - `#[derive(Component)]` → `## components`. A unit struct
    (`struct Spawner;`) becomes `marker Spawner`, not an empty struct.
  - `#[derive(Resource)]` → `## resources`.
  - `#[derive(Event)]` / `#[derive(Message)]` → `## events`.
  - Field types map as in step 4, plus the Bevy atoms `Entity`, `Vec2`, `Vec3`,
    `Quat`, `Color`, `Timer`, `Duration`, `Handle<T>`.
  - **Do not declare Bevy's own types.** `Transform`, `Time`, `AssetServer`,
    `AppExit` and friends are known to the parser. Declaring one locally
    shadows the built-in and is almost always a mistake.
  - Reuse before redeclaring, exactly as in step 4: `use enemy-waves: Wave` at
    the top of the world section that gives it its role.

- **`## systems`** — one entry per system the plugin registers. Read them off
  `app.add_systems(<Schedule>, (a, b).chain())` and friends:

  ```
  <fn_name> — <what it does> [@ <path to the fn>] [@ spec <unit>]
    schedule: FixedUpdate        # the add_systems schedule argument
    set:      WaveSpawning       # .in_set(..)
    after:    tick_wave_timer    # .after(..) — and .chain() implies after
    before:   render_hud         # .before(..)
    run_if:   WaveState.due      # .run_if(..), as prose
    query:    Transform + Spawner - Dead
    reads:    WaveConfig, Time
    writes:   WaveState
    spawns:   EnemyHealth, Wave
    despawns: Wave
    emits:    WaveCleared
    consumes: WaveCleared
  ```

  Derive the lists from the system's parameters:
  - `Query<&T, With<A>>` → `query: T + A`; `Without<B>` → `- B`.
  - `&mut T` inside a query → also list `T` under `writes:`.
  - `Res<T>` → `reads: T`; `ResMut<T>` → `writes: T`.
  - `EventWriter<E>` → `emits: E`; `EventReader<E>` → `consumes: E`.
  - `commands.spawn((A, B))` → `spawns: A, B`; `.despawn()` → `despawns:`.
  - Omit a key entirely when the system does not use it — an empty list is
    noise. `schedule:` is the one key that is always required.

  Keep the responsibility at mental-model altitude; the access block already
  carries the mechanics, so do not restate them in prose.

- **When a system is too big for one entry**, give it its own `kind: system`
  file and point at it: `spawn_enemies — … @ spec spawn-enemies`. The split file
  carries the same access block as a `## access` section and imports the shapes
  it touches.

- **`## modules`** is optional for Bevy kinds — keep it for the non-system
  collaborators (the `impl Plugin` type itself, an asset loader, a helper),
  not for the systems, which have their own section.

5. **`## behavior`** — **this is the whole game.** Nested `-` bullets at
   *mental-model altitude*: the steps I would narrate to a colleague, not a
   transcription of the code.

   For `kind: plugin`, make the top-level bullets the **system names with a
   trailing `:`** and nest each system's logic under it — the LSP then
   cross-links each bullet to its `## systems` entry.

   - **Altitude test**: each step is a decision or an action a human cares about
     ("get a proxy from ProxyPool", "if a cookie banner is present: click accept
     cookies"). If a step names a local variable, a specific method call, or a
     language construct, it is too low — lift it. If a step could describe ten
     different implementations, it is too high — sharpen it.
   - Indentation is **2 spaces per level** (spaces only). A step's children are
     indented one level deeper.
   - Control flow uses only this closed keyword set, each ending in `:` with its
     body nested beneath it: `if <cond>:` · `else:` · `for each <x>[ in <y>]:` ·
     `while <cond>:` · `try:` · `on error <e>:`. Everything else is free prose.
   - **Reference module/struct/enum names verbatim** in the prose (`ProxyPool`,
     `Offer`, `Category`) so the LSP cross-links them to their definitions.
   - Use literal `X` / `Y` / `N` for abstract quantities (parallelism,
     thresholds, retry caps) rather than baking in a config number — the mental
     model is "at most X in flight", not "at most 4". When a symbol's concrete
     value and where it lives in code are worth pinning, add a `where:` block at
     the **end of `## behavior`** binding them: `X = 4 @ TERRAIN_MESH_WORKERS` /
     `Y = 3 @ max_sessions` — `value @ config-source` (env var, constant, or
     parameter default). The block is optional; bind the symbols whose value a
     human might realistically want to change.
   - Wrap the risky span in `try:` / `on error e:` when the unit has a real
     failure/retry story (see the shader-reload example).
   - **Cross-spec refs**: when a step delegates to behaviour that is itself a
     specced unit, do not inline that unit's details — keep the step at one line
     and end it with `@ spec unit`, resolving to a sibling `<unit>.hmml` in the
     same task dir. Check the other `.hmml` files in the task dir for an existing
     spec before adding a ref; never invent a ref to a spec that doesn't exist.

6. **`## invariants`** — one `-` bullet per property that must always hold, flat
   (not nested), each a **checkable claim**. Surface: concurrency/parallelism
   caps, ordering guarantees, retry/dead-letter caps, idempotency, uniqueness,
   auth preconditions. Read them off the code (a semaphore, a prefetch count, a
   unique index, a max-requeue constant), not off intuition.

7. **Assemble and write the file.** Header first (`system`, `unit`, `kind`,
   `method` for endpoints, then the `source` block), then the four `##` sections
   **in this exact order**: `## modules`, `## data`, `## behavior`,
   `## invariants`. Then write to the current task's notes dir:

   ```
   ~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml
   ```

   where `{project}`, `{topic}`, and `{task}` are the first, second, and third
   directories under `~/fun_worktrees` (from
   `~/fun_worktrees/{project}/{topic}/{task}` — the task dir is the worktree
   itself, there is no repo level below it; use `{task}` verbatim, including any
   blurb suffix, and fall back to the git branch if the path doesn't match — the
   same detection `resume-journal` uses). The
   filename is the `unit:` header + `.hmml`, placed **directly** in the task dir
   (no `<system>` subdirectory). The `system:` header is now an optional label,
   not a path component. Fill the `source` block:

   ```
   source:
     repo: <repo>            # resolves to ~/fun_repos/<repo>
     paths:
       - <repo-relative path>   # must resolve to a real file
       - ...
   ```

   Do **not** put a sha or timestamp anywhere in the `.hmml` — the grammar stays
   pure.

8. **Lint the written file.** Run `hobby-mml-parser <written-path> --lint` and fix
   every diagnostic until it exits clean. An `unresolved-source-path` warning
   means a `source.paths` entry or module `@ path` names a file that does not
   exist — go back to the checkout and find the real path; **never delete the
   reference just to silence the warning** (a spec without working linkage is
   the bug this step exists to prevent: goto-definition silently no-ops on it).
   If the `hobby-mml-parser` binary is not installed
   (`cargo install --path ~/fun_repos/hobby-tooling/lsps/hobby-mml-lsp/mml-parser`),
   fall back to verifying each path with `ls` against the checkout root.

9. **Stop.** Report the path you wrote and a one-line summary. Do **not**
   iterate on the spec, and do **not** run `git add` / `git commit` in the notes
   repo or the tooling repo — reviewing and editing the `.hmml`, and committing
   it, are the human's job.

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
- Header keys: `system` (req), `unit` (req), `kind` (req: `endpoint`|`workflow`|
  `job`|`plugin`|`system`), `method` (opt, endpoints: `GET`|`POST`|…), `base`
  (opt), `source` (opt: `repo` + `paths` list). `system`/`unit` are identifiers.
- `#` at column 0 is a comment; `##` at column 0 is always a section header.
- Indent = spaces only, 2 spaces per nesting level.
- `## modules`: `PascalName — responsibility [@ repo-relative-path]`.
- `## data`: `struct Name { field: Type [@ source], ... }`,
  `enum Name { Variant, ... }` (unit variants, no payloads), then `in: <Type>` /
  `out: <Type>`. Types: primitives, `Uuid`/`Money`/`DateTime`,
  `Option<T>`/`Vec<T>`/`Map<K,V>`/ `Queue<T>`/`Stream<T>`, or a defined
  struct/enum. Undefined type = hard error. A field's optional `@ source` is
  `path`|`query`|`body`|`header` (endpoints).
- Type imports (grammar §4.5): `## data` may start with `use unit: Type, …`
  lines that bring a data model defined by another unit into scope. The `unit`
  is a sibling `.hmml` in the same task directory. Reuse a shared type this way
  instead of redeclaring it; resolves like `@ spec`. Unresolved target/type =
  warning.
- `## behavior`: nested `-` bullets; control-flow keywords
  `if…: / else: / for each…: / while…: / try: / on error…:` (trailing `:`, body
  nested); everything else prose; declared names cross-link. An optional
  trailing `where:` block binds symbols: `Symbol = value [@ config-source]`.
- `## invariants`: flat `-` bullets, each a checkable claim.
- Cross-spec refs (grammar §5.4): a module line, **system entry**, behavior step,
  or invariant may end with `@ spec unit` — "more specification under" a sibling
  `.hmml` in the same task directory. At most one per line, always the last
  annotation (after an `@ path`); it resolves to `<unit>.hmml` in the referencing
  file's own directory. Unresolved target = warning, not error.

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

Match the altitude and formatting of these (from the finalized grammar). Pick
the one whose `kind` matches your unit.

### Workflow (queue) — `mesh-terrain-chunks.hmml`

```hmml
system: terrain-erosion
unit: mesh-terrain-chunks
kind: workflow
source:
  repo: bevy_terrain_erosion
  paths:
    - src/chunk/scheduler.rs
    - src/chunk/mesher.rs
    - src/chunk/cache.rs

## modules
ChunkScheduler — pulls chunk coords off the request queue and runs X meshing jobs in parallel @ src/chunk/scheduler.rs
HeightSampler  — samples the erosion heightfield for a chunk's extent @ src/chunk/mesher.rs
ChunkMesher    — turns a heightfield tile into vertices, normals, and indices @ src/chunk/mesher.rs
MeshCache      — caches finished meshes keyed by chunk coord and LOD

## data
struct ChunkCoord {
  x: i32,
  z: i32,
  lod: u8,
}
struct HeightTile {
  coord: ChunkCoord,
  resolution: u32,
  heights: Vec<f32>,
}
struct ChunkMesh {
  coord: ChunkCoord,
  vertices: Vec<f32>,
  normals: Vec<f32>,
  indices: Vec<u32>,
}

in:  Queue<ChunkCoord>
out: Stream<ChunkMesh>

## behavior
- pull up to X chunk coords from the queue and process them in parallel
- for each chunk coord:
  - if MeshCache already holds a mesh for the coord and lod:
    - emit the cached ChunkMesh and skip the rest
  - else:
    - sample a HeightTile from HeightSampler at the resolution implied by lod
    - if the tile is entirely below the water plane:
      - emit an empty ChunkMesh (no triangles) so the coord is not re-requested
    - else:
      - build vertices and indices from the heightfield
      - compute normals from neighbouring heights, borrowing one row/column from adjacent tiles so chunk seams match
    - insert the mesh into MeshCache
  - emit the ChunkMesh downstream

## invariants
- at most X chunks are meshed at once
- a chunk coord leaves the queue only after a mesh is emitted or it fails N times
- neighbouring chunks at the same lod agree on the vertex positions along their shared edge
```

### Endpoint — `get-map-tile.hmml`

Note the `@path` / `@query` annotations on the request struct: for an endpoint,
record *how* it is called, not only its input type.

```hmml
system: mosaik-map
unit: get-map-tile
kind: endpoint
method: GET
source:
  repo: mosaik_map
  paths:
    - src/server/routes.rs
    - src/server/tile_handler.rs
    - src/tiles/store.rs

## modules
TileHandler — HTTP layer: validates zoom/coords, renders or serves a cached tile as PNG @ src/server/tile_handler.rs
TileStore   — reads pre-rendered tiles off disk, keyed by zoom/x/y @ src/tiles/store.rs
TileRenderer — rasterizes a tile from vector source data on a cache miss

## data
enum TileFormat {
  png,
  webp,
}
# synthesized: a GET whose args are path + query params, so there is no request type in code
struct TileRequest {
  zoom: u8 @path,
  x: u32 @path,
  y: u32 @path,
  format: TileFormat @query,
  retina: Option<bool> @query,
}
struct Tile {
  bytes: Vec<u8>,
  format: TileFormat,
  from_cache: bool,
}

in:  TileRequest
out: Tile

## behavior
- if zoom is outside the supported range:
  - respond 400 Bad Request naming the valid zoom range
- if x or y is outside the 2^zoom grid for the given zoom:
  - respond 400 Bad Request
- ask TileStore for the tile at zoom/x/y in the requested format
- if the store has it:
  - respond 200 OK with the bytes and a long cache-control header
- else:
  - render the tile with TileRenderer at 2x resolution when retina is set
  - if the source data covers no features in this tile's extent:
    - respond 204 No Content so the client stops asking
  - else:
    - write the rendered tile into TileStore and respond 200 OK

## invariants
- zoom, x, and y are validated before any disk or render work happens
- a rendered tile is always written to the store before being returned, so a tile is rendered at most once
- the response format always equals the requested format
```

### Workflow (with error handling) — `reload-shader-batch.hmml`

Note the `try` / `on error` form and the per-item skip decision.

```hmml
system: shader-runner
unit: reload-shader-batch
kind: workflow
source:
  repo: bevy_shader_runner
  paths:
    - src/watch/debouncer.rs
    - src/watch/reload.rs
    - src/pipeline/registry.rs

## modules
ChangeDebouncer  — collapses filesystem events into one batch per quiet period @ src/watch/debouncer.rs
ShaderCompiler   — runs naga validation on a WGSL source and reports diagnostics @ src/watch/reload.rs
PipelineRegistry — swaps validated shader modules into the live render pipelines @ src/pipeline/registry.rs
DiagnosticSink   — surfaces compile errors as an on-screen overlay

## data
enum ReloadOutcome {
  swapped,
  unchanged,
  rejected,
  skipped,
}
struct ShaderChange {
  path: String,
  source: String,
  digest: String,
}
struct ReloadBatch {
  batch_id: Uuid,
  changes: Vec<ShaderChange>,
}
struct ReloadResult {
  batch_id: Uuid,
  path: String,
  outcome: ReloadOutcome,
  diagnostics: Vec<String>,
}

in:  Queue<ReloadBatch>
out: Stream<ReloadResult>

## behavior
- consume one ReloadBatch; ChangeDebouncer has already collapsed duplicate events for the same path
- log batch_id and the number of changed shaders
- try:
  - for each change in the batch:
    - if the digest matches the digest already registered for that path:
      - record an unchanged result and move on
    - else:
      - if the path is not referenced by any live pipeline:
        - record a skipped result — nothing to swap into
      - else:
        - validate the source with ShaderCompiler
        - if validation produced errors:
          - record a rejected result carrying the diagnostics and keep the previous module bound
        - else:
          - swap the module into PipelineRegistry and record a swapped result
  - if any result was rejected:
    - push the collected diagnostics to DiagnosticSink
- on error e:
  - log the failure, leave every pipeline on its previous module, and drop the batch without retrying

## invariants
- a pipeline is never left bound to a module that failed validation
- every change in the batch produces exactly one ReloadResult
- a failed batch is dropped rather than retried — the next filesystem event re-triggers it
```

### Bevy plugin — `enemy-waves.hmml`

Note what is *not* here: no `## data`, no `in:`/`out:`. The interface is the
access blocks. `Transform` and `Time` are Bevy built-ins, so they are used
without being declared; `Spawner` is a unit struct in the code and so becomes a
`marker`.

```hmml
system: bevy-survivor
unit: enemy-waves
kind: plugin
source:
  repo: bevy_survivor
  paths:
    - src/waves/mod.rs
    - src/waves/timer.rs

## modules
WavePlugin — builds the plugin: registers the types below and adds the systems @ src/waves/mod.rs

## components
struct Wave {
  index: u32,
  remaining: u32,
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
tick_wave_timer — advances the wave clock and marks the next wave due @ src/waves/timer.rs
  schedule: FixedUpdate
  reads:    WaveConfig, Time
  writes:   WaveState
spawn_enemies — spawns the next wave at each spawn point @ spec spawn-enemies
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
    - spawn N enemies at the spawner's Transform, where N grows with WaveState.current
    - give each one EnemyHealth and a Wave tagged with the current index
  - clear the due flag and advance WaveState.current
- detect_cleared:
  - count the entities carrying a Wave for the current index
  - if none remain:
    - emit WaveCleared for that index

where:
  N = 8 @ WaveConfig.base_count

## invariants
- no entity carries both Spawner and EnemyHealth
- WaveState.current only ever increases
- exactly one WaveCleared is emitted per wave index
```

### Bevy system — `spawn-enemies.hmml`

The split-out form: one system, its access block as a section, and the shapes it
touches imported rather than redeclared.

```hmml
system: bevy-survivor
unit: spawn-enemies
kind: system
source:
  repo: bevy_survivor
  paths:
    - src/waves/spawn.rs

## components
use enemy-waves: Wave, EnemyHealth, Spawner
struct SpawnBudget {
  remaining: u32,
}

## resources
use enemy-waves: WaveConfig, WaveState

## access
schedule: FixedUpdate
set:      WaveSpawning
after:    tick_wave_timer
run_if:   WaveState.due
query:    Transform + Spawner - SpawnBudget
reads:    WaveConfig, Time
writes:   WaveState
spawns:   EnemyHealth, Wave, SpawnBudget

## behavior
- compute the wave size as WaveConfig.base_count scaled by WaveState.current
- for each Spawner:
  - split the wave size evenly across the spawners, giving the remainder to the first
  - while the spawner's share is not exhausted:
    - pick a position by jittering the spawner's Transform within radius X
    - spawn an entity with EnemyHealth, a Wave tagged with WaveState.current, and that position
- clear WaveState.due and advance WaveState.current

where:
  X = 3 @ constant SPAWN_JITTER

## invariants
- the number of entities spawned equals the wave size, regardless of spawner count
- every spawned entity carries both EnemyHealth and Wave
- a spawner already carrying SpawnBudget is skipped, so a wave is never spawned twice
```
