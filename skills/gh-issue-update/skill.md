---
description: Decide whether the task's GitHub issue warrants a progress comment, gather the evidence for one, and write the comment for me to edit before it is posted
allowed-tools: Bash(git branch:*), Bash(git -C:*), Bash(ls:*), Bash(pwd:*), Bash(mkdir:*), Bash(git log:*), Bash(gh:*), Read, Write
---

Work out whether there is anything worth saying on the GitHub issue behind the
current task, gather the evidence, and write the comment from it. Show me the
comment before posting it; I edit what I disagree with.

The goal is **low noise**: link to where the detail already lives (PRs, commits)
instead of restating it, and surface only outcomes backed by evidence.

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

1. **Determine the project and issue number:**

   a. **From worktree path**: run `pwd`. Under
   `~/fun_worktrees/PROJECT/TOPIC/TASK`, the **first** component is the project
   and the **third** is the task. A `gh-<number>` prefix on the task gives the
   issue number. Example: `~/fun_worktrees/bevy_survivor/features/gh-42-enemy-waves`
   -> project `bevy_survivor`, issue `42`.

   b. **From git branch**: `git branch --show-current` — a leading `gh-<number>`
   gives the issue. Skip `main`, `master`, `develop`.

   c. **From the notes dir**: check `prs.json` and journal files for a `gh-<n>`
   reference.

   d. **From recent conversation**: if an issue number was mentioned.

   **If there is no issue number, stop and say so.** Many hobby tasks have no
   issue, and inventing one is worse than posting nothing. Offer
   `/gh-issue-create` instead.

2. **Read prior comments to find the delta:**

   ```bash
   gh issue view <n> --json title,body,state,comments
   ```

   Find the last progress comment and work out what is genuinely new since it.
   Do NOT repeat anything already posted, and do NOT repeat detail that already
   lives in a linked PR.

3. **Gather evidence for the delta** — this is what makes a comment worth
   posting:

   - `git log --oneline` since the last update (commit SHAs).
   - PRs for the branch: `gh pr view --json number,url,state,title`. Capture the
     PR reference — that is the link you cite instead of restating the diff.
   - Test or command output that proves an outcome. Without evidence, an outcome
     is "in progress", not "done".

4. **Decide whether a comment is warranted** (Decision Rules below). If nothing
   clears the bar, tell the user there is nothing worth posting and stop — no
   filler comments.

5. **Collect the facts** — the evidence the comment is written from, kept
   apart from the comment itself:

   ```markdown
   ## New since the last comment
   - PR #<n> <state> — <url>
   - <sha> <subject>
   - passing: <test names>, from `<command>`
   - blocker: <what>, evidence <where>

   ## Deliberately not surfaced
   - <what, and why it does not clear the bar>

   ## Last comment on the issue
   <date> — <first line, for me to avoid repeating it>
   ```

   **Link, don't duplicate.** Reference the PR (`#123`) or commit SHA; never paste
   diffs, file lists or code that already lives in the PR.

6. **Post it.** Write the comment from the evidence gathered above to a temp
   file (`mktemp`), show it to me, and once I confirm:

   ```bash
   gh issue comment <n> --body-file <file>
   ```

   Run it from the project's worktree or main checkout so `gh` resolves the right
   repo.

7. **Confirm** what was posted, and what you deliberately skipped.

## Decision Rules — what's worth a comment

**Post when there is a new, verifiable outcome:**

- A PR was opened or merged (link it, cite # and state).
- A blocker was resolved or a new one was hit (cite the evidence).
- A decision was made that changes scope or approach (state the decision).
- A verifiable result landed: a test now passing (name it), a bug reproduced or
  fixed, a build confirmed green.
- Status genuinely changed.

**Skip (do NOT post):**

- Work already described in a linked PR — link the PR instead.
- In-progress churn with no outcome (debugging, refactors, compile fixes).
- Anything you cannot back with evidence — unless you explicitly mark it
  "in progress / attempted" AND it carries real signal (e.g. a new blocker).
- Restating the diff or file list. The PR is the source of truth for *what
  changed*; the issue comment is for *why it matters and where to look*.

## Comment shape

The default shape, and the one the facts above line up with:

```markdown
## Update

- {verified outcome — cite PR #, commit SHA, or test name}
- {new blocker / decision, if any — with evidence}

**PRs:** {#123 — title (state)}
**Status:** {only if it changed}
**Next:** {only if it changed — concrete, not speculative}
```

Example of the register (mine, from an earlier task — links and cites, no
restated diff):

```markdown
## Update

- Chunk seams root-caused: the mesher computed normals per-tile, so edge vertices
  disagreed between neighbours (visible as lit cracks at every lod boundary).
- Fix raised in PR #58 (open); borrows one row/column from adjacent tiles.

**PRs:** #58 — Fix terrain chunk seam normals (open)
**Status:** In review
**Next:** Confirm no visible seams at lod 0-3 before merging.
```
