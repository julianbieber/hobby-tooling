# gh-issue-update

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| Type         | skill                                       |
| Install path | `~/.claude-hobby/commands/gh-issue-update.md` |
| Owner        | @julianbieber                               |
| Dependencies | `gh` CLI authenticated; task with a `gh-<n>` prefix |
| Status       | experimental                                |

## Description

Posts a short, evidence-cited progress comment on the task's GitHub issue. Reads
prior comments first to find the genuine delta, links to PRs and commits instead
of restating them, and **declines to post** when nothing clears the bar.

Replaces the work account's `julian-update-jira`.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/gh-issue-update
```

## Known Limitations

- Needs an issue number, taken from a `gh-<n>` task/branch prefix or the notes.
  Tasks with no issue stop with a pointer to `/gh-issue-create` rather than
  inventing one.
- Deliberately posts nothing when there is no verifiable new outcome.
