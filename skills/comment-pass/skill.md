---
description: Enforce the comment rule over a diff or a path — delete comments the rule disallows, relocate the ones carrying real information, and write the module headers, public doc comments and test justifications that are missing
argument-hint: '[<base> | <path>…] [--report-only]'
allowed-tools: Bash(python3 */comment-candidates.py:*), Bash(git diff:*), Bash(git log:*), Bash(git -C:*), Bash(git rev-parse:*), Bash(git merge-base:*), Bash(pwd:*), Bash(ls:*), Read, Grep, Glob, Edit
---

Make the comments match the rule in `~/.claude-hobby/CLAUDE.md`: **a module header,
a public element's contract, a test's justification. Nothing else.**

**This pass goes both ways.** Its predecessor only ever cut, which is why the tree
ended up with placeholders instead of comments. Here, deleting a disallowed comment
and writing a missing one are the same job — a diff that comes out of this pass with
fewer comments and no new ones has probably failed.

## Stage 0 — Scope

1. Default scope is the branch diff. A path argument sweeps that path instead; a
   ref argument is the diff base.

   ```bash
   python3 ~/fun_repos/hobby-tooling/scripts/comment-candidates.py --diff [<base>]
   python3 ~/fun_repos/hobby-tooling/scripts/comment-candidates.py <path>…
   ```

   It emits `path:line<TAB>CATEGORY<TAB>detail` for the five states the rule can
   detect mechanically. Report the counts per category before changing anything.

2. **Vendored crates are report-only**: `mcps/just-mcp`, `mcps/lsp-mcp`,
   `mcps/tree-extract`, `lsps/claude-lsp`, `lsps/claude-skills-lsp`,
   `lsps/treesitter-class-lsp`, `lsps/path-lsp`. They are copies of work crates and
   every rewrite here widens the drift a re-sync has to reconcile. List their
   findings, change nothing, unless I say otherwise.

3. `--report-only` stops after the report everywhere. It is this skill's flag —
   never pass it through to the detector, which rejects unknown flags.

## Stage 1 — What the detector found

Work the categories in this order. The first two remove, the last three add.

**`BARE-COMMENT` — a comment where the rule allows none.** Do not reflexively
delete. Read what it says, then pick one:

| the comment carries | do |
|---|---|
| nothing the code doesn't already say (`// Parse URI` above `Url::parse`) | delete |
| a contract a caller needs — preconditions, what an error case really is, observable state change | move it into the enclosing item's `///`, rewritten as a contract; delete the inline |
| a fact about the enclosing test | move it above the test as its justification |
| why the body is shaped this way rather than the obvious way | **delete, and list it in the report as a commit-message note** — it is a decision, and decisions live in the commit that made them |
| a section banner (`// ----`) | delete; if the file needed them to be navigable, say so in the report — that is a split, not a comment |

The third and fourth rows are where the information actually is. A pass that
resolves every `BARE-COMMENT` to "delete" has not read them.

**`PRIV-DOC` — a doc comment on a private element.** Delete it. If losing it makes
the element hard to follow, the fix is the name or the shape, not the comment:
report that instead of quietly keeping the comment.

**`MOD-UNDOC` — no module header.** Write what the module is *responsible for*.
The constraint that matters: this comment is read long after, far from the code, so
it may contain only what survives the contents changing. Name the responsibility.
Never list what is currently in the file — that is wrong the first time something
moves. If you cannot state a single responsibility, the finding is that the module
does not have one; report it rather than writing a header that lists two.

**`PUB-UNDOC` — a public element with no doc comment.** Write what the caller needs
to use it correctly, and stop: preconditions, what the error cases actually are,
what it does to state the caller can observe. Not how it works inside. Not the name
again — if the doc comment would only restate the signature, the honest output is no
comment, and the finding is that the item is self-describing. Say so.

**`TEST-UNJUSTIFIED` — a test with no justification.** One line above the test: why
it exists and which behaviour it pins. If you cannot write that line from the test's
own body, **that is the finding** — the rule says the test should not exist. Report
it for me to decide. Never invent a rationale to satisfy the check.

## Stage 2 — What the detector cannot see

A comment in a legal place can still be the wrong comment. Read every `//!`, `///`
and test justification in scope against the code it sits on:

- a module header that lists contents rather than naming a responsibility, or that
  is already false;
- a `///` that describes the implementation instead of the contract;
- a `///` that restates the signature — `/// Check if two LSP ranges overlap` on
  `fn ranges_overlap(a: &Range, b: &Range) -> bool`;
- a `///` that is simply wrong about what the code does;
- a test justification that names no behaviour ("tests the parser").

Rewrite these. A wrong comment is worse than a missing one, because the next reader
stops checking.

## The rule that governs every sentence you write here

**Never assert a reason you have not verified in the code.** If you cannot establish
why a function returns an error, what a precondition really is, or why a test exists,
write what you can see or write nothing — and put the gap in the report. A confident,
plausible, wrong comment is the single worst output of this pass: it is load-bearing
for the next reader and it is invisible, where a missing comment is merely missing.

Same applies to the module headers: "responsible for X" is a claim. Verify it against
what the file actually contains before writing it.

## Stage 3 — Report

- counts per category, before and after;
- every comment **relocated**, with where it went;
- every shape-decision note pulled out for the commit message, verbatim — this is
  output I need, not a side effect;
- every finding you deliberately did not act on, with the reason: a test whose
  justification cannot be written, a module with no single responsibility, a public
  item whose doc comment would only restate it, anything you could not verify;
- vendored-crate findings, listed and untouched.

Never `git add` or commit. The edits are the deliverable; the decision to keep them
is mine.
