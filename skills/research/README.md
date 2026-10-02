# Research

| Field        | Value                                    |
| ------------ | ----------------------------------------- |
| Type         | skill + CLI tool (`bin/hresearch`)        |
| Install path | `~/.claude-hobby/commands/research.md`, `hresearch` on `PATH` |
| Owner        | @julianbieber                             |
| Dependencies | python3 (stdlib only)                     |
| Status       | experimental                              |

## Why

During a game jam, code and asset generation with an LLM is prohibited but
research on the side is fine. Reading LLM-generated prose as a research result
isn't wanted either, so a question gets answered as a **card**: a fixed set of
slots, each either filled with something checkable or explicitly left empty —
never a paragraph.

## Picking the shape

`hresearch new "<question>"` classifies the question and records the result
in `shape.txt`, deterministically, on the lowercased question with whitespace
collapsed, rules tried in this order:

1. **matrix** — a survey word follows `which`/`what`
   (`which techniques exist to …`, `what approaches are there for …`), or the
   question contains `ways to`, `alternatives to` or `approaches to`.
2. **card** — a how-to about the asker's own use: `how do/can/should/would
   i/you/we …`, or `how to …`.
3. **figure** — asks how a thing works: starts with `how does/do/is/are …` or
   `how a/an/the …`, starts with `explain how`, or contains `how … works`.
4. otherwise **card**.

| question | shape |
|---|---|
| how do I set up colliders with avian? | card |
| how to set up colliders with avian | card |
| what does Collider::trimesh_from_mesh do | card |
| which techniques exist to simulate water flow | matrix |
| what approaches are there for procedural terrain | matrix |
| how does a flow field pathfinder work | figure |
| how does a flow field pathfinder move many agents | figure |
| how a flow-field pathfinder moves 200 agents | figure |

`new --shape card|matrix|figure` overrides the classifier — `new` records the
flag's shape and reports it as `shape: <x> (override)` instead of
`(from question)`. Once recorded, `render` refuses a `card.json` in any other
shape (`render: shape: the question called for <x>, got <y>`), and `check`
reports the same mismatch as a failure — a dir made before this existed, with
no `shape.txt`, is unconstrained and renders in whatever shape its `card.json`
names.

## The card screen

A `.hresearch` file, `research/<date>-<slug>.hresearch`, one per question —
Helix highlights it through `lsps/tree-sitter-hresearch`, the snippet as Rust,
and `gd` on a link, or on a `[k]` ref, opens it in the browser
(`lsps/hobby-research-lsp`):

```
┌ <question>                                              <date> ┐
│ verdict   <the answer, 1-2 items>                                │
│ how       1. <step>  2. <step>  …                                │
│ snippet   <code copied verbatim from a fetched source>           │
│           ↳ <link>#L<a>-L<b> [k]                                 │
│ breaks    "<a caveat, the source's own words>"                   │
│           — <url> [k]                                            │
│ source    [k] <url>                                              │
└──────────────────────────────────────────────────────────────────┘
```

Every claim traces to a link (`[k]`) or is shown `[unsourced]`. A slot with
nothing honest to fill renders `empty: <reason>`. No prose anywhere in it.

## File layout

```
research/<stem>.hresearch                  the card, generated — never hand-edited
research/sources/<stem>/
  question.txt                             the question, verbatim
  shape.txt                                the shape `new` picked for this question
  sources.json                             [{id, url, fetched}]
  <id>.txt                                 the stored, fetched text of that source
  card.json                                the slots — the card's actual input
