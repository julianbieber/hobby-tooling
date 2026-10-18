---
description: Decide whether the task's issue warrants a progress entry, gather the evidence for one, and append it to the issue.md in the task's notes for me to edit
allowed-tools: Bash(git branch:*), Bash(git -C:*), Bash(git log:*), Bash(gh pr:*), Bash(ls:*), Bash(pwd:*), Bash(date:*), Read, Write, Edit
---

Work out whether there is anything worth recording on the issue behind the current
task, gather the evidence, and append it to the task's `issue.md` as a dated
progress entry. Show me the entry before writing it; I edit what I disagree with.

The goal is **low noise**: link to where the detail already lives (PRs, commits,
journal entries) instead of restating it, and surface only outcomes backed by
evidence. The journal is the running log; an issue's progress section is the few
lines someone reading the problem statement needs to know where it stands.

## Facts standard (applies to everything this skill writes)

- **Evidence-gated:** Only call something done/working when there is evidence —
  a passing test (name it), command output, an open/merged PR (#), a commit SHA.
  Without evidence, label it "in progress" / "attempted", never "done".
- **No speculation:** Drop hedges and predictions ("likely", "should",
  "probably", "this will fix"). State only what is known. If something is
  unverified, say so plainly ("not yet verified").
- **Cite the source:** Every non-obvious claim names its evidence inline — PR #,
  commit SHA, file path, test name, or the exact command used.

## Instructions

1. **Find the issue:**

   a. **From worktree path**: run `pwd`. Under
   `~/fun_worktrees/PROJECT/TOPIC/TASK` the three components name the notes dir
   `~/fun_repos/hobby-mimisbrunnr/notes/PROJECT/TOPIC/TASK/`.

   b. **From recent conversation**: if I named a task.

   The issue is that dir's `issue.md`. **If there is none, stop and say so.** Many
   tasks have no issue, and a progress entry with no problem statement above it is
   worse than none. Offer `/issue-create` instead.

2. **Read the existing `issue.md`** — the body and every entry under
   `## Progress`. Work out what is genuinely new since the last entry. Do NOT
   repeat anything already recorded there, and do NOT repeat detail that already
   lives in a linked PR.

3. **Gather evidence for the delta** — this is what makes an entry worth writing:

   - `git log --oneline` since the last entry (commit SHAs).
   - PRs for the task: `prs.json` in the notes dir, then
     `gh pr view <url> --json number,url,state,title` for their current state.
   - Test or command output that proves an outcome. Without evidence, an outcome
     is "in progress", not "done".

4. **Decide whether an entry is warranted** (Decision Rules below). If nothing
   clears the bar, tell me there is nothing worth recording and stop — no filler.

5. **Collect the facts** — the evidence the entry is written from, kept apart from
   the entry itself:

   ```markdown
   ## New since the last entry
   - PR #<n> <state> — <url>
   - <sha> <subject>
   - passing: <test names>, from `<command>`
   - blocker: <what>, evidence <where>

   ## Deliberately not surfaced
   - <what, and why it does not clear the bar>

   ## Last entry on the issue
   <date> — <first line, for me to avoid repeating it>
   ```

   **Link, don't duplicate.** Reference the PR (`#123`) or commit SHA; never paste
   diffs, file lists or code that already lives in the PR.

6. **Write it.** Show me the entry, and once I confirm, append it to `issue.md`:
   add a `## Progress` heading at the end of the file if there is none, then the
   entry under it as `### YYYY-MM-DD` (`date +%F`), newest last. Never edit the
   body above `## Progress` — that is `/issue-create`'s refresh — and never edit an
   earlier entry.

   Leave `git add` / `commit` in the notes repo to me.

7. **Confirm** what was written, and what you deliberately skipped.

## Decision Rules — what's worth an entry

**Write when there is a new, verifiable outcome:**

- A PR was opened or merged (link it, cite # and state).
- A blocker was resolved or a new one was hit (cite the evidence).
- A decision was made that changes scope or approach (state the decision).
- A verifiable result landed: a test now passing (name it), a bug reproduced or
  fixed, a build confirmed green.
- Status genuinely changed.

**Skip (do NOT write):**

- Work already described in a linked PR — link the PR instead.
- In-progress churn with no outcome (debugging, refactors, compile fixes) — that
  is the journal's.
- Anything you cannot back with evidence — unless you explicitly mark it
  "in progress / attempted" AND it carries real signal (e.g. a new blocker).
- Restating the diff or file list. The PR is the source of truth for *what
  changed*; the progress entry is for *why it matters and where to look*.

## Entry shape

```markdown
### YYYY-MM-DD

- {verified outcome — cite PR #, commit SHA, or test name}
- {new blocker / decision, if any — with evidence}

**PRs:** {#123 — title (state)}
**Status:** {only if it changed}
**Next:** {only if it changed — concrete, not speculative}
```

Example of the register (mine, from an earlier task — links and cites, no
restated diff):

```markdown
### 2026-08-14

- Chunk seams root-caused: the mesher computed normals per-tile, so edge vertices
  disagreed between neighbours (visible as lit cracks at every lod boundary).
- Fix raised in PR #58 (open); borrows one row/column from adjacent tiles.

**PRs:** #58 — Fix terrain chunk seam normals (open)
**Status:** In review
**Next:** Confirm no visible seams at lod 0-3 before merging.
```
