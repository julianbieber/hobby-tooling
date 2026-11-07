---
description: Enforce the comment rule over a diff or a path — delete comments the rule disallows, rewrite the ones pitched at the wrong level, and write the missing module headers and the public comments that say more than the signature
argument-hint: '[<base> | <path>…] [--report-only]'
allowed-tools: Bash(python3 */comment-candidates.py:*), Bash(git diff:*), Bash(git log:*), Bash(git -C:*), Bash(git rev-parse:*), Bash(git merge-base:*), Bash(pwd:*), Bash(ls:*), Read, Grep, Glob, Edit
---

Make the comments match the rule in `~/.claude-hobby/CLAUDE.md`: **a module
header, and a public element's comment when it tells the caller something the
signature does not. Nothing else.**

A comment is written for a caller, at the level they use the code. The failures
this pass exists to remove are the ones a model produces by default: a comment
that restates the name, one that walks through the body, and one that belongs to
the conversation that wrote the code rather than to the code.

**This pass is mostly a filter.** Coming out with fewer comments is the normal
result. It writes only two things: a missing module header, and a public
comment the caller actually needs.

## Stage 0 — Scope

1. Default scope is the branch diff. A path argument sweeps that path instead; a
   ref argument is the diff base.

   ```bash
   python3 ~/fun_repos/hobby-tooling/scripts/comment-candidates.py --diff [<base>]
   python3 ~/fun_repos/hobby-tooling/scripts/comment-candidates.py <path>…
   ```

   It emits `path:line<TAB>CATEGORY<TAB>detail` for the four placement states it
   can detect mechanically. Report the counts per category before changing
   anything.

2. **Vendored crates are report-only**: `mcps/just-mcp`, `mcps/lsp-mcp`,
   `mcps/tree-extract`, `lsps/claude-lsp`, `lsps/claude-skills-lsp`,
   `lsps/treesitter-class-lsp`, `lsps/path-lsp`. They are copies of work crates and
   every rewrite here widens the drift a re-sync has to reconcile. List their
   findings, change nothing, unless I say otherwise.

3. `--report-only` stops after the report everywhere. It is this skill's flag —
   never pass it through to the detector, which rejects unknown flags.

## Stage 1 — What the detector found

**`BARE-COMMENT` — a comment where the rule allows none**: in a body, a banner,
above a test. Read it, then:

| the comment carries | do |
|---|---|
| nothing the code doesn't already say | delete |
| something a caller needs — a precondition, what an error means, an observable effect | move it into the enclosing public item's comment at the caller's level; delete the inline |
| why the body is shaped this way rather than the obvious way | delete, and list it in the report as a commit-message note |
| what a test pins | delete; if the test's name does not say it, propose a better name in the report |
| a section banner | delete; if the file needed them to be navigable, say so — that is a split, not a comment |

**`PRIV-DOC` — a doc comment on a private element.** Delete it. If losing it
makes the element hard to follow, the fix is the name or the shape: report that.

**`MOD-UNDOC` — no module header.** Write one: the module's responsibility, for
someone deciding whether and how to use it. Never list what is in the file. If
you cannot state a single responsibility, report that instead of writing a header
that lists two.

**`PUB-UNDOC` — a public element with no comment.** A candidate, not a defect.
**Leave it uncommented** unless you can name something the caller needs that the
names and types do not say — a precondition, what an error case means, an effect
on state the caller can see, a unit or range the type does not carry. Write only
that, in a sentence or two. Do not count these as work left over.

## Stage 2 — What the detector cannot see

Read every module header and public comment in scope against the code and the
signature it sits on. Rewrite or delete:

- **restatement** — the name or the types again in prose
  (`/// The project the tasks belong to.` on `pub project: String`): delete;
- **too much detail** — how the body works, the algorithm, the steps: cut to
  what the caller sees, or delete;
- **conversation** — anything that only makes sense to someone who saw the
  task: history (`no longer`, `used to`, `instead of the old …`), references to a
  task, issue, PR, gate, plan or notes file, who asked for it: rewrite as the
  behaviour it describes, or delete;
- **wrong** — says something the code does not do: fix it; a wrong comment is
  worse than none.

The test for every sentence that survives: it reads correctly to someone who
never saw the conversation or the diff, it is at the caller's level, and the
signature does not already say it.

**Never assert what you have not verified in the code.** A behaviour you cannot
establish is not written; the site goes in the report.

## Stage 3 — Report

Short:

- counts per category, before and after;
- comments written — module headers, and public comments with what they add over
  the signature;
- comments relocated, rewritten or deleted for level or conversation, as a count
  with a few examples;
- shape-decision notes pulled out for the commit message, verbatim;
- anything not acted on, with the reason — a module with no single
  responsibility, a private element that needs a better name, a test that needs a
  better name, a behaviour you could not verify;
- vendored-crate findings, listed and untouched.

Never `git add` or commit. The edits are the deliverable; the decision to keep them
is mine.
