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

## The card screen

A markdown file, `research/<date>-<slug>.md`, one per question:

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
research/<stem>.md                         the card, generated — never hand-edited
research/sources/<stem>/
  question.txt                             the question, verbatim
  sources.json                             [{id, url, fetched}]
  <id>.txt                                 the stored, fetched text of that source
  card.json                                the slots — the card's actual input
```

`<stem>` is `<date>-<slug>`, deduplicated with a `-2`, `-3`, … suffix.

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
the card otherwise. `hresearch check --live <card.md>` re-fetches those
sources fresh and re-runs the same check, which is how a card is audited after
the fact rather than only trusted at write time.

## The option-matrix screen

A question of the form "which techniques exist to …" is answered as a
**matrix** instead: `research/<stem>.md`, one row per criterion instead of
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

## Commands

```
hresearch new "<question>"          create the sources dir, print its path
hresearch fetch <dir> <url>         fetch and store a source, print its id
hresearch render <dir> - <<JSON     validate card.json/matrix and write the file
hresearch check <card.md> [--live] [--require <slot>]...
```

`check` prints `shape: card` or `shape: matrix` first, then shape-specific
lines (a card's `slots:`/`claims:`, or a matrix's `criteria:`/`options:`/
`verdict:`/one `precedent:` line per cited link). `--require` only applies to
a card's slots — on a matrix it fails with `--require: not applicable to a
matrix`. `check` also verifies the `.md` is byte-identical to a re-render of
its `card.json` — a hand edit to the file fails it, for either shape.

## Install

`just install-chobby` links `hresearch` onto `PATH`; `just install-skills`
links `/research`.
