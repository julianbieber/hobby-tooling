# Subtask

| Field        | Value                                                                |
| ------------ | -------------------------------------------------------------------- |
| Type         | skill                                                                |
| Install path | `~/.claude-hobby/commands/subtask.md`                                                  |
| Owner        | @julianbieber                                                        |
| Dependencies | nushell `hwt.nu`; `gh` CLI (optional, only to create an issue) |
| Status       | experimental                                                         |

## Description

Splits part of the current task into a separate subtask: creates a worktree
(same topic + project as the parent), creates a notes directory with a stub
`implementation.md` linking back to the parent, and optionally creates + links a
GitHub issue.

Asks whether to open a GitHub issue, takes a short description as `$ARGUMENTS`,
and reports the new worktree path plus the issue URL if one was created.

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
- Issue creation runs `gh` from `~/fun_repos/{project}`, so the project must have
  a GitHub remote; otherwise answer no to the issue question.
