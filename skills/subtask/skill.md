---
description: Split current work into a subtask with an optional issue in its notes
allowed-tools: Skill, Bash(git branch:*), Bash(git status:*), Bash(git rev-parse:*), Bash(ls:*), Bash(pwd:*), Bash(mkdir:*), Bash(nu -c:*), Read, Write, Edit
---

Split part of the current task into a separate subtask with its own worktree,
notes, and optionally an `issue.md`.

**Arguments:** $ARGUMENTS — a short description of what to split out (e.g.,
"extract the spatial hash into its own module")

## Instructions

1. **Extract the parent task context** from the current worktree path:

   - Run `pwd`. The layout is `~/fun_worktrees/PROJECT/TOPIC/TASK`.
   - **First** component is the project, **second** is the topic, **third** is the
     parent task. There is no repo level — the task directory is the worktree.
   - Example: `~/fun_worktrees/bevy_survivor/features/enemy-waves`
     - project `bevy_survivor`, topic `features`, parent task `enemy-waves`
   - The subtask stays in the same project and topic as its parent.
   - If not in a worktree, ask the user for the project, topic, and parent task.

2. **Determine subtask details** from `$ARGUMENTS`:

   - If no arguments were given, ask what should be split out.
   - The description becomes the subtask summary.

3. **Ask the user** two questions:

   a. **Write an issue?** (yes/no) — should the subtask get an `issue.md` of its
   own, linked to the parent?
   b. **Subtask ID** — a short plain slug (e.g. `spatial_hash`), or suggest one
   derived from the description. It must not already exist under
   `notes/{project}/`.

4. **Create the worktree:**

   ```bash
   nu -c "use ~/.config/nushell/hwt.nu *; hwt new {subtask_id} -p {project} -t {topic} --no-cd"
   ```

   Same project and topic as the parent. `--no-cd` because this runs from bash.

5. **If writing an issue:** run `/issue-create` (Skill tool) as a **refresh** of
   `{project}/{topic}/{subtask_id}`, passing the subtask description and the
   parent task to name in its Context section. That skill writes `issue.md` and
   runs the task-shaping check; this one does not write issues itself.

6. **Create the notes directory** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{subtask_id}/`:

   - `mkdir -p` the directory
   - Write `implementation.md`:

     ```markdown
     ---
     title: "{subtask_id} — Implementation"
     created: {YYYY-MM-DD}
     tags:
       - implementation
       - subtask
       - {project}
     ---

     ## Current State
     **Status:** Not Started
     **Parent:** {parent_task}
     **Focus:** {subtask description}
     **Blockers:** None
     **Next:** Begin implementation

     ## Goal

     {subtask description from arguments}

     Split from [{parent_task}](../{parent_task}/implementation.md).

     ## Requirements

     {to be defined}

     ## Plan

     {to be defined}

     ## Key Files

     {to be defined}
     ```

   The parent link is a sibling path (`../{parent_task}/`) because parent and
   subtask share a project and topic.

7. **Report to the user:**

   - Subtask ID and worktree path
   - The `issue.md` path, if written
   - Navigate: `hwt go {subtask_id}`
   - Start a session there: `hcdev wt {subtask_id}`

8. **Git:** `~/fun_repos/hobby-mimisbrunnr/` is a git repo. After writing the
   notes files, **do not run `git add` or `git commit`** — leave that to the user.
