# hobby-struct-lsp

| Field        | Value                                                    |
| ------------ | -------------------------------------------------------- |
| Type         | language server + checker CLI (Rust)                      |
| Owner        | @julianbieber                                             |
| Dependencies | `tower-lsp`, `tree-sitter-rust`                           |
| Status       | v1 — three repos adopted, all at budget                   |

**`grammar.md` in this directory is the source of truth for HSTRUCT.**

An `.hstruct` file states the **code quality standards** one repo holds itself
to — rules that keep it maintainable and performant, why each exists, and how far
the code currently is from each. One file per repo (or per crate in a workspace),
named `structure.hstruct` at that root.

## The standard is durable; the instances are not

A standard is as true on a project's last day as its first. The files that
violate it are renamed, split, and edited constantly. So the file stores **the
standard and a number**, never the offenders:

```
## A file fits in one head

A file that cannot be read in one sitting gets edited by guessing, and the
guesses accumulate.

check: files over 900 lines
budget: 10
```

Eleven is an error. Nine is a nudge to tighten. Renaming a file, splitting one,
or adding two hundred lines to one already over changes nothing here.

**Nothing in an `.hstruct` file names a module, a file, or a type.** A rule that
needs an enumeration to be expressed is the wrong rule for this format. The
offenders are still available — `--detail` prints them — they are just not
*stored*, because storage is where the churn was.

### The design this replaced

The first draft stored the instances: a record per module with its path, layer
membership lists, and a waiver per violation including `? gameplay/growth.rs 2758 lines`.
Editing `growth.rs` at all — even to improve it — made that waiver stale *and*
raised a new violation. The file punished touching the code, and 236 of its lines
were largely a staler copy of the directory listing.

## The ratchet is the improvement loop

`--ratchet` rewrites every budget the code has moved below, in place. **Only
downward** — a budget that rose would record a regression as the new normal.

That is what lets a repo adopt this on its worst day: write the standards with
`budget: 999`, ratchet once, and the file records exactly how far the code is
from each rule. Every later refactor that improves a number is one command from
being recorded.

## Rough metrics are a feature, not a compromise

`unfiltered queries` and `allocating systems` are heuristics. Some unfiltered
queries are correct; the allocation check matches names rather than types.

Under a per-instance waiver scheme each would have to be adjudicated and
re-adjudicated on every edit — unusable, and the reason the first draft had no
performance metric at all. Against a budget the count only has to be **monotone**: getting
worse is caught and the noise is constant. A budget is what makes an imperfect
signal usable, and that is what opened the performance half of the format.

## Not everything is countable, and the file says so

A standard declares how it is verified — `check:` + `budget:`, or `review:` +
`audited:`. A `review:` standard is honest about not being mechanical, which is
the right home for the rules that matter most and that no parser can settle:
*"every spawn in an `OnEnter` carries a `DespawnOnExit`"* is the most
load-bearing rule in `wusel` and is not statically decidable.

The first draft exiled those to prose, on the reasoning that the file must only
contain what a checker can verify — which is exactly why it drifted toward
whatever was easy to count. The honest requirement is only that a rule declares its mode.

## Rust is parsed, and paths are resolved

`tree-sitter-rust`, not text matching, and an import resolves by **path** rather
than by its last segment. Text matching read `use crate::gameplay::{city,
growth};` as one import of a module called `gameplay`; leaf matching then blamed
`use crate::terrain::brush::Brush` on an unrelated top-level `brush.rs` and read
`use bevy::app::…` as a local module named `app`.

## Usage

```
hobby-struct-parser <repo>            report every standard against its budget
hobby-struct-parser --detail <repo>   also list what each count is made of
hobby-struct-parser --ratchet <repo>  tighten budgets the code has moved below
hobby-struct-parser --lint <repo>     exit non-zero on warnings too
hobby-struct-parser --json <repo>     diagnostics as JSON
just structure                        every repo or crate under ~/fun_repos
just structure wusel                  one repo
```

## Layout

```
grammar.md          the language definition — read this first
struct-parser/      lexer, parser, AST, the metrics, and the CLI
struct-lsp/         tower-lsp glue; diagnostics, hover, completion
```

The LSP publishes the same diagnostics the CLI prints, from the same parse and
check, so the editor and `just structure` cannot disagree. Syntax highlighting
comes from `lsps/tree-sitter-hstruct`, never from here.
