---
title: "Hobby HMML — language specification (v3, the PR delta, Bevy dialect)"
created: 2026-07-25
tags:
  - spec
  - reference
---

# HMML — Mental Model Language (v3: the PR delta)

**This file is the source of truth for the HMML grammar.** `/hmml-extract`,
`hobby-mml-parser` and `hobby-mml-lsp` implement exactly what is defined here;
`tree-sitter-hmml` lives in the Helix fork and follows it.

HMML is the hobby dialect of the work MML and its files are **`.hmml`**. The
work dialect keeps `.mml` and its own `mml-lsp`; the distinct extension is what
lets both live on one machine. v3 is work MML v2 plus a light Bevy layer: a unit
may name what runs it (§7.1), and a `## data` type may carry its ECS role
(§6.1).

An `.hmml` file is the **review delta of one pull request**. It says what the PR
changes at the altitude you would use to narrate it to a colleague, and every
changed line points at the place in the diff that makes it. The file is derived
from the diff, never maintained, and rewritten whenever the diff changes.

A delta is **organised by change, not by file**. One file covers one PR and
holds:

- the touched units' behaviour walks, every step included, with unchanged steps
  kept as context;
- the data shapes the change touches;
- the public signatures the walks link to or the change alters;
- a summary of its tests.

The examples in this file use one made-up PR to a Bevy survivor game: every
enemy killed pays a bounty into a purse, a wave cleared early pays a bonus, and
the shop spends from the purse.

> **v3 (2026-11-10)** replaces v2.x. The kinds `plugin`, `system`, `module`,
> `endpoint`, `workflow` and `job` are gone, along with the world sections
> (`## components`, `## resources`, `## events`, `## systems`, `## access`),
> the `# +`/`# -` comment annotations, `use` imports, `@ spec` refs and
> `@ <path>` type references. A v2 file does not parse as v3.
> `hobby-mml-parser --version` prints `3.x` for this grammar.

______________________________________________________________________

## 1. File shape

```
<header>          key: value lines

## intent         required, first
## data           optional
## unit <name> …  one or more, in the order a request meets them
## api            optional
## tests          optional
```

- Sections appear in this order, and `## unit` repeats once per unit. Any other
  `##` line is an error (`unknown-section`), and so is an out-of-order one
  (`section-order`).
- `#` at column 0 followed by a space or the end of the line is a comment and is
  ignored. `##` at column 0 is always a section header.
- Blank lines are ignored everywhere.
- The file is named `delta-<branch>.hmml`, with `/` in the branch replaced by
  `-`. It lives in the notes dir of the task the branch belongs to,
  `notes/{project}/{topic}/{task}/delta-<branch>.hmml`, or of the aspect,
  `notes/{project}/{topic}/{task}/aspects/{aspect}/delta-<branch>.hmml`.

## 2. Header

The header is the `key: value` lines before the first `##`. Each value is a
single token, except `pr`, which is a URL.

| Key      | Req | Value                                                                      |
| -------- | --- | -------------------------------------------------------------------------- |
| `repo`   | yes | the project, as named under `~/fun_repos`; the checkout is resolved from it (§8) |
| `branch` | yes | the PR's head branch                                                       |
| `base`   | yes | commit the delta is read against (merge base, or the last reviewed commit) |
| `head`   | yes | commit the delta describes, or `worktree` for the checkout as it stands     |
| `pr`     | no  | the PR's URL, once one exists                                              |

`base` and `head` are commit ids (7–40 hex digits), except that `head` may be
`worktree`: the change is not committed yet, and its head side is the checkout's
files on disk, tracked and untracked alike. That is the normal case for a delta
`/task:build` derives before GATE 2, and it keeps resolving after the commits
land, because the tree then equals `HEAD`. A missing required key is
`missing-header-key`, and an unknown key is `unknown-header-key`. Both are
errors.

## 3. The gutter

Every line in a section body has a **gutter**: column 0 is one of

