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

## Commands

```
hresearch new "<question>"          create the sources dir, print its path
hresearch fetch <dir> <url>         fetch and store a source, print its id
hresearch render <dir> - <<JSON     validate card.json and write the card
hresearch check <card.md> [--live] [--require <slot>]...
```

`check` also verifies the `.md` is byte-identical to a re-render of its
`card.json` — a hand edit to the file fails it.

## Install

`just install-chobby` links `hresearch` onto `PATH`; `just install-skills`
links `/research`.
