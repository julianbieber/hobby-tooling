---
description: Carve the current task into a sibling subtask worktree (new blurb, linked to the parent) with seeded notes, recorded as owned in the parent's subtasks.json — sets up directories only, launches nothing
allowed-tools: Bash(pwd:*), Bash(date:*), Bash(mkdir:*), Bash(ls:*), Bash(test:*), Bash(nu -c:*), Bash(git:*), Read, Write, Edit
---

Split a focused piece of the current task into its own sibling worktree.
`/spawn` **only sets up directories and notes** — it creates the worktree, seeds
the subtask's notes, and records the subtask under the current task's ownership
meta. It does **not** launch a session: open the subtask later from the
`hdash` dashboard (the parent's card lists its subtasks), or with
`hwt go` / `hcdev wt`.

**Arguments:** `$ARGUMENTS` — a short description of the subgoal. This is both
the subtask's brief and the source of its blurb (the new branch/dir name
suffix).

## Instructions

1. **Parse `$ARGUMENTS`** as the subgoal/goal text. If empty, ask the user for
   the subgoal. Do not invent one.

2. **Detect the parent task context** from `pwd`:

   - Worktree path is `~/fun_worktrees/PROJECT/TOPIC/TASK` — 1st subdir is the
     **project**, 2nd is the **topic**, 3rd is the **parent task**. There is no
     repo level: the task directory is itself the worktree.
   - **The parent MUST be in a worktree** — `/spawn` always creates a worktree
     and needs the project, topic, and parent task. If `pwd` is not under
     `~/fun_worktrees/<project>/<topic>/<task>`, **fail with a clear error**
     ("/spawn must be run from inside a ~/fun_worktrees/PROJECT/TOPIC/TASK
     worktree") and stop.

3. **Pick a blurb**: 2–4 words summarizing the subgoal, lowercase,
   dash-separated, ≤30 chars. Examples: `refactor-price-calc`,
   `investigate-amazon-feed`, `write-migration`. (Normalize like: lowercase,
   replace runs of non-`[a-z0-9]` with `-`, strip leading/trailing `-`, truncate
   to ~30 chars.)

4. **The new task name** is `new_task = {blurb}` — a sibling under the same
   topic, tied to the parent by its `subtasks.json` entry and the parent link in
   its notes, not by its name.
   - No timestamp suffix. `hwt new` is idempotent, so re-running with the same
     blurb just reuses the existing worktree.

5. **Create the worktree** (same topic + project as the parent):

   ```
   nu -c "use ~/.config/nushell/hwt.nu *; hwt new {new_task} -p {project} -t {topic} --no-cd"
   ```

   That puts the worktree at `~/fun_worktrees/{project}/{topic}/{new_task}` on a new
   branch named `{new_task}`.

6. **Seed the subtask's notes** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{new_task}/`:

   - `mkdir -p` the directory.
   - Write `implementation.md` (the self-contained brief — the subtask session
     has no memory of this conversation, so make the goal complete: file paths,
     constraints, definition-of-done):
     ```markdown
     ---
     title: "{new_task} — Implementation"
     created: {YYYY-MM-DD}
     tags:
       - implementation
       - subtask
     ---

     ## Current State
     **Status:** Not Started
     **Parent:** {parent_task}
     **Focus:** {subgoal}
     **Blockers:** None
     **Next:** Begin implementation

     ## Goal

     {subgoal — written as a full brief, not just a title}

     Split from [{parent_task}](../{parent_task}/implementation.md).
     {when the parent has an issue.md: The problem it serves: [{parent_task}](../{parent_task}/issue.md).}

     ## Requirements

     {to be defined}

     ## Plan

     {to be defined}

     ## Key Files

     {to be defined}
     ```

7. **Record ownership** in the **parent's** notes directory
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{parent_task}/subtasks.json`:

   - `mkdir -p` the parent notes dir if needed.
   - `Read` the existing `subtasks.json`; treat missing or empty as `[]`. It is
     a JSON array.
   - Append this entry unless an entry with the same `task` already exists
     (dedup by `task`):
     ```json
     { "project": "{project}", "topic": "{topic}", "task": "{new_task}",
       "blurb": "{blurb}", "goal": "{subgoal}", "created": "{YYYY-MM-DD}" }
     ```
     `project` is recorded even though a subtask always shares its parent's
     project — it keeps the entry self-describing for `hdash`.
   - `Write` the updated array back.

8. **Report to the user** (terse):

   - New task id and worktree path.
   - Child notes path (`.../notes/{project}/{topic}/{new_task}/implementation.md`).
   - That it's recorded in the parent's `subtasks.json` and now appears under
     the parent task in `hdash`, where the dashboard's open action launches it.
   - Nav reminder: `hwt go {new_task}` / `hcdev wt {new_task}`.

9. **Git:** The notes repo (`hobby-mimisbrunnr`) is git-backed. Do
   **not** `git add`/`git commit` the seeded notes or `subtasks.json` — leave
   that to the user.