| Gutter | Meaning                                                               |
| ------ | --------------------------------------------------------------------- |
| ` `    | context: the line holds at both `base` and `head`                     |
| `+`    | added: holds at `head`, not at `base`                                 |
| `-`    | removed: held at `base`, not at `head`                                |
| `~`    | changed: one line in both versions, written as `<old> → <new>` (§3.1) |

Column 1 is a space, and the **content** starts at column 2. Anything else at
column 0 is `bad-gutter`, and so is a non-space at column 1. Content indents in
steps of 2 spaces (`bad-indent` otherwise). The tree a section describes is
built from content indentation alone. The gutter takes no part in nesting.

**Nesting rule.** A removed step's body went with it, so a line nested under a
`-` line is `-` too. A new step's body is new, so a line nested under a `+` line
is `+` too. Breaking this rule is `nested-change-mismatch` (error). Lines under
`~` or context lines may carry any gutter.

**`## intent` takes no change gutter.** Every intent line is context (`  - …`).
Using `+`, `-` or `~` there is `change-in-intent` (error).

### 3.1 `~` lines

A `~` line holds exactly one `→` (U+2192, with a space either side). The text
before the arrow is the old wording and the text after it is the new one, each
cut down to the clause that differs. The words both versions share lead the line
and are written once:

```
~ - check the Score covers the price → the Purse covers the price
```

This reads as "check the Score covers the price" becoming "check the Purse
covers the price". When the shared words would only come after the change, rephrase
the step so they lead.

When the change only adds a clause, the new side starts with `… `. That means
"the whole old wording, then this":

```
~ - write WaveCleared → … with early set when the timer has not finished
```

When more than one clause changes, write a `-` line and a `+` line instead. A
`~` line with no arrow, or with more than one, is `bad-tilde` (error).

## 4. Line annotations

A content line may end with annotations. They come in this order, and each at
most once:

```
<text> [^anchor] [@ <file>[:<line>]] [@ api <M>::<fn>[, <M>::<fn>…]] [@ guards ^a, ^b]
```

- **`^anchor`** names the line so a test can say it guards it.
  - It is `[a-z][a-z0-9_]*` and unique in the file (`duplicate-anchor`).
  - It goes on unit steps and unit invariants only.
- **`@ <file>[:<line>]`** is the **diff link** (§8): where the diff makes this
  line true.
  - `<file>` is any path suffix that identifies one file (`waves.rs` or
    `waves/director.rs`).
  - The line counts on the `head` side for context, `+` and `~` lines, and on
    the `base` side for `-` lines.
  - Name the first line of the expression that decides the behaviour, not the
    first line of the enclosing function.
- **`@ api <Module>::<fn>`** (or `<Module>::<Type>::<fn>`) points at an `## api`
  entry of this file. `unknown-api-ref` (error) is reported when it names none.
  - A step whose work spans several functions lists them, separated by commas.
  - `@ api M::f` matches a `fn f` or a `fn M::f` in module `M`.
  - Unit steps only.
- **`@ guards ^a, ^b`** lists the anchors a test line guards. `unknown-anchor`
  (error) is reported when one is not defined. Test lines only.

An annotation out of order, repeated, or on a line kind that does not take it is
`bad-annotation` (error).

**`unlinked-change`** (warning) is reported for a `+`, `-` or `~` unit step,
`## api` `fn` entry or test line without a diff link.

- A `+` or `-` step also counts as linked when a line it sits under, or one
  nested under it, carries a link and has the same gutter. One link can cover a
  whole added or removed block.
- Data lines, invariants and module lines may stay unlinked.

## 5. `## intent`

One to three `- ` bullets saying what the PR sets out to do, for its consumer.
They read like the done-when, not like the diff.

```
## intent
  - every enemy killed pays its bounty into the purse, and a wave cleared before its timer pays a bonus
```

## 6. `## data`

The data shapes the change touches. A type is listed only when a unit step or an
`## api` signature of this delta names it, **and** one of these holds:

- its fields change;
- the change adds or removes it;
- the reader needs its shape to follow a changed step.

