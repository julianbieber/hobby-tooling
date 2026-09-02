# HSTRUCT — Hobby Code Quality Standards

**This file is the source of truth for the HSTRUCT grammar.** The
`hobby-struct-parser` / `hobby-struct-lsp` binaries implement exactly what is
defined here.

An `.hstruct` file states the **code quality standards** one repo holds itself
to: rules that keep it maintainable and performant, why each exists, and how far
the code currently is from each. One file per repo — or per crate in a workspace
— named `structure.hstruct` at that root.

## 1. What the format is for, and what it refuses to hold

A standard is durable. *"State has one owner"* is as true on the last day of a
project as the first. The **instances** that violate it are not durable at all:
a file is renamed, a module is split, a function moves, and a list of offenders
is wrong again.

So the file stores **the standard and a number**, never the offenders.

```
## A file fits in one head

A file that cannot be read in one sitting gets edited by guessing, and the
guesses accumulate.

check: files over 900 lines
budget: 10
```

Ten files exceed the limit today. Eleven is an error. Nine is a nudge to tighten.
Renaming one, splitting one, or adding two hundred lines to one that was already
over changes nothing here.

**This is the correction the format is built around.** The obvious design stores
the instances, and the first draft of this one did: a record per module with its
path, a layer membership list, and a waiver per violation including `? gameplay/growth.rs 2758 lines`. Editing `growth.rs` at all, even to
improve it, made that waiver stale *and* raised a new violation. The file
punished touching the code, and its 236 lines were mostly a second, staler copy
of the directory listing.

Two consequences worth stating outright:

- **Nothing in an `.hstruct` file names a module, a file, or a type.** If a rule
  needs an enumeration to be expressed, it is the wrong rule for this format.
- **The offenders are still available.** `--detail` prints them. They are simply
  not *stored*, because storage is where the churn was.

## 2. File shape

Line-oriented, UTF-8, two parts in a fixed order:

1. A **header** (`key: value`) block.
2. A sequence of **standards**, each opened by a `##` heading.

| Key | Required | Meaning |
|---|---|---|
| `repo` | yes | The repo's directory name. Identifies the file; not a path. |
| `root` | yes | Source root the metrics walk, relative to the file. Usually `src`. |
| `crate` | no | The cargo package. **Verified** against the sibling `Cargo.toml`. |

Rules everywhere: blank lines ignored; indentation is spaces, never tabs; a `#`
at the first non-space column is a comment, except `##` at column 0, which opens
a standard.

### 2.1 Workspaces

**One `.hstruct` per crate, at the crate root**, beside its `Cargo.toml`:

```
watershed/crates/watershed/structure.hstruct
watershed/crates/watershed_editor/structure.hstruct
```

`root:` is then always `src` and the manifest is always a sibling, which is what
lets `crate:` be checked rather than decorative. A repo-level file describing
several crates does not work: `root:` cannot name two source trees, and two files
at one repo root collide on the filename.

## 3. A standard

```
## <name — a sentence, not an identifier>

<prose: why this rule exists, and what goes wrong without it>

<verification>
```

The prose is the point. A standard nobody understands the reason for is a
standard that gets deleted the first time it is inconvenient, so the paragraph
arguing for it is the durable part and usually the longest.

Every standard must declare **how it is verified**. A rule that does not say is
a rule that reads as enforced and may not be — `unverified-standard`, an error.
There are two ways.

### 3.1 Counted

```
check: <metric>
budget: <n>
```

The checker counts offenders and compares to the ceiling. Over is an error,
under is a warning naming the tighter number, equal is silence.

### 3.2 Reviewed

```
review: <what a reader checks>
audited: <YYYY-MM-DD>
```

Not mechanical, and says so. This is for the rules that matter most and that no
parser can settle — *"every spawn in an `OnEnter` carries a `DespawnOnExit`, or
it leaks across screens"* is the single most load-bearing rule in `wusel` and is
not statically decidable.

**The first draft exiled these to prose**, on the reasoning that the file must
only contain what a checker can verify. That was the wrong cut, and it is why
that draft drifted toward whatever was easy to count — line numbers and import edges. The honest
requirement is only that a rule declares its verification mode. A `review:`
standard with a date says plainly that a person last looked on that day, which
is more than prose can say and can be seen to go stale.

An undated `review:` is `missing-audit`, a warning: nothing records that anyone
ever looked.

## 4. Metrics

The closed set the checker counts. Every one is a **count of offenders**,
computable from the code alone, needing no per-module configuration — a metric
that needed configuration would put the enumeration back in the file.

A module is a **file**. Nothing declares the set; it is derived from `root:`, so
a new file joins every metric with no edit anywhere.

### Maintainability

| `check:` | counts |
|---|---|
| `import cycles` | pairs of modules that import each other |
| `modules importing more than <n>` | modules whose fan-out exceeds `n` |
| `files over <n> lines` | files longer than `n` |
| `multi-writer types` | types more than one module takes `&mut` to |

