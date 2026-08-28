# Build Task

| Field        | Value                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| Type         | skill                                                                  |
| Install path | `~/.claude-hobby/commands/build/task.md` → `/build:task`               |
| Owner        | @julianbieber                                                          |
| Dependencies | `journal:resume`, `hmml-*`, `comment-pass`, `journal:write`            |
| Status       | experimental                                                           |
| Supersedes   | `dev-flow`                                                             |

## Description

The whole task as one pipeline with a recorded stage. Takes a brief from a
description, a GitHub issue, or the task notes; plans it in HMML; loops on my
review until the spec is right; implements and tests; runs `/comment-pass`; stops
for my review; then the PR and the journal entry.

Two hard stops: after the HMML review, and after the implementation review.

## Stages

| phase | runs | ends |
|---|---|---|
| `plan` | Gate 0 prior-art/size, brief, `hmml-new` \| `hmml-extract`, `hmml-review` | hard stop |
| `await-hmml-review` | nothing — I edit the spec | `continue` or `revise` |
| `revise` | `hmml-review` again on the edited spec | back to `await-hmml-review` |
| `implement` | `hmml-translate`, self-review, run + test, `hmml-validate` | falls through |
| `comments` | `/comment-pass` over the branch diff | falls through |
| `review` | prints the diff, evidence and comments for me to read | hard stop |
| `await-review` | nothing — I read the summary | `continue` |
| `pr` | gathers the facts, opens the PR | falls through |
| `journal` | `journal:write` | `done` |

State lives in `build-task.json` in the task's notes dir, beside the `.hmml`.

## Installation

```bash
just install-skills
```

Removing `skills/dev-flow/` is enough to retire `/dev-flow` — `install-skills`
prunes stale symlinks on the next run.

## Usage

```
/build:task "make the spawner respect the wave budget"
/build:task #42
/build:task --notes
/build:task implement
```

## Known Limitations

- Comments are checked by `/comment-pass`; commit messages and the PR body are
  written to the rules in `~/.claude-hobby/CLAUDE.md` but nothing verifies them.
- Inferring the starting phase from artifacts is a guess when a task was worked on
  outside the pipeline.

## TODO(jb-doc): why the pipeline stops exactly twice

<!-- What it should cover: what makes those two points the ones worth a hard stop. -->