A type the PR adds that nothing in the delta names stays out.

```
## data
  component struct Enemy {
    health: f32,
-   score_value: u32,
+   bounty: u32,
  }
+ event struct EnemyDied { enemy: Entity, bounty: u32, at: Vec3 }
+ resource struct Purse { coins: u32, earned_this_wave: u32 }
+ enum Rarity { common, elite }
```

- `struct Name {` opens a block whose fields are `name: Type,` lines nested one
  step deeper. A `}` at the struct's own depth, with the struct's own gutter,
  closes it. In a one-line `struct Name { a: A, b: B }` the fields have no
  gutter of their own.

- Field names are written as the code writes them.

- `enum Name { a, b }` has unit variants. It is written on one line or as a
  block, like a struct.

- Types use the Rust-like subset:

  - primitives: `String bool i8..i64 u8..u64 f32 f64`;
  - atoms: `Uuid Money DateTime`, and Bevy's
    `Entity Vec2 Vec3 Quat Color Timer Duration Handle<T>`;
  - containers:
    `Option<T> Vec<T> Set<T> Map<K, V> Future<T> Result<T, E> Either<L, R> Tuple<A, B> Queue<T> Stream<T>`;
  - functions: `Fn<A, R>`, with `Tuple<…>` for several parameters;
  - any `PascalCase` name.

  A name the file does not define is fine, because the delta shows only what
  changed.

- A `struct`/`enum` defined twice on the same side is `duplicate-type` (error).
  A `-`/`+` pair of the same name is a change, not a duplicate. The same holds
  for modules and `fn` entries.

- Struct and enum names cross-link wherever they appear in prose, and in a unit
  header's `on <Event>` (§7.1).

### 6.1 ECS roles

A type line may start with the role the code registers it with:

| Role        | Bevy                                                      |
| ----------- | --------------------------------------------------------- |
| `component` | `#[derive(Component)]`                                    |
| `resource`  | `#[derive(Resource)]`                                     |
| `event`     | `#[derive(Event)]` / `EntityEvent`, observed with `On<E>` |
| `message`   | `#[derive(Message)]`, read with `MessageReader<E>`        |
| `state`     | `#[derive(States)]` / `SubStates`                         |

```
  component struct Enemy {
+ resource struct Purse { coins: u32 }
  state enum Screen { title, gameplay }
```

- The role is read off the derive. A type with none of these derives takes no
  role: a field type, an enum a component is built from.
- A unit struct (a marker such as `struct Boss;`) is written
  `component struct Boss {}`.
- A type whose role changes is a `-`/`+` pair.
- Nothing checks the role against the code or an access against the role; it
  is there so a changed ECS shape reads as one. Any other word before
  `struct`/`enum` is `bad-struct`.
- Bevy's own types (`Transform`, `Time`, `Children`, …) are never listed.

## 7. `## unit <name> [in <Schedule> | on <Event>] — <what it is> [@ <file>:<line>]`

There is one section per **unit**. A unit is an entry point whose behaviour
reaches a changed file: an endpoint, a queue consumer, a scheduled job, a tool
the agent calls, or a CLI command.

- In a Bevy app the entry points are what the `App` runs: a **system** in a
  schedule, an **observer**, and a `Plugin::build` when what it registers
  changes beyond the systems it adds (a resource inserted, a state
  initialised). A CLI tool or a non-Bevy crate keeps the classic ones: a
  command, a request handler, a queue consumer.
- Follow each call path from the change up to the first entry point. A
  collaborator reached through another unit gets a unit of its own when its
  consumer differs. A helper that two systems call is not a unit; it is the
  `## api` entry both link to.
- `<name>` is kebab-case, verb-first or the queue, tool or command name, and
  unique in the file (`duplicate-unit`).
- The prose after `—` says what the unit is. The optional diff link points at
  the entry point.
