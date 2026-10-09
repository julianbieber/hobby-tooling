---
description: Split current work into a subtask with an optional GitHub issue
allowed-tools: Skill, Bash(git branch:*), Bash(git status:*), Bash(git rev-parse:*), Bash(gh issue:*), Bash(gh repo:*), Bash(ls:*), Bash(pwd:*), Bash(mkdir:*), Bash(nu -c:*), Read, Write, Edit
---

Split part of the current task into a separate subtask with its own worktree,
notes, and optionally a GitHub issue.

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

   a. **Create a GitHub issue?** (yes/no) — should an issue be opened in the
   project's repo and linked to the parent's issue?
   b. **Subtask ID** — if creating an issue, use `gh-{number}-{slug}` (e.g.
   `gh-42-spatial-hash`). If not, ask for a short slug (e.g. `spatial-hash`) or
   suggest one derived from the description.

4. **If creating a GitHub issue:**

   a. Find the parent's issue number, if any. Check in order:
      - `prs.json` / notes in the parent's notes dir for a `gh-<n>` reference
      - the parent task name itself, if it carries a `gh-<n>` prefix
      - `gh issue list --search "<parent task name>"` in the project repo
   b. File it with `/gh-issue-create` (Skill tool), passing the subtask
      description, the project, the parent's topic as the `topic:` label, and
      the parent task and issue number (`#<n>`) to name in the body so GitHub
      cross-links them. That skill writes the label and runs the task-shaping
      check; this one does not file issues itself.
   c. `/gh-issue-create` reports the issue URL; its trailing number gives the
      subtask ID: `gh-{number}-{slug}`.

5. **Create the worktree:**

   ```bash
   nu -c "use ~/.config/nushell/hwt.nu *; hwt new {subtask_id} -p {project} -t {topic} --no-cd"
   ```

   Same project and topic as the parent. `--no-cd` because this runs from bash.

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
   - GitHub issue URL, if created
   - Navigate: `hwt go {subtask_id}`
   - Start a session there: `hcdev wt {subtask_id}`

8. **Git:** `~/fun_repos/hobby-mimisbrunnr/` is a git repo. After writing the
   notes files, **do not run `git add` or `git commit`** — leave that to the user.
