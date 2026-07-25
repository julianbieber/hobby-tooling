# Triage

| Field        | Value                                                                |
| ------------ | -------------------------------------------------------------------- |
| Type         | skill                                                                |
| Install path | `~/.claude-hobby/commands/triage.md`                                                   |
| Owner        | @julianbieber                                                        |
| Dependencies | `~/.claude-hobby/context/known-issues.md`; notes repo with journal entries |
| Status       | experimental                                                         |

## Description

Collects open action items from `known-issues.md` and recent daily journals
(last 30 days), deduplicates, and walks through each item with the user.
Supported decisions per item: `do` (turn into a GitHub issue/worktree), `defer`
(with optional review date), `drop` (mark `- [x] DROPPED:`), `done`, or
`delegate`. Batch decisions are supported.

Applies the decisions in place in `known-issues.md` and prints a count summary.

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/triage/skill.md \
  ~/.claude-hobby/commands/triage.md
```

Or: `just install-skills`.

## Usage

```
/triage
```

## Known Limitations

- Journal scan uses a plain `find -mtime -30` — files touched for unrelated
  reasons will be scanned too. Harmless, just slower.
- Doesn't auto-create GitHub issues for `do` items; delegates to
  `/gh-issue-create` or a manual flow.