- A unit the PR adds writes `(new)` after its name:
  `## unit pay-bounty (new) on EnemyDied — …`. A unit it removes writes `(removed)`.
  Every line of a `(new)` unit is `+` and every line of a `(removed)` one is
  `-`; a line that breaks this is `nested-change-mismatch`.
- When one unit contains another (a message handler around the tools it calls),
  the outer one comes first.

The body is the unit's **whole behaviour walk as `head` leaves it**, with the
changed steps marked, then an optional `invariants:` block:

```
## unit spawn-wave in FixedUpdate — the wave director spawning enemies on the wave timer @ waves.rs:40
  - tick the wave timer
  - if the timer finished:
    - spawn X enemies around the player at the wave's ring radius
~   - give each Enemy a score value from the wave table → a bounty from the wave table ^bounty_from_table @ waves.rs:61 @ api WaveTable::bounty
  - if no Enemy is left alive:
~   - write WaveCleared → … with early set when the timer has not finished ^early_clear @ waves.rs:88
-   - add the wave's score bonus to the Score @ waves.rs:96
-     - flash the score text
+   - if the clear was early:
+     - credit the wave bonus to the Purse ^wave_bonus @ waves.rs:92 @ api Purse::credit, WaveTable::bonus
    - start the next wave's timer
  invariants:
  - at most N enemies are alive at once
  - a wave is cleared exactly once
+ - a wave bonus is paid only for a clear before the timer ran out ^bonus_only_early
```

- **Steps** are `- ` bullets at mental-model altitude. Control flow uses only
  the closed keyword set, each keyword ending in `:` with its body nested
  beneath it: `if <cond>:` · `else:` · `for each <x>[ in <y>]:` ·
  `while <cond>:` · `try:` · `on error[ <e>]:`. Everything else is prose.
- **Context is kept.** Unchanged steps stay in the walk so that the changed ones
  read in place. A unit the PR removes is written as it was at `base`, every
  line `-`. Every line of a new unit is `+`.
- **A step that only moved is context**, written at its `head` position. A move
  that changes what happens (a step now outside a `try:`, or now before a check)
  is marked by what changed: `~` on the step, or `+`/`-` on the control-flow
  line that moved.
- **Data files steer behaviour too.** A changed asset or config the code reads
  (a wave table in RON, a shader constant) is a step nested under the step that
  reads it, linked to the file's line.
- Literal `X`, `Y`, `N` stand for quantities whose value is not the point.
- **`invariants:`** at content depth 0 opens the unit's invariants: flat `- `
  bullets, each a checkable claim. Unchanged ones stay as context.
  - They come after the last step.
  - `invariants:` itself takes the gutter of its first bullet, or a space.
  - An invariant is marked by whether it holds at `base` and at `head`, even
    when the code that makes it true is untouched. For example, a memory that
    now holds a placeholder because the model writes one is marked `+`.
  - An invariant that holds at both sides is context, whatever enforces it now.
    That holds even when a check was removed because the new design makes the
    claim true by construction.
  - An invariant the PR adds or changes about **scope** (which entities a query
    can reach), **ordering** (what runs before what, within a frame or across
    states), **failure** (what an error leaves behind) or **persistence** (what
    outlives a state or a despawn, and where it is kept) is always written.
    Lifting a walk never folds such an invariant into a step.
- A step whose work is done by public functions listed in `## api` ends with
  `@ api Module::fn` after its diff link.

### 7.1 What runs a unit

A Bevy unit names what runs it, between the name (or its `(new)`/`(removed)`)
and the ` — `:

- `in <Schedule>` for a system: `in Update`, `in FixedUpdate`,
  `in OnEnter(Screen::Gameplay)`. The schedule is the `add_systems` argument,
  written as the code writes it: a PascalCase label, optionally with one
  parenthesised `Type::Variant` argument, no spaces.
- `on <Event>` for an observer: `on EnemyDied`, `on Add<Enemy>`. The event is
  the observer's `On<…>` type, one token. When it is a `## data` type, it
  cross-links there.

```
## unit pay-bounty (new) on EnemyDied — the observer paying an enemy's bounty when it dies @ bounty.rs:12
```

