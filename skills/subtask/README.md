# Subtask

| Field        | Value                                                                |
| ------------ | -------------------------------------------------------------------- |
| Type         | skill                                                                |
| Install path | `~/.claude-hobby/commands/subtask.md`                                                  |
| Owner        | @julianbieber                                                        |
| Dependencies | nushell `hwt.nu`; `/issue-create` (optional) |
| Status       | experimental                                                         |

## Description

Splits part of the current task into a separate subtask: creates a worktree
(same topic + project as the parent), creates a notes directory with a stub
`implementation.md` linking back to the parent, and optionally writes an
`issue.md` for it through `/issue-create`.

Asks whether to write an issue, takes a short description as `$ARGUMENTS`,
and reports the new worktree path plus the `issue.md` path if one was written.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/subtask/skill.md \
  ~/.claude-hobby/commands/subtask.md
```

Or: `just install-skills`.

## Usage

```
/subtask refactor price calculation logic
```

## Known Limitations

- Requires nushell with the `hwt.nu` module set up (see the `wt` commands
  documented in `~/fun_worktrees/CLAUDE.md`).
