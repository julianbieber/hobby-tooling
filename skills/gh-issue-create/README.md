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

## It also tags the topic

Every issue gets a **`topic:<name>`** label, creating the label in the repo when
it is missing. That label is the one component of `~/fun_worktrees/PROJECT/TOPIC/TASK`
an issue cannot otherwise supply — the project is the repo, the task is
`gh-<number>-<slug>` — and it is what gives `hdash` a one-click **Start task**
button. An untagged issue shows up in the dashboard as *untagged* and cannot be
launched, so the skill resolves a topic (from the worktree path, from the project's
existing topics, or by asking) rather than skipping the label.

Prefixed on purpose: a bare `features` label would be indistinguishable from the
repo's ordinary `bug`/`enhancement` labels, and the prefix lets an issue name a
topic whose directory doesn't exist yet.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/gh-issue-create
```

Resolves the repo from the worktree path or `git remote`. Confirms before any
write.

## Known Limitations

- On a refresh it writes the body and, if absent, the `topic:` label — never the
  title, other labels, assignees, or state.
- One topic per issue; the dashboard takes the first `topic:` label it sees.
- Requires a GitHub remote; a purely local hobby repo has nowhere to file an issue.

## It also checks the issue's shape

An issue is a task, so the **Task shaping** rules in `CLAUDE.md` apply to it. The
skill checks the three that are decidable from the text — a named user-visible
behaviour (or a declared `enabling` with its consumer), a stated done-condition,
and no unanswered question in the body — and **reports** what fails rather than
rewriting the issue to pass. A failing shape usually means the work is two issues,
which is a decision, not a wording fix. `/task-split` is the pass that does the
cutting.