- The trigger is as `head` leaves it (`base` for a `(removed)` unit). A change
  to when a unit runs — its schedule, a run condition, an ordering constraint
  such as `.after(..)` or `.chain()` — is a step at the top of the walk, linked
  to the registration line: `~ - runs in Update → FixedUpdate @ plugin.rs:22`.
- A unit that is not a system or an observer names no trigger.
- A malformed trigger, or a word other than `in`/`on` in that place, is
  `bad-unit-header`.

## 8. Diff links and the checkout

A diff link is resolved against the **checkout** of `repo`. The first of these
that exists and contains the `head` commit (the `base` commit, for
`head: worktree`) is used:

1. `~/fun_aspects/{project}/{topic}/{task}/{aspect}`, where the delta sits in
   `notes/{project}/{topic}/{task}/aspects/{aspect}/`
2. `~/fun_worktrees/{project}/{topic}/{task}`, where the delta sits in that
   task's notes dir or one of its aspects'
3. `~/fun_repos/{repo}`

The first two are tried only when `repo` names the delta's `{project}`: a task
is one repo, so the task directory is the checkout.

`<file>` resolves first among the files `git diff --name-only base...head`
lists, then among all files at that side's commit. For `head: worktree` the
head side is the working tree: the diff is `git diff --name-only base` plus the
untracked files, and the files are those on disk. It must match exactly one
path, comparing whole path components from the end. `<line>` must lie within the
file at that side (`head` for context/`+`/`~`, `base` for `-`).

These checks run under `--lint` and in the editor, and report:

- `unresolved-diff-link` (warning): no file, more than one file, or a line out
  of range. The message names the candidates.
- `checkout-not-found` (warning, once per file): no checkout holds `head`
  (`base`, for `head: worktree`).
- `stale-delta` (info): the checkout's tip of `branch` is past `head`. Never
  for `head: worktree`.

## 9. `## api`

The public signatures the delta links to or changes, **signatures only**. Each
module line sits at content depth 0 and says what the module is for. Its `fn`
entries sit one step deeper:

```
## api
  WaveTable — the per-wave numbers the director reads @ wave_table.rs
-   fn score_value(wave: u32) -> u32 @ wave_table.rs:20
+   fn bounty(wave: u32) -> u32 @ wave_table.rs:20
+   fn bonus(wave: u32) -> u32 @ wave_table.rs:31
+ Purse — the coins the player holds this run @ purse.rs
+   fn credit(coins: u32) @ purse.rs:14
+   fn Purse::spend(cost: u32) -> Result<u32, NotEnough> @ purse.rs:22
```

- **Module line:** `PascalName — responsibility [@ <file>]`. Names are unique in
  the section (`duplicate-module`).

- **`fn` entry:** `fn [Type::]name(param: Type, …) [-> Type] [@ <file>:<line>]`.

  - Signatures stay faithful to the code (`Result<u32, NotEnough>`, not
    normalised away). A method's receiver (`&self`, `&mut self`) is left out.
  - `fn` entries carry no body.
  - Two entries with the same name on the same side of one module are
    `duplicate-fn` (error).
  - A `fn` line outside a module, or nested deeper than one step, is `bad-fn`.

- **What a module is.** A module is the unit of code that owns the functions,
  named as the code names it:

  - Rust: a module, or a type with an `impl`. A resource's methods are its
    module, named after the resource.
  - A trait defined in another type's file is its own module, with its own
    module line.

  **A system or an observer is never an `## api` entry**: the schedule calls
  it, not a caller, so it is a unit (§7). Neither is a `Plugin` impl.

- **Which `fn` entries are listed.** An entry is listed when:

  - a unit step links to it, or
  - the PR removes or changes a public function that exists at `base`.

  A function the PR adds that no step links to stays out. A module the PR adds
  still gets its module line, so the reader knows it exists. A changed signature
  is a `-`/`+` pair. It is a `~` only when the change is the signature's tail,
  such as the return type (`fn spend(cost: u32) -> bool → Result<u32, NotEnough>`), so
  that the parts before it read once. A parameter added, removed or retyped
  mid-signature is always a pair. A function that turns public counts as added
  (`+`). One that stops being public counts as removed (`-`).

