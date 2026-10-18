# issue-update

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| Type         | skill                                       |
| Install path | `~/.claude-hobby/commands/issue-update.md`  |
| Owner        | @julianbieber                               |
| Dependencies | a task whose notes dir holds an `issue.md`  |
| Status       | experimental                                |

## Description

Appends a short, evidence-cited progress entry to the task's `issue.md`, under a
`## Progress` heading, one `### YYYY-MM-DD` per entry. Reads the earlier entries
first to find the genuine delta, links to PRs and commits instead of restating
them, and **declines to write** when nothing clears the bar.

The port of the old GitHub-comment skill: the issue is a notes file now (see
`issue-create`), so a comment thread became a section of that file. It is
separate from the journal on purpose — the journal is the running log of a
session, the progress section is the few lines that belong beside the problem
statement.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/issue-update
```

## Known Limitations

- Needs an `issue.md` in the task's notes dir. A task with none stops with a
  pointer to `/issue-create` rather than inventing one.
- Deliberately writes nothing when there is no verifiable new outcome.
- Append-only: it never edits the body or an earlier entry.