```

`<stem>` is `<date>-<slug>`, deduplicated with a `-2`, `-3`, … suffix.

## The index

`hresearch search` reads `~/.cache/hresearch/index.sqlite` (under
`$XDG_CACHE_HOME` when set), a SQLite FTS5 index over every project's
`notes/*/*/*/research/sources/*/`. It is derived and never committed: every
search first re-indexes any card whose `question.txt`, `card.json`,
`sources.json` or `.hresearch` changed, and drops cards that are gone, so there
is no rebuild command. Deleting the file is always safe. No model is in the
query path.

A sources dir counts as a card only once it has both `card.json` and its
rendered `.hresearch`; an unanswered question is not indexed.

Indexed per card: the question, every string in `card.json` (verdict, steps,
snippet, quotes, matrix cells, figure lines, caption), Rust identifiers from
docs.rs item URLs (`struct.TilemapChunk.html`) and CamelCase names in that text,
every URL in `sources.json`, and project/topic/task. Terms match as prefixes
and are ANDed: `tile` finds `tilemap`.

**crate@version** comes from the cited URLs:

- `docs.rs/<crate>/<ver>/…` → `<crate>@<ver>`; `latest` or `*` → `unpinned`
- `github.com/<owner>/<repo>/blob/<ref>/<path>` → the crate is `<name>` when the
  path starts `crates/<name>/`, else `<repo>`; the version is `<ref>` without a
  leading `v` when it is a version number, else (`main`, a branch) `unpinned`
- any other host names no crate

Names compare with `-` folded to `_`. The card's crate is the one most of its
URLs name (ties to the earliest), at its most-cited pinned version, or
`unpinned` when none is pinned. A card naming no crate prints `-`.

**The tag** compares that against the `Cargo.lock` of the worktree search was
run in — the nearest one walking up from the current directory to the worktree
root, or else the only `*/Cargo.lock` one level below the root:

| tag | when |
|---|---|
| `current` | the lock holds the crate at the card's version |
| `other-version` | the lock holds the crate at a different version |
| `unpinned` | the card's version is `unpinned` |
| `-` | outside a worktree, no lock found, or the crate is not in it |

**`space S`** in Helix, from any `.rs` or `.md` buffer, lists the same cards:
`hobby-research-lsp` answers `workspace/symbol` by running `hresearch symbols
<query>` from the workspace root, which uses the index, filters and tag above and
prints a JSON array. Each card yields its question, the identifiers it cites and
its `crate@version`; the picker's container column reads
`<project>/<task> <crate>@<version> <tag>`, and Enter opens the card at the line
naming the identifier. `just install-research && just wire-helix` attaches it.

**stale** lists every card that pins a crate below the newest version of that
crate in a `Cargo.lock`, one line per card and crate:
`<date> <shape> <crate>@<pinned> -> <locked> <question>  <path>`. It reads the
worktree's lock, or the file named with `--lock <path>` from anywhere, and
exits 2 when it has neither. It is a report and never deletes or edits a card.
It exits 0 when it listed something and 1 when it did not.

**retract** `<card.hresearch> "<reason>"` hides a wrong card from `search`,
`space S` and `stale`, and keeps the reason in
`research/sources/<stem>/retracted.json` next to the card, so it travels with
the notes repo. Deleting that file un-retracts the card. A missing card exits 1
and an empty reason exits 2.

## The source store

Pinned sources are kept in `$XDG_CACHE_HOME/hresearch/sources/` (default
`~/.cache/hresearch/sources/`), one file per URL, shared by every task. A URL
is pinned when it names a fixed version: a full `x.y.z` on docs.rs, or a
version-shaped tag (`v1.2.3`) on a GitHub `blob`. `latest`, `main`, a branch
and a docs.rs `0.19` (a semver range) are never stored and never served, so
they are fetched every time. `hresearch fetch` serves a stored URL without a
request and prints `(cached)`; the text is the same one `render` checks quotes
against.

`hresearch new --seed <card>` copies the pinned sources of a near-miss card
into the store and lists the rest on stderr as `fetch again`. The new dir's
`sources.json` starts empty: the run fetches what it cites. Deleting the store
is always safe.

## Card JSON

```json
{ "verdict": [ {"text": "...", "src": 1} ],
  "how":     [ {"text": "...", "src": 2} ],
  "snippet": {"code": "...", "src": 2},
  "breaks":  [ {"quote": "...", "src": 1} ] }
```

Any slot may instead be `{"empty": "<reason>"}`. `src` is optional on
`verdict`/`how` items (a missing one renders `[unsourced]`), and **required**
on `snippet` and every `breaks` item.

## The verbatim rule

A `snippet`'s code and a `breaks` quote must appear, whitespace-normalised, in
the stored text of the source they cite — `hresearch render` refuses to write
the card otherwise. `hresearch check --live <card.hresearch>` re-fetches those
sources fresh and re-runs the same check, which is how a card is audited after
the fact rather than only trusted at write time.

## The option-matrix screen

A question of the form "which techniques exist to …" is answered as a
**matrix** instead: `research/<stem>.hresearch`, one row per criterion instead of
slots of prose:

```
question: which techniques exist to simulate water flow

                | heightfield pipes | shallow water eq | SPH particles |
looks like      | rivers, pools     | waves, splash    | droplets      |
cost per frame  | O(cells)          | O(cells), stiff  | O(n log n)    |
jam-sized       | yes               | yes, careful dt  | small counts  |
bevy precedent  | [1]               | [2]              | none found    |
verdict         | start here        |                  |               |

[1] <url>
[2] <url>
```

**Criteria (first set)**, fixed by the tool rather than chosen per question —
so "one row per criterion" is checkable against a known set:

- **looks like** — what the technique produces or feels like in play.
- **cost per frame** — the shape of its runtime cost.
- **jam-sized** — whether it fits a jam's time and scope.
- **bevy precedent** — a link to a Bevy example/crate/repo, or `none found`.

## Matrix JSON

```json
{ "shape": "matrix",
  "options": [
    { "name": "heightfield pipes",
      "cells": {
        "looks like": {"text": "rivers, pools", "src": 1},
        "cost per frame": {"text": "O(cells)"},
        "jam-sized": {"text": "yes"},
        "bevy precedent": {"src": 3}
      } },
    { "name": "cellular automaton",
      "cells": {
        "looks like": {"text": "falling sand"},
        "cost per frame": {"text": "O(cells)"},
        "jam-sized": {"text": "yes"},
        "bevy precedent": {"none": true}
      } }
  ],
  "verdict": {"option": "heightfield pipes", "text": "start here"} }
