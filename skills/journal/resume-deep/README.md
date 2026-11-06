# Journal: Resume Deep

| Field        | Value                                                                         |
| ------------ | ----------------------------------------------------------------------------- |
| Type         | skill                                                                         |
| Install path | `~/.claude-hobby/commands/journal/resume-deep.md`                                                       |
| Owner        | @julianbieber                                                                 |
| Dependencies | Notes repo; `~/.claude-hobby/context/known-issues.md` |
| Status       | experimental                                                                  |

## Description

Front-loads all context for a task in a single pass: current state, journal,
recent git commits in the worktree, and relevant entries from
`known-issues.md`. Prints the `## Current State` section first so you see
status before the rest of the context loads.

Use this instead of `/journal:resume` when you want everything a cold
session would need before you start coding — git history and open action items
included.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/journal/
ln -s ~/fun_repos/hobby-tooling/skills/journal/resume-deep/skill.md \
  ~/.claude-hobby/commands/journal/resume-deep.md
```

Or: `just install-skills`.

## Usage

```
/journal:resume-deep
```

