# PR Comments

| Field        | Value                   |
| ------------ | ----------------------- |
| Type         | skill                   |
| Install path | `~/.claude-hobby/commands/pr-comments.md` |
| Owner        | @julianbieber           |
| Dependencies | `gh` CLI authenticated  |
| Status       | experimental            |

## Description

Reviews comments on the current branch's PR and works on the ones you've marked
with a thumbs-up reaction. Filters out already-resolved threads, fetches
reactions per comment, and implements the requested change for each thumbs-upped
item.

A thumbs-up on a comment = "go fix this".

## Installation

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/pr-comments/skill.md \
  ~/.claude-hobby/commands/pr-comments.md
```

Or: `just install-skills`.

## Usage

```
/pr-comments
```

## Known Limitations

- Uses the GraphQL API for `isResolved`; falls back to REST when GraphQL isn't
  available (and then can't filter resolved threads).
- Assumes one PR per branch (the output of `gh pr view`).
