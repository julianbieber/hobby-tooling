---
description: Split part of the current task into a subtask — a task of its own off the default branch, optionally taking whole commits from this branch — worked now as a tab of this session or postponed, with a seeded brief and an optional issue
allowed-tools: Skill, AskUserQuestion, Bash(git branch:*), Bash(git status:*), Bash(git log:*), Bash(git rev-parse:*), Bash(ls:*), Bash(pwd:*), Bash(date:*), Bash(mkdir:*), Bash(nu -c:*), Read, Write, Edit
---

Split part of the current task into a **subtask**: a task of its own — worktree,
branch, `task.json`, its own PR — recorded in this task's `subtasks.json` and
shown on its hdash card. A subtask is either **worked now**, as a tab of this
task's zellij session, or **postponed**, waiting on its own card until someone
opens it.

A question whose answer is a file, not a merge, is a research aspect
(`/aspect`), not this.

**Arguments:** `$ARGUMENTS` — what to split out (e.g. "extract the spatial hash
into its own module").

## Instructions

1. **Resolve the parent** from `pwd`: `~/fun_worktrees/PROJECT/TOPIC/TASK`, the
   task directory is the worktree. Anywhere else, stop with "/subtask must run
   inside a task worktree". The subtask shares the parent's project and topic.

2. **What to split.** `$ARGUMENTS`, or — when empty — ask. Do not invent it.

3. **Which commits it takes.** List this branch's own commits:

   ```bash
   git log --oneline "$(git rev-parse --abbrev-ref origin/HEAD 2>/dev/null || echo origin/main)..HEAD"
   ```

   If any of them belong to the split-out work, propose them by hash and
   subject. Only **whole commits** move: a commit mixing both halves stays
   here, and you say so. If the tree is dirty and the dirty work belongs to
   the subtask, say that it must be committed first — the commit is mine to
   make or approve, never made silently.

4. **Ask me**, in one AskUserQuestion call:

   a. **Work it now or postpone it?** Now opens it as a tab of this session;
      postponed leaves it on its card.
   b. **Name** — a short plain slug, suggested from the description, not
      already a task under `notes/PROJECT/`.
   c. **Commits to take** — the ones from step 3, or none.
   d. **Write an issue?** — an `issue.md` of its own.

5. **Create it:**

   ```bash
   nu -c "use ~/.config/nushell/hwt.nu *; hwt subtask <name> --goal '<one line>' [--take [<hash> …]] [--tab]"
   ```

   That cuts the worktree from the default branch, cherry-picks the taken
   commits in this branch's order, rebases this branch onto the subtask's, and
   records the entry in `subtasks.json`. On a conflict it aborts and says what
   it left; report that and stop. A rebase of a branch that was already
   pushed needs a force push later — say so when it happened.

6. **Write the brief** —
   `~/fun_repos/hobby-mimisbrunnr/notes/PROJECT/TOPIC/<name>/implementation.md`.
   The subtask's session has no memory of this conversation, so make it
   complete: goal, file paths, constraints, done-when, and which commits it
   took.

   ```markdown
   ---
   title: "<name> — Implementation"
   created: YYYY-MM-DD
   tags:
     - implementation
     - subtask
   ---

   ## Current State
   **Status:** Not Started
   **Parent:** TASK
   **Focus:** <one line>
   **Next:** Begin implementation

   ## Goal
   <the full brief>

   Split from TASK.

   ## Taken over
   <commits moved here from TASK, or "nothing">

   ## Done when
   <one observation someone can make>
   ```

7. **If writing an issue:** run `/issue-create` (Skill tool) as a refresh of
   `PROJECT/TOPIC/<name>`, naming the parent in its Context section. That skill
   writes `issue.md` and runs the task-shaping check.

8. **If worked now:** `nu -c "use ~/.config/nushell/hcdev.nu *; hcdev tab <name>"`.

9. **Report:** the subtask, its worktree, its notes, what it took, and — worked
   now — that its tab is open; postponed — that it waits on this task's hdash
   card, and `hcdev tab <name>` or `hcdev wt <name>` opens it later.

Never commit to the notes repo.
