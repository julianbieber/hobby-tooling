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

HMML comes back only **on demand**: `--delta` (or `delta` at the play gate)
runs `/hmml-extract --against <merge-base>` after the implementation, so the
change can be read as a delta to the mental model. It is derived from the diff
after the code exists, which is the same reason `/task:build` no longer designs
one up front; it stays off by default because this pipeline exists to spend as
little context as a task allows.

The plan's `Steps` also seed `todo` in `build-task.json`, which `hdash` shows on
the task's card beside `waiting_on`, so an unattended window can be read
without being looked at.

## Why the gates moved to the ends

`/task:build` stops three times because each stop is a place I read something:
the split, the questions, the diff. For work where I care about what the change
*does* and not how it is built, two of those reads are cost without return.
What I still want is to say what done means before anything is designed, and to
try the result before it becomes a PR. So the two gates that survive are exactly
those: GATE C's questions are asked at the acceptance gate if they are asked at
all, and the diff gives way to the running application. The second of the two survives
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

## Why a hand scenario is captured rather than checked

A goal marks a scenario `hand` when nobody can script it. The two tempting
responses are both the failure this pipeline exists to avoid: skip it, so nothing
ever looks, or write a command that passes while the observation was never made,
which is worse because nothing downstream looks again — the run that shipped four
of six scenarios failing is the evidence for how much a green report is worth.

So the plan stage records a `hand` check with no command, and `verify` captures the
screen the scenario names to `screens/<id>.png` in the task's notes directory. The
capture turns "nobody looked" into an artefact a later pass can look at, and
`hcoord wait` and `hcoord tui` count the hand checks apart from the passes so a
coordinator sees what nobody ran. A hand check never blocks the PR: the scripted
checks still have to pass, and a failed capture is a `note` on the check, because
the scenario already said there was nothing to run.

The mechanism is `hcoord capture`, not a paragraph of this prompt. A capture spelled
in a prompt is re-derived on every run, and its details are exactly what a prompt
gets wrong silently: only a window the X server has mapped captures, an ambiguous
window name has to be refused rather than guessed at, and ImageMagick's policy
blocks `label:@file` outright.

## What it commits, and why that is different from `/task:build`

`/task:build` commits only after GATE 2, where I have read the local changes and
the proposed messages; no sub-skill commits. This pipeline commits once, after
the play gate — or after the acceptance checks pass, when
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
without either session talking to the other. `hcoord wait` also returns on a
`play` phase that has gone untouched, so the gate is still seen when the
write to `await-play` is the thing that went missing. A coordinator seeds the
file before the session starts — `brief`, `acceptance`, `auto`, `coordinator`
— and that seed is the brief.

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

## The session merges its own PR, when it was handed the merges

`merge` in the seeded state file — `mine` or `coordinator` — is how a task
session learns whether merging its own green PR is its job. **Absent means
`mine`**, so a `hcdev auto` typed by hand, which seeds no such field, never
merges: only a coordinator writes `coordinator` into the seed, from its own
`tasks.json`.

With `coordinator`, the `merge` phase removes four round trips a coordinator
used to make per task — `hcoord merged --ready`, `gh pr merge`, `hcoord
merged` again to confirm, `git pull --ff-only` in the main checkout — by
making the session that already has the PR open watch its own checks, merge,
and confirm the merge landed before it reports `done`.

A merge that cannot be made reuses `await-fix` rather than a new phase. The
reason is what an un-updated coordinator does with the name: `hcoord wait`
already stops a list on `await-fix`, so reusing it means a coordinator that
has not been taught about `merge_result` still stops correctly, where a phase
name it does not recognise would instead hold the list waiting on a gate that
never resolves. `merge_result.outcome` is what tells the two failure kinds
apart once the coordinator does know to look.

A merge conflict is always `blocked`, never resolved by rebasing here.
`business-context.md`'s `## merge-conflicts` records why: a conflict means
something landed on the default branch outside this run, which is exactly the
one-task-at-a-time assumption the whole coordinator loop depends on — the
session escalates it rather than guessing at a resolution unattended.

## A PR is handed over green, whoever merges it

"Ensure green CI" was the whole request, and it is enforced at both ends of the
push. **Before**: the `pr` phase runs the repo's own `.github/workflows` steps
locally and repairs what fails, so the first push is already the one CI is
expected to pass. **After**: the PR's checks are watched and repaired through
`fix-ci` in both merge modes — with `mine` too, which used to skip straight to
`journal` and hand over a PR whose checks nobody had looked at.

The local run is a filter, not a substitute. `uses:` steps, secrets, service
containers and other operating systems cannot be reproduced on this machine, so
the PR's checks remain the authority and the local run only catches what can be
caught before a round trip to the runner. Both share `ci_result.rounds` and its
bound of three: they are two places the same CI can fail, not two loops.

