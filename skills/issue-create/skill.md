---
description: Gather the facts an issue needs, resolve its project, topic and task name, and write it to the task's notes directory as issue.md for me to edit
allowed-tools: Bash(git branch:*), Bash(git remote:*), Bash(git rev-parse:*), Bash(pwd:*), Bash(ls:*), Bash(mkdir:*), Bash(date:*), Bash(nu -c:*), Read, Write, Edit
---

Resolve which project, topic and task an issue belongs to, gather what has been
established about the problem, and write the title and body from those facts into
`notes/<project>/<topic>/<task>/issue.md` for me to edit.

**An issue is a notes entry, not a GitHub issue.** These repos have one author,
so the notes repo is the only tracker: the issue is `issue.md` beside the task's
`task.json`, and writing both is what makes the task exist for `hwt` and `hdash`.
Nothing is filed anywhere else.

**The facts come first and they constrain the text.** An issue is a problem
statement, so it may assert only what the gathering step actually established —
what was observed, where, and what reproduces it. A cause you have not verified is
a question in the body, not a claim. Show me the text before writing it.

Issues are usually opened before any journal exists, so the conversation is the
source; on a refresh the existing `issue.md` is read as well.

## Facts standard (applies to everything this skill writes)

- **Evidence-gated:** Only call something done/working when there is evidence —
  a passing test (name it), command output, an open/merged PR (#), a commit SHA.
  Without evidence, label it "in progress" / "attempted", never "done".
- **No speculation:** Drop hedges and predictions ("likely", "should",
  "probably", "this will fix"). State only what is known. If something is
  unverified, say so plainly ("not yet verified").
- **Cite the source:** Every non-obvious claim names its evidence inline — PR #,
  commit SHA, file path, test name, or the exact command used.

## Where an issue lives

```
~/fun_repos/hobby-mimisbrunnr/notes/<project>/<topic>/<task>/
    task.json    { "done": false, "blocked": null }   — the task exists
    issue.md     the problem statement                 — what the work is
```

`issue.md` is zk-compatible:

```markdown
---
title: "<title>"
created: YYYY-MM-DD
tags:
  - issue
---

## Problem
## Goal
## Scope / acceptance criteria
## Out of scope
## Context
```

`hdash` shows the `title:` on the task's card, so it is a sentence about the
problem, not the task name. Issues migrated from GitHub also carry a `github:`
line with the closed issue's URL; a refresh keeps it, a new issue never has one.

The three path components:

- **Project** — a directory under `~/fun_repos`. A project with no GitHub remote is
  as good as any other; nothing here needs one.
- **Topic** — directory-shaped (`[A-Za-z0-9._-]+`, no leading dot, no spaces).
  Reuse an existing topic where one fits: `hwt topics -p <project>` lists them from
  the notes tree. A new topic is fine when the work genuinely belongs to none of
  them. `coordinator` is reserved and never a topic for an issue.
- **Task** — a plain slug of the title: lowercase ASCII words joined by `_`, at most
  40 characters, cut at a word boundary. No `gh-<n>` prefix — there is no number to
  point at. It must not already exist anywhere under `notes/<project>/`, since
  `hwt go` and `hwt restore` find a task by name across topics; pick a more specific
  slug when it does.

## The shape check (an issue is a task, so the shaping rules apply)

`CLAUDE.md` -> **Task shaping** is the standard. Before writing the issue, check the
issue against it and **report the result — do not rewrite the issue to pass.** A
badly shaped issue is usually a signal that the work is two issues, and that is my
call to make, not a wording problem to fix in the body.

Three of the five rules are checkable from the text alone and are the ones to run
here:

- **Rule 1** — does the issue name a user-visible behaviour? If it does not, it
  must declare itself `enabling` and name the issue it unlocks. Never invent a
  user-facing claim to make an issue pass; that is the specific failure the rule
  exists to prevent.
- **Rule 2** — is there a stated observation separating done from not-done? If the
  conversation has not established one, say so rather than composing a plausible
  one, and let me write it.
- **Rule 4** — does the body contain an unanswered question? If it does, the
  decision belongs in its own issue in front of this one.

Rules 3 and 5 are about the diff and the sequence, so they are checked at
review and merge time, not here.

For a **split**, say so plainly and propose the cut rather than writing one
issue that holds several tasks. `/task-split` is the pass for that.

## Instructions

1. **Determine the project:**

   a. **From worktree path** (`~/fun_worktrees/PROJECT/TOPIC/TASK`): run `pwd`;
   the **first** component is the project.

   b. **From a main checkout**: `~/fun_repos/<project>` — `git rev-parse
   --show-toplevel` and take the directory name.

   c. Otherwise ask which project the issue belongs to.

2. **Decide: new issue, or refresh an existing one?**

   - I named a task, or the current worktree's notes dir already holds an
     `issue.md` and the conversation is about that same work: a **refresh**.
   - Otherwise a **new** issue. Working inside a worktree does not make the new
     issue that task's — an issue found mid-task is usually a different task.

3. **For a refresh, read the current `issue.md`** whole, and keep its body verbatim
   in the facts — links, references and notes-to-self included. I decide what to
   keep. Do not fold, merge or condense it.

4. **Gather the facts from the conversation** — what was established this session,
   with citations, under the layout below. Facts only: the command that failed and
   its output, the file and line, the tasks mentioned. The title and body are
   written from these in step 7, and assert nothing the facts do not.

5. **Resolve topic and task** (see *Where an issue lives*):

   - Topic: on a refresh, the existing directory's. Otherwise, inside a worktree
     whose topic fits, the **second** component under `~/fun_worktrees/PROJECT/`;
     else the one from `nu -l -c 'hwt topics -p <project>'` that fits; else propose a
     new one and say it is new. If the conversation gives no basis at all, **ask**.
   - Task: on a refresh, the existing name. Otherwise slug the title, then
     `ls ~/fun_repos/hobby-mimisbrunnr/notes/<project>/*/` to check it is unused.

6. **Run the shape check** (see above) and **report the path you resolved** — say
   when the topic would be new, and when a refresh would replace an existing body.
   Report any shaping rule the issue fails, and stop for my decision when the
   answer is that this is more than one issue.

7. **Write it.** Write the title and body from the facts gathered above. Show me
   both, and once I confirm:

   a. **New issue:** `mkdir -p` the notes dir, write `task.json` as
      `{ "done": false, "blocked": null }` (two-space indent, one key per line, the
      shape `hwt` writes), and write `issue.md` with `created:` set to today
      (`date +%F`).

   b. **Refresh:** rewrite `issue.md` in place, keeping `created:`, `github:` and
      any extra tags from the existing frontmatter. Never touch `task.json`, and
      never touch an existing `## Progress` section — that is `/issue-update`'s.

   Leave `git add` / `commit` in the notes repo to me.

8. **Report** the notes path and the title.

9. **Suggest the follow-up** for a new issue: `hdash`, and **Start task** on its
   card, or by hand `hwt restore <task>` then `hcdev wt <task>`. `hwt restore`
   creates the worktree and its branch from the notes entry and leaves `task.json`
   as it is.

## Facts file layout

Facts, with citations — the material the issue's own headings (Problem, Goal,
Scope, Out of scope, Context) are written from.

```markdown
## Observed
- `<command run>` → <output, quoted>
- <file>:<line> — <what is there>

## Referenced this session
- <project>/<topic>/<task> — <title>
- <url>

## Existing body (refresh only)
<the current body, verbatim, in full>

## Path
notes/<project>/<topic>/<task> — topic <existing | new>, task <new | existing>

## Unknown
- <what was discussed but not established>
```
