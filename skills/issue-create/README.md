# issue-create

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| Type         | skill                                       |
| Install path | `~/.claude-hobby/commands/issue-create.md`  |
| Owner        | @julianbieber                               |
| Dependencies | the notes repo at `~/fun_repos/hobby-mimisbrunnr` |
| Status       | experimental                                |

## Description

Drafts an issue from the **current conversation**, shows it for confirmation, then
writes it to `notes/<project>/<topic>/<task>/issue.md` beside a fresh `task.json`
— or replaces the body of an existing `issue.md` on a refresh.

The notes repo is the only tracker. These repos have one author, and keeping a
GitHub issue beside the notes entry was double bookkeeping: two places recording
that a piece of work exists, with a label (`topic:`) and a backlog mirror whose
only job was making the two agree. The open GitHub issues were copied into notes
as `issue.md` (with a `github:` line naming the closed original) and closed.

Preserves meaningful existing body content on a refresh rather than overwriting
blindly. Marks genuine unknowns as unknown instead of guessing.

## It resolves the whole path

An issue now has to name all three components of
`~/fun_worktrees/PROJECT/TOPIC/TASK` itself — there is no repo to supply the
project and no issue number to make the task name. Project from the worktree or
checkout, topic from `hwt topics` (or a new one, said to be new), task as a plain
slug of the title that no other task in the project already uses.

Writing `task.json` is what makes the task exist: `hdash` lists it as **Not
started** with a **Start task** button, and `hwt restore <task>` creates its
worktree.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/issue-create
```

Confirms before any write. Never commits the notes repo.

## Known Limitations

- On a refresh it rewrites the body only — never `task.json`, `created:`,
  `github:`, or the `## Progress` section `/issue-update` keeps.
- The task name is fixed at creation; retitling an issue does not rename its
  directory.

## It also checks the issue's shape

An issue is a task, so the **Task shaping** rules in `CLAUDE.md` apply to it. The
skill checks the three that are decidable from the text — a named user-visible
behaviour (or a declared `enabling` with its consumer), a stated done-condition,
and no unanswered question in the body — and **reports** what fails rather than
rewriting the issue to pass. A failing shape usually means the work is two issues,
which is a decision, not a wording fix. `/task-split` is the pass that does the
cutting.