```

Rules `render` enforces:

- 2–5 options, unique `name`s, each ≤ 32 chars.
- Every option's `cells` has exactly the four criteria, no extras.
- A non-precedent cell's `text` is a single line, ≤ 32 chars and ≤ 4 words
  (a cited `[k]` suffix doesn't count as a word); `src` is optional and must
  be in `sources.json`.
- A `bevy precedent` cell is exactly `{"src": k}` (k in `sources.json`) or
  `{"none": true}` — never a `text` cell.
- `verdict.option` must name exactly one option; `verdict.text` is a
  single line of ≤ 4 words.

## The mechanism-figure screen

A question of the form "how does X work" is answered as a **figure** instead:
`research/<stem>.hresearch`, an ASCII drawing of the mechanism with one sourced
caption line underneath — no slots, no prose:

```
question: how does a flow field pathfinder move many agents

    goal ●          cost field            flow field
    ┌───────┐      ┌───────┐             ┌───────┐
    │ . . ● │      │ 4 3 0 │             │ → → ● │
    │ . █ . │ ───► │ 5 █ 1 │ ────────►   │ ↑ █ ↑ │
    │ . . . │      │ 6 5 2 │             │ → → ↑ │
    └───────┘      └───────┘             └───────┘
    one BFS from the goal per frame; every agent reads one arrow [1]

[1] <url>
```

## Figure JSON

```json
{ "shape": "figure",
  "figure": "line one\nline two\n...",
  "caption": {"text": "one BFS from the goal per frame; every agent reads one arrow", "src": 1} }
```

`figure` may be the multi-line string shown above, or an equivalent JSON list
of line strings — both render identically. `caption` is always exactly
`{text, src}`; unlike a card's slots, its `src` is never optional, because the
line under the figure is where the figure's source has to live.

Rules `render` enforces:

- 3–30 lines, none over 78 columns, no tabs.
- At least one drawing character somewhere in the figure — box-drawing,
  arrows, block elements, geometric shapes, or one of `|+/\<>^=#` — or it's
  prose with line breaks, not a figure.
- Every run of words inside a line, split on 2+ spaces or on a drawing
  character, is at most 5 words — a label, never a sentence.
- `caption.text` is a single line, ≤ 120 chars and ≤ 16 words; `caption.src`
  is required and must be in `sources.json`.

## Commands

```
hresearch new "<question>" [--shape card|matrix|figure] [--seed <card>]
                                     create the sources dir, print its path
hresearch fetch <dir> <url>         fetch and store a source, print its id (`(cached)` when served from the store)
hresearch render <dir> - <<JSON     validate card.json/matrix and write the file
hresearch check <card.hresearch> [--live] [--require <slot>]...
hresearch list                      one line per question in this task's research/
hresearch search [<term>...] [--crate <name>[@<ver>]] [--shape card|matrix|figure] [--project <p>]
                                     search every project's cards, from any directory
hresearch stale [--lock <Cargo.lock>]
                                     cards pinned below the lock, a report only
hresearch retract <card.hresearch> "<reason>"
                                     hide a wrong card from search, space S and stale
```

`check` prints `shape: card`, `shape: matrix` or `shape: figure` first, then
shape-specific lines: a card's `slots:`/`claims:`, a matrix's
`criteria:`/`options:`/`verdict:`/one `precedent:` line per cited link, or a
figure's `figure: <n> lines, <w> cols` and `caption: [k] <url>`. `--require`
only applies to a card's slots — on a matrix or a figure it fails with
`--require: not applicable to a <shape>`. `check` also verifies the `.hresearch` is
byte-identical to a re-render of its `card.json` — a hand edit to the file
fails it, for any shape.

`list` prints `<stem>\t<shape>\t<file>\t<question>` for every question under
the current task's `research/sources/`, `<file>` being the rendered `.hresearch`'s
path relative to the notes dir, or `-` before it's rendered.

`search` prints `<date> <shape> <crate@ver> <tag> <question>  <path>` per hit,
best match first (newest first with no terms), `<path>` being the absolute
`.hresearch` path after two spaces. It exits 1 with nothing on stdout when
nothing matches. See [The index](#the-index).

`fetch` and `render` both refuse a `dir` that isn't one of the current task's
own sources dirs — a dir belonging to another task, or one `new` never
created — exiting 2 rather than reading or writing into it. That, plus `new`
always resolving the task from the current directory, is what keeps a
session's answers out of another task's `research/`.

## Install

`just install-chobby` links `hresearch` onto `PATH`; `just install-skills`
links `/research`.
