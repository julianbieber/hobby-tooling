# Coordinate

| Field        | Value                                                              |
| ------------ | ------------------------------------------------------------------ |
| Type         | skill                                                              |
| Install path | `~/.claude-hobby/commands/task/coordinate.md` → `/task:coordinate` |
| Owner        | @julianbieber                                                      |
| Dependencies | `task-split`, `hcoord` and `gh` on PATH, the `hwt` / `hcdev` nushell modules, `task:auto` in each spawned session |
| Status       | experimental                                                       |

## Description

The session in the right pane of `hcdev coordinator`. Given a description, it
carves the work into tasks with `/task-split`, records them in `tasks.json`,
and then works the list: one task at a time, each in its own `hcdev auto`
window, blocking on `hcoord wait` until the task stops for a person or
finishes — which, for an unattended task, is after it has merged its own PR. I
hear from it only when a task needs something only I can give. A task it started is
unattended (`auto: true`), so it runs its own acceptance rather than stopping to
show me the application; it interrupts me only when it cannot write acceptance
criteria, or when its own checks would not pass.

## One session at a time is the design, not a limitation

Two `/task:auto` sessions on one repo would race on the default branch and on
`structure.hstruct` budgets, and the two gates that are left both assume one
window has my attention. So the loop is strictly sequential.

## A task is not done until its PR is merged

The list used to move on the moment a task's session ended, and a task whose
dependency was still open was based on that dependency's branch. That produced
a stack: each PR's base was a branch rather than the default one, the whole
stack had to be merged in order, and a review that changed anything early
rebased everything behind it.

So `done` now means **merged**, with a status of its own — `awaiting-merge` —
between the session ending and the PR landing. The consequences are all
simplifications:

- **There is no stacked base.** Every task branches off the default branch,
  because everything before it is already in that branch. `W2` is a
  fast-forward and nothing else, and the two-open-dependencies question that
  had no good answer is gone with it.
- **The cost is real and is the point.** The list now stalls on me. That is the
  trade the whole skill makes: unattended until it needs a person, and what it
  needs is the thing only I can give.

## Who merges, and what stops the list

Each task's **own session** merges its own PR, once its checks are green and
its acceptance passed, and there is no other option (`## one-merge-option` in
`notes/hobby-tooling/skills/coord-contract/business-context.md`: *"there should only be 1 option,
the task session should merge itself on green cicd and reached acceptence
criteria."*). Every seed says `merge: coordinator` in the task's
`build-task.json`; the session watches its checks itself, merges, marks its
`task.json` done (`## task-done-owner`: *"in auto mode, the task session
itself on merge"*), and reports `done` with the merge already landed. The
coordinator's W4 reads that off the `hcoord wait` line and moves on, making no
merge call of its own. W5 survives only for a session that finished without
merging, and there it waits for a person, never merges.

**Green is decided by `gh`, not by the coordinator.** `hcoord merged` reads
`statusCheckRollup` and reports `green`, `pending`, `failing` or `none` — and
`none`, a repository that runs no checks at all, counts as green, because most
hobby repos have no CI and the alternative is a list that never moves.

A decision the session cannot make — a red check it may not repair, a
conflict, an acceptance that would not pass — is `await-fix`. `hcoord wait`
puts its `decision` on the line, the card reads `requires decision: <…>`, and
the list **holds** on that task: I answer in its window, the session carries
on to a merged PR, and the list moves (`## leave-await-fix`). A block no
session holds — a PR closed unmerged — stops the loop, and a resumed
coordinator re-checks every `blocked` entry, so a PR I merged by hand releases
what depends on it. The tasks behind a block wait either way, since they would
branch off work that has not landed.

## The coordinator knows nothing about any task

It never reads a task's code, plan, diff or journal. Planning and building
happen in the task's own window — `hauto` runs a headless plan process, then
a fresh build session — and what reaches the coordinator is a phase name from
`build-task.json` and a PR URL from `prs.json`. That is what lets one
coordinator session work a whole list: its context holds the list, not the
tasks.

## `tasks.json` is the record and the display

Everything the coordinator knows is in one file in its notes directory, written
at every change. The `hcoord tui` pane re-reads it on every change, so there is
no channel between the two: the file is the channel. The same file is what
`hdash` counts for the coordinator's card, and what a resumed
`/task:coordinate` picks up from after an interrupt. Editing it by hand between
tasks is expected.

## Why the coordinator waits on an idle notice, and falls back to a shell call

A Claude session has no way to sleep except through a tool. The cheapest one
is a cross-session idle subscription: the task's window is a peer session, and
`SendMessage` with `notify_when_idle` and no message delivers one notice when
that session ends a turn — which is a gate or `done` — at no cost to either
side. The coordinator never sends the session text: a message from it would
arrive as input to a pipeline that must not take instructions from anyone but
me, and a slash command in it would arrive as plain text anyway.

When the peer is not listed yet, or after a resume, `hcoord wait` blocks for up
to ten minutes per shell call and returns a JSON line the coordinator switches
on. `--after <phase>` is the second call on the same gate — once `await-play`
has been reported, the next wait must block until the phase is something else.

That line is also where a task's acceptance arrives. An unattended task runs the
observation itself and records every check; `hcoord wait` puts `{passed, checks,
failed}` on the line, each failure naming its command and exit status. This is
what replaces looking at the window, and it is why the coordinator can judge a
task it has read no code for. A task that could not make its checks pass reaches
`await-fix`, with or without a PR, and is held as a decision like any other.

## What it deliberately does not do

- Write issues: `/issue-create` is an editing session.
- Push to the default branch, or merge any PR.
- Answer a task's gate on my behalf: an `awaiting-human` task is reported and
  waited on, never continued from here.
