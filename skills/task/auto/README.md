# Auto Task

| Field        | Value                                                          |
| ------------ | -------------------------------------------------------------- |
| Type         | skill                                                          |
| Install path | `~/.claude-hobby/commands/task/auto.md` → `/task:auto`         |
| Owner        | @julianbieber                                                  |
| Dependencies | `hauto` (the stage driver in `bin/`), `comment-pass`, `structure`, `journal:write` |
| Status       | experimental                                                   |
| Derived from | `task:build`                                                   |

## Description

A task in two processes that never share a context. The **plan** stage runs
headless, reads the code once, and writes `plan.md`; the **build** stage starts
fresh and implements from the plan alone. How it ends depends on whether anyone
is there: unattended (`auto: true`, which only a coordinator sets) it runs the
acceptance observation as commands and opens the PR when they pass; interactive
it stops with the application running for me. **Acceptance** is asked at the
start of either, and only when the brief does not already say what done looks
like.

`hauto` — what the `hobby-auto` layout runs in its Claude pane — starts each
stage from the phase in `build-task.json`. It is the pipeline
`/task:coordinate` runs in each task window, and it also works typed by hand
in a worktree: `hcdev auto <task>`.

## Two processes, because context is the cost

The first version ran the whole pipeline in one session: HMML baseline, delta,
a multi-lens spec review, then the implementation on top of all of that. It
used a session's budget on one task. Two things in that design cost the most
and returned the least for work where the UX matters and the implementation
does not: the spec review loop, and carrying the planning conversation into
the implementation.

So there is no HMML here, and the plan stage's process exits once `plan.md` is
written. The build stage reads the plan and the brief and nothing else — no
journal resume, no directory listings, no neighbouring files — and every build
and test command it runs is truncated before it reaches the conversation. The
build stage also runs on a cheaper model by default (`model` in the state
file, `sonnet` when a coordinator seeded it), because a plan that names the
files and the steps leaves it mostly mechanical work.

## Why the gates moved to the ends

`/task:build` stops four times because each stop is a place I read something:
the split, the questions, the spec delta, the diff. For work where I care about
what the change *does* and not how it is built, three of those reads are cost
without return. What I still want is to say what done means before anything is
designed, and to try the result before it becomes a PR. So the two gates that
survive are exactly those, and the middle two fold into them: GATE C's
questions are asked at the acceptance gate if they are asked at all, and GATE 1's
review plan is implemented rather than shown. The second of the two survives
only where I am there to use it — see below.

The play gate is a deliberately different kind of stop from GATE 2. GATE 2 hands
me a diff and evidence; GATE P hands me a **running application** and the one
observation the task promised. Feedback there is recorded verbatim and becomes
another round of the pipeline, not a conversation about the code.

## Why an unattended run has no play gate

A gate is worth having because a person reads what it prints. In a run a
coordinator started there is no such person, and the report is not merely
unread but unreadable: the `hcdev auto` window is 800x600 and off-screen, and
the Claude pane inside it is about seventeen columns wide. Answering `continue`
there is answering having read nothing, which is what a coordinator does — and
a task has shipped that way with four of its six scenarios failing.

So with `auto: true` the report is replaced by **commands**. The plan stage
writes the acceptance observation as checks; the `verify` phase runs them; a
failure is the session's own to fix, like a failing test, for at most three
rounds. The difference that matters is not who looks — it is that a failing
command stops the pipeline by itself and a printed report does not.

Three rounds rather than none or unlimited: a repair loop nobody can interrupt
is the same failure this replaced, wearing different clothes. What the rounds
cost is recorded, so a run that keeps needing all three is visible.

Unattended runs keep the *acceptance* gate, because a task whose done-when
cannot be written as commands is not one to guess at. The plan stage asks there
— at `await-acceptance`, the stop that already exists — rather than inventing a
proxy check, since a check that passes while the observation fails is worse
than no check at all.

## What it commits, and why that is different from `/task:build`

`/task:build` never commits; every sub-skill has that contract. This pipeline
does, once, after the play gate — or after the acceptance checks pass, when
there is no play gate — because a PR cannot open without commits and nobody is
at the keyboard between that point and the PR. The message rule is
unchanged: terse, a record of decisions, justified only from the brief, the
acceptance answer, the recorded context and verified facts. The notes repo is
still never committed here.

## The state file is what the coordinator watches

`build-task.json` is written at every transition, as in `/task:build`, and the
waiting phases are named `await-acceptance`, `await-play` (interactive) and
`await-fix` (unattended, when the acceptance checks would not pass). `hcoord
wait` returns on any `await-*` phase and on `done`, which is how
`/task:coordinate` knows when a task wants a person and when it is finished
without either session talking to the other. A coordinator seeds the file before
the session starts — `brief`, `acceptance`, `auto`, `coordinator` — and that
seed is the brief.

An unattended run also writes `acceptance_result`: every check with its command,
exit status and verdict. That is deliberately **data, not prose**, and it is
what `hcoord wait` puts on its line and `hcoord tui` puts under the task. The
whole point is that the coordinator sees what was checked without reading the
pane it cannot read.

Every write also carries `written` (when this write happened) and `phase_since`
(when the current phase began). The two answer different questions: `written`
says the pipeline is still alive even while it sits in one phase for a long
task, and `phase_since` is what turns "still at `implement`" into "unchanged
for 1h 12m" instead of a guess made by sampling `/proc` or reading a pane that
cannot be read.
