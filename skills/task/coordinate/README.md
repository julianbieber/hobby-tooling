# Coordinate

| Field        | Value                                                              |
| ------------ | ------------------------------------------------------------------ |
| Type         | skill                                                              |
| Install path | `~/.claude-hobby/commands/task/coordinate.md` → `/task:coordinate` |
| Owner        | @julianbieber                                                      |
| Dependencies | `task-split`, `hwt`, `hcdev auto`, `hcoord` on PATH, `task:auto` in each spawned session |
| Status       | experimental                                                       |

## Description

The session in the right pane of `hcdev coordinator`. Given a description, it
carves the work into tasks with `/task-split`, records them in `tasks.json`,
and then works the list: one task at a time, each in its own `hcdev auto`
window, blocking on `hcoord wait` until the task stops for a person or
finishes. I hear from it once per task, when the application is ready to play.

## One session at a time is the design, not a limitation

Two `/task:auto` sessions on one repo would race on the default branch, on
`structure.hstruct` budgets, and on my attention — the play gate assumes I am
looking at one window. So the loop is strictly sequential, and a task that
depends on an unmerged predecessor is based on that predecessor's branch rather
than waiting for a merge. That produces a stack of PRs whose base is named in
the coordinator's note; merging in order is mine.

## `tasks.json` is the record and the display

Everything the coordinator knows is in one file in its notes directory, written
at every change. The `hcoord tui` pane re-reads it on every change, so there is
no channel between the two: the file is the channel. The same file is what
`hdash` counts for the coordinator's card, and what a resumed
`/task:coordinate` picks up from after an interrupt. Editing it by hand between
tasks is expected.

## Why the coordinator waits in a shell call

A Claude session has no way to sleep except through a tool. Polling a file from
prompts costs a prompt per poll; `hcoord wait` costs one shell call per ten
minutes and returns a JSON line the coordinator switches on. `--after <phase>`
is the second call on the same gate — once `await-play` has been reported, the
next wait must block until the phase is something else.

## What it deliberately does not do

- File GitHub issues: `/gh-issue-create` is an editing session.
- Merge, or push to the default branch.
- Run `/task-split` itself: that skill owns the shaping rules and their re-cuts.
- Answer a task's gate on my behalf: an `awaiting-human` task is reported and
  waited on, never continued from here.