- **What stays out:** private helpers, constructors (`new`, `Default`), impls
  of Bevy's own traits (`Plugin`, `FromWorld`), and overrides of a trait method
  already listed under the trait.

## 10. `## tests`

The PR's test changes, **summarised**. Every line is a flat entry (content depth
0, no body):

```
## tests
+ tests 4 — bounty per wave, the early flag, bonus only when early, one clear per wave @ waves_test.rs:12 @ guards ^bounty_from_table, ^early_clear, ^bonus_only_early
+ test bonus_paid_on_early_clear — clearing before the timer credits the wave bonus @ waves_test.rs:80 @ guards ^wave_bonus
- tests 3 — score spending in the shop; the score is no longer spent @ shop_test.rs:30
~ test shop_buys_upgrade — with score → with coins @ shop_test.rs:60
```

- `test <name> — <description>` is one test. `<name>` is the test's name in
  snake_case, shortened to the words that identify it.
- `tests <N> — <description>` is a group of N tests that change together for one
  reason. N counts test functions as they appear in the source, so a case table
  one test function runs over counts once. A group's diff link points at its
  first test. When the tests of one reason sit in several files, write one line
  per file, so that each file is one jump away.
- Group whenever several tests share a reason. Give a test its own `test` line
  when it is the only guard of a changed step, or when its change needs saying
  on its own.
- `@ guards` names the anchored lines the tests exercise. A test that guards no
  line of the delta has no `@ guards`.
- Unchanged tests are not listed.
- Any other entry shape is `bad-test` (error).

## 11. Checks at a glance

| Code                     | Severity | Where                                          |
| ------------------------ | -------- | ---------------------------------------------- |
| `missing-header-key`     | error    | header (§2)                                    |
| `unknown-header-key`     | error    | header (§2)                                    |
| `bad-header-value`       | error    | `base` not a commit id, `head` neither one nor `worktree` (§2) |
| `unknown-section`        | error    | (§1)                                           |
| `section-order`          | error    | (§1)                                           |
| `missing-intent`         | error    | no `## intent`, or an empty one (§5)           |
| `no-unit`                | error    | no `## unit` section (§7)                      |
| `bad-gutter`             | error    | (§3)                                           |
| `bad-indent`             | error    | (§3)                                           |
| `nested-change-mismatch` | error    | (§3, §7)                                       |
| `change-in-intent`       | error    | (§3)                                           |
| `bad-tilde`              | error    | (§3.1)                                         |
| `bad-annotation`         | error    | (§4)                                           |
| `duplicate-anchor`       | error    | (§4)                                           |
| `unknown-api-ref`        | error    | (§4)                                           |
| `unknown-anchor`         | error    | (§4)                                           |
| `bad-struct`             | error    | malformed struct/enum block, unknown role (§6) |
| `duplicate-type`         | error    | (§6)                                           |
| `duplicate-unit`         | error    | (§7)                                           |
| `bad-unit-header`        | error    | no name or `—`, a bad trigger (§7, §7.1)       |
| `misplaced-invariants`   | error    | a step after `invariants:`, or two blocks (§7) |
| `bad-fn`                 | error    | (§9)                                           |
| `duplicate-module`       | error    | (§9)                                           |
| `duplicate-fn`           | error    | (§9)                                           |
| `bad-test`               | error    | (§10)                                          |
| `no-change`              | warning  | no `+`, `-` or `~` line anywhere               |
| `unlinked-change`        | warning  | (§4)                                           |
| `unresolved-diff-link`   | warning  | `--lint` / editor (§8)                         |
| `checkout-not-found`     | warning  | `--lint` / editor (§8)                         |
| `stale-delta`            | info     | `--lint` / editor (§8)                         |

