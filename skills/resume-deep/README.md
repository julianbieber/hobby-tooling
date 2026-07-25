# Resume Deep

| Field        | Value                                                                         |
| ------------ | ----------------------------------------------------------------------------- |
| Type         | skill                                                                         |
| Install path | `~/.claude-hobby/commands/resume-deep.md`                                                       |
| Owner        | @julianbieber                                                                 |
| Dependencies | Notes repo; cached schemas at `~/.claude/context/schemas/`; `known-issues.md` |
| Status       | experimental                                                                  |

## Description

Front-loads all context for a task in a single pass: current state, journal,
cached database schemas, recent git commits per repo in the worktree, and
relevant entries from `known-issues.md`. Prints the `## Current State` section
first so you see status before the rest of the context loads.

Use this instead of `/julian:resume-journal` when you want everything a cold
session would need before you start coding — schema context, git history, and
open action items included.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/resume-deep/skill.md \
  ~/.claude-hobby/commands/resume-deep.md
```

Or: `just install-skills`.

## Usage

```
/resume-deep
```

## Known Limitations

- Expects `~/.claude/context/schemas/` populated by the `refresh-schemas` binary
  (from `mcps/julian-db-mcp/`).
- Will warn if schema cache is older than 7 days.