`multi-writer types` is the ECS question an import graph cannot answer. An import
edge says who *knows about* whom; who may *change* what is sharper, because a
module that only reads a resource can be rewritten without asking anyone.
Mutable access is the definition — `ResMut<T>`, or `&mut T` inside a `Query` or
`Single` — not an attempt to prove a write happens: asking for `&mut` is what the
schedule reasons about.

Fan-out is listed before file size deliberately. A long file doing one thing is
easier to work with than a short one wired to nine others.

### Performance

| `check:` | counts |
|---|---|
| `unfiltered queries` | `Query` params with no `With` / `Without` / `Changed` / `Added` |
| `allocating systems` | functions taking a system param whose body also allocates |

**Both are deliberately rough**, and a budget is what makes rough usable. Some
unfiltered queries are correct; `allocating systems` matches names rather than
types, so it over-counts a `Vec` built once at startup and under-counts an
allocation behind a helper.

Under a per-instance waiver scheme every one of those would have to be
adjudicated, and re-adjudicated on every edit — unusable, and the reason the
first draft had no performance metric at all. Against a budget the count only has to be *monotone*:
getting worse is caught, and the noise is constant.

## 5. The ratchet

`hobby-struct-parser --ratchet <repo>` rewrites every `budget:` the code has
moved below, in place.

**Only downward.** A budget that rose would record a regression as the new
normal, which is the one thing a ratchet must never do — an exceeded budget stays
an error until the code is fixed.

This is the whole improvement loop, and the reason a repo can adopt the format on
its worst day. Write the standards with `budget: 999`, run `--ratchet` once, and
the file now records exactly how far the code is from each rule. Every later
refactor that improves a number is one command away from being recorded.

## 6. Diagnostics

| Code | Severity | Meaning |
|---|---|---|
| `missing-header-key` | error | `repo:` or `root:` absent |
| `unknown-header-key` | error | a header key outside §2 |
| `malformed-header-line` | error | not `key: value` |
| `tab-indent` | error | indentation using a tab |
| `no-standards` | error | a file that states nothing |
| `duplicate-standard` | error | two standards with one name |
| `unverified-standard` | error | neither `check:` nor `review:` |
| `two-verifications` | error | both `check:` and `review:` |
| `unknown-metric` | error | a `check:` outside §4 |
| `malformed-budget` | error | a budget that is not a whole number |
| `missing-budget` | error | a `check:` with no ceiling |
| `budget-exceeded` | error | the code moved further from the standard |
| `missing-root` | error | no source root at `root:` |
| `crate-name-mismatch` | error | `crate:` disagrees with the sibling manifest |
| `missing-manifest` | error | `crate:` set and no `Cargo.toml` beside the file |
| `budget-slack` | warning | the code improved; tighten with `--ratchet` |
| `missing-audit` | warning | a `review:` with no date |

`--lint` exits non-zero on any diagnostic, warnings included: a warning that
never fails is a warning nobody reads.

## 7. How the checker reads the code

Rust is parsed with `tree-sitter-rust`, not matched as text, and **an import is
resolved by path rather than by its last segment**. Each file has a Rust module
path from its location (`terrain/brush.rs` is `terrain::brush`, `ui/mod.rs` is
`ui`); a `use` path is rooted through `crate::` / `super::` / `self::` and matched
by its longest prefix naming a real file.

Both halves were learned the hard way. Text matching read
`use crate::gameplay::{city, growth};` as one import of a module called
`gameplay`. Leaf-segment matching then blamed `use crate::terrain::brush::Brush`
on an unrelated top-level `brush.rs`, and read `use bevy::app::…` as a local
module named `app`.

## 8. Worked example

`wusel`, complete, abridged only in its prose:

```
repo: wusel
root: src

## State has one owner

Two modules writing one resource means every change to it has to be read twice,
and a third writer arrives unnoticed. Reading is free; writing is the thing
that has to be accounted for.

check: multi-writer types
budget: 10

## Modules form a DAG

check: import cycles
budget: 20

## A module has few collaborators

check: modules importing more than 6
budget: 13

## A file fits in one head

check: files over 900 lines
budget: 10

## Per-frame work is proportional to what changed

The world is 4096x4096 tiles. A system that looks at all of it every frame is
sixteen million cells of work for a handful of changes, and it gets slower as
the world gets more interesting rather than as the player does more.

check: unfiltered queries
budget: 26

## Systems do not allocate

check: allocating systems
budget: 22

## Screen content is torn down by tag

Content is spawned in `OnEnter(Screen::X)` and removed by tagging entities
`DespawnOnExit(Screen::X)`. There is no cleanup system, so an entity spawned
without that tag leaks across screens.

review: every spawn in an OnEnter carries a DespawnOnExit, or hangs on the camera and is removed by hand
audited: 2026-09-01
```

Seven standards. Nothing named. Nothing that an ordinary day of work would force
an edit to.

## 9. Authoring checklist

1. Write the standards you actually hold, prose first. If you cannot say what
   goes wrong without a rule, you do not want the rule.
2. Give each a `check:` from §4 with `budget: 999`, or a `review:` with today's
   date.
3. Run `--ratchet`. The budgets are now the truth about the repo.
4. Run `--detail` once to see what the numbers are made of. Do not paste any of
   it into the file.