`hobby-mml-parser --lint` exits non-zero on any error or warning. Info never gates.

## 12. A whole delta

```hmml
repo:   bevy_survivor
branch: wave-bounties
base:   3a9c41e07b
head:   c58d20f9a1
pr:     https://github.com/example/bevy_survivor/pull/42

## intent
  - every enemy killed pays its bounty into the purse, and a wave cleared before its timer pays a bonus
  - the shop spends from the purse instead of from the score

## data
  component struct Enemy {
    health: f32,
-   score_value: u32,
+   bounty: u32,
    speed: f32,
  }
+ event struct EnemyDied { enemy: Entity, bounty: u32, at: Vec3 }
+ resource struct Purse { coins: u32, earned_this_wave: u32 }
  message struct WaveCleared {
    wave: u32,
+   early: bool,
  }
+ state enum Shop { closed, browsing }

## unit spawn-wave in FixedUpdate — the wave director spawning enemies on the wave timer @ waves.rs:40
  - tick the wave timer
  - if the timer finished:
    - spawn X enemies around the player at the wave's ring radius
~   - give each Enemy a score value from the wave table → a bounty from the wave table ^bounty_from_table @ waves.rs:61 @ api WaveTable::bounty
  - if no Enemy is left alive:
~   - write WaveCleared → … with early set when the timer has not finished ^early_clear @ waves.rs:88
-   - add the wave's score bonus to the Score @ waves.rs:96
-     - flash the score text
+   - if the clear was early:
+     - credit the wave bonus to the Purse ^wave_bonus @ waves.rs:92 @ api Purse::credit, WaveTable::bonus
    - start the next wave's timer
  invariants:
  - at most N enemies are alive at once
  - a wave is cleared exactly once
+ - a wave bonus is paid only for a clear before the timer ran out ^bonus_only_early

## unit pay-bounty (new) on EnemyDied — the observer paying an enemy's bounty when it dies @ bounty.rs:12
+ - credit the bounty to the Purse and add it to earned_this_wave @ bounty.rs:18 @ api Purse::credit
+ - spawn a coin burst where it died ^coin_burst @ bounty.rs:24
+ invariants:
+ - a dead enemy is paid for exactly once ^paid_once

## unit buy-upgrade in Update — the shop screen's buy button @ shop.rs:70
  - for each pressed buy button:
~   - check the Score covers the price → the Purse covers the price ^can_afford @ shop.rs:78 @ api Purse::spend
    - if it does not:
      - shake the button and play the denied sound
    - else:
      - apply the upgrade to the player
  - refresh the button labels

## api
  WaveTable — the per-wave numbers the director reads @ wave_table.rs
-   fn score_value(wave: u32) -> u32 @ wave_table.rs:20
+   fn bounty(wave: u32) -> u32 @ wave_table.rs:20
+   fn bonus(wave: u32) -> u32 @ wave_table.rs:31
+ Purse — the coins the player holds this run @ purse.rs
+   fn credit(coins: u32) @ purse.rs:14
+   fn Purse::spend(cost: u32) -> Result<u32, NotEnough> @ purse.rs:22
  Score — the run's score display @ score.rs
-   fn spend(cost: u32) -> bool @ score.rs:40

## tests
+ tests 4 — bounty per wave, the early flag, bonus only when early, one clear per wave @ waves_test.rs:12 @ guards ^bounty_from_table, ^early_clear, ^bonus_only_early
+ test bonus_paid_on_early_clear — clearing before the timer credits the wave bonus @ waves_test.rs:80 @ guards ^wave_bonus
+ tests 2 — a dead enemy credits its bounty once, even when two hits land in one frame @ bounty_test.rs:8 @ guards ^paid_once
+ test purse_refuses_overspend — a price above the coins leaves the purse untouched @ purse_test.rs:10 @ guards ^can_afford
- tests 3 — score spending in the shop; the score is no longer spent @ shop_test.rs:30
~ test shop_buys_upgrade — with score → with coins @ shop_test.rs:60
```
