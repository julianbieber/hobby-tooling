# Spawn

| Field        | Value                 |
| ------------ | --------------------- |
| Type         | skill                 |
| Install path | `~/.claude-hobby/commands/spawn.md` |
| Owner        | @julianbieber         |
| Dependencies | nushell `hwt.nu` |
| Status       | experimental          |

## Description

Carves a focused piece of the current task into its own **sibling worktree** and
sets up its directories — but launches nothing. Given a subgoal it:

- Detects the parent task context from `pwd` (must be inside a
  `~/fun_worktrees/PROJECT/TOPIC/TASK` worktree).
- Derives a blurb from the subgoal and names the new task `{blurb}`.
- Creates the worktree (same topic + project) via `hwt new`.
- Seeds the subtask's notes directory with a stub `implementation.md` carrying
  the brief and a link back to the parent.
- Records the subtask under the parent's
  `notes/{project}/{topic}/{parent_task}/subtasks.json` ownership meta (deduped by task).

The subtask is surfaced under the parent task in the `hdash` dashboard,
where the dashboard's open action launches it. `/spawn` never starts a session
itself.

Sibling to `subtask`: `/subtask` optionally writes the subtask an `issue.md` of its
own; `/spawn` links back to the parent's and records ownership.

## Installation

```bash
ln -s ~/fun_repos/hobby-tooling/skills/spawn/skill.md \
  ~/.claude-hobby/commands/spawn.md
```

Or: `just install-skills`.

## Usage

```
/spawn refactor price calculation logic
```

## Known Limitations

- Requires nushell with the `hwt.nu` module set up (see the `wt` commands
  documented in `~/fun_worktrees/CLAUDE.md`).
- Must be run from inside a `~/fun_worktrees/PROJECT/TOPIC/TASK` worktree — it
  always creates a worktree and needs the topic, parent task, and project.
- The `subtasks.json` visualization requires `hdash` built from the
  version that reads it.
