---
description: Check a repo against the code quality standards its structure.hstruct states — explain what a count is made of, attempt the fix when one regressed, ratchet a budget the code has moved below, and draft standards for a repo that has none
argument-hint: '[<repo>] [--draft | --explain <standard>]'
allowed-tools: Bash(hobby-struct-parser:*), Bash(git diff:*), Bash(git status:*), Bash(git -C:*), Bash(pwd:*), Bash(ls:*), Bash(wc:*), Read, Grep, Glob, Edit, Write
---

Hold a repo to the standards it states about itself. The file is
`structure.hstruct` at the repo root — or at each crate root in a workspace — and
`lsps/hobby-struct-lsp/grammar.md` is the language.

**A standard is durable; the count is not.** The file stores a rule, the argument
for it, and a budget saying how many offenders exist today. Nothing in it names a
module, a file or a type — an enumeration is what goes stale as an implementation
grows. So this skill never writes an instance into the file, only a number.

## Stage 0 — Scope

Default is the repo the working directory sits in. A `<repo>` argument names one
under `~/fun_repos` instead. In a workspace, every crate root with a file is
checked.

```bash
hobby-struct-parser <repo>
```

**A repo with no `structure.hstruct` is not a finding.** Most have none. Say so in
one line and stop, unless `--draft` was passed.

## Stage 1 — Report

Print the table as the checker gives it: each standard, its count, its budget, and
whether the count is at, under, or over. Then read it out loud in one line —
which standards hold, which the code has moved toward, which it has moved away
from.

Reviewed standards are listed separately with their audit date. **An old date is
information, not a defect.** Do not treat it as something to fix; say when it was
last looked at and move on unless the work at hand touches what it covers.

## Stage 2 — Explain a count

On `--explain <standard>`, or whenever a number is surprising:

```bash
hobby-struct-parser --detail <repo>
```

This lists what each count is made of. **Read it, report what it says, and do not
put any of it in the file.** The offenders are output, never state — writing them
down is exactly the mistake the format exists to avoid.

Two of the metrics are deliberately rough. `unfiltered queries` over-counts a
query that legitimately matches two entities; `allocating systems` matches names
rather than types, so it counts a `Vec` built once at startup and misses one
behind a helper. When explaining either, say which offenders look like noise. Do
not "fix" noise — the budget absorbs it, and the count only has to be monotone.

## Stage 3 — A budget that is exceeded

The code moved further from a standard. In order:

1. **Read what it is made of** (`--detail`) and find what this change added.
2. **Try to fix it.** A new cycle usually wants an event or a read-only query
   rather than a module import; a file crossing the line usually wants the split
   its own responsibility already implies. Fix the code, then re-run.
3. **If the fix does not work, stop and report it as a regression.** Name the
   standard, the count before and after, and what you tried. This is a decision,
   not a failure to route around.

**Never raise a budget.** `--ratchet` only tightens, and loosening is a hand edit
the person doing the work makes deliberately — it appears in the diff as a line
saying this project got further from its own standard, and the PR body has to say
why. A machine widening a standard so its own change fits is the one move this
whole format exists to prevent.

## Stage 4 — A budget with slack

The code improved. Record it:

```bash
hobby-struct-parser --ratchet <repo>
```

Only tightens, never raises, so this is always safe to run. The tightened number
joins the current diff, which is how an improvement gets recorded in the same
change that made it.

## Stage 5 — Drafting standards (`--draft`)

For a repo with no file. **Do not generate the standards from the metric list** —
that produces a file nobody believes, stating rules the project never agreed to.

1. Read the repo: its `CLAUDE.md`, its module layout, what it is for.
2. Propose the standards *this* project actually holds, prose first, and say for
   each what goes wrong without it. A rule you cannot argue for is a rule to drop.
3. Only then pick a `check:` from the grammar's metric list for the ones that are
   countable, and a `review:` with today's date for the ones that are not.
   Reviewed standards are first-class — the rules that matter most are often the
   ones no parser can settle.
4. Write every budget as `999`, then run `--ratchet` once. The file now records
   exactly how far the code is from each rule, on the repo's worst day.
5. Show me the file. It is a statement about the project, so it is mine to edit.

## What this skill does not do

- **It does not write instances.** No file, module or type name enters the file.
- **It does not raise a budget.**
- **It does not add standards during a task.** A task is short and a standard is
  long-lived; deciding a new one is its own conversation, not a side effect.
