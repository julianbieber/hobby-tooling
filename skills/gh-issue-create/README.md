# gh-issue-create

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| Type         | skill                                       |
| Install path | `~/.claude-hobby/commands/gh-issue-create.md` |
| Owner        | @julianbieber                               |
| Dependencies | `gh` CLI authenticated; project has a GitHub remote |
| Status       | experimental                                |

## Description

Drafts a GitHub issue body from the **current conversation** (not from notes —
issues usually predate them), shows it for confirmation, then creates the issue
or replaces the body of an existing one with `gh`.

Replaces the work account's `julian-jira-create`. Hobby projects track work in
GitHub issues; there is no Jira.

Preserves meaningful existing body content on a refresh rather than overwriting
blindly. Marks genuine unknowns as `TODO` instead of guessing.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/gh-issue-create
```

Resolves the repo from the worktree path or `git remote`. Confirms before any
write.

## Known Limitations

- Body only — never touches title, labels, assignees, or state on a refresh.
- Requires a GitHub remote; a purely local hobby repo has nowhere to file an issue.
