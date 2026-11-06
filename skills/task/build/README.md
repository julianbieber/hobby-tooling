# Build Task

| Field        | Value                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| Type         | skill                                                                  |
| Install path | `~/.claude-hobby/commands/task/build.md` → `/task:build`               |
| Owner        | @julianbieber                                                          |
| Dependencies | `journal:resume`, `hmml-extract`, `comment-pass`, `structure`, `journal:write`; nushell `hwt subtask`, `hcdev tab`; `hcoord ci`; `gh` |
| Status       | experimental                                                           |
| Supersedes   | `dev-flow`                                                             |

## Description

The whole task as one pipeline with a recorded stage. Asks once whether this is
really one task; takes a brief from a description, the task's `issue.md`, or the
task notes; collects the business context only I have; writes a short plan and
works it as a todo list; implements and tests; reads the implemented change back
as an HMML delta derived from the diff; runs `/comment-pass` and `/structure`;
stops for me to read the diff, the delta and the proposed commit and PR
messages; then commits, pushes, opens the PR and writes the journal entry.

Three hard stops: the subtask assessment (GATE S), the business-context
questions (GATE C), and the review of the local changes (GATE 2).

## The delta is read after the code, not reviewed before it

The pipeline used to extract a baseline, annotate the intended change on top of
it, review that at GATE 1, and have `hmml-translate` implement the markers.
Reviewing that plan and then the real diff was a double review, and the plan
drifted from what was built — "reviewing the version that works seems more
productive than the double review from before". So only facts are settled
before code — the brief and the business context — and after the change is
implemented `/hmml-extract --against <merge-base>` writes the touched units as
the worktree leaves them, with what the diff changed marked `# -` / `# +`. The
HMML exists for me to understand what the code change is doing, so it
represents the implemented change; it is regenerated whenever the diff moves and
never edited.

## A plan file and a live todo list

With no spec review, implement works from a short `plan-<date>-<slug>.md` in the
notes, written right after GATE C, and its steps seed `todo` in
`build-task.json`. `todo` and `waiting_on` are the hobby half of work's `flow`
record: `hdash` shows them on the task's card, so where a session is can be read
without opening it. Work's aspects did not come over — a second PR here is a
subtask, decided at GATE S.

## The questions come before the code, not after it

A comment pass that quizzes the author line by line after the code is written
asks for a judgment about code they did not author, at a point where the only
honest answer is often "you tell me". So the questions moved to GATE C: they are
derived from the goal read against what the code does today, before anything is
designed.

The answers are recorded verbatim in `business-context.md` in the task notes,
and that file is then the only text — besides a fact verified in the code — that
any later phase may take a *why* from. It reaches the plan, then the code, and
only last a comment. `/comment-pass` becomes a **matcher**: it
writes a comment where a recorded passage answers the site, and reports the rest
as gaps rather than filling them in. A gap is closed by extending the context
(`/task:build context`), never by inventing the sentence.

That is the same three-source rule the commit-message policy in
`~/.claude-hobby/CLAUDE.md` already states — my prompts, what we settled, or a
verified fact — with the first two finally written down somewhere a later
session can read them.

## Subtasks and this pipeline are two different cuts

`assess-split` runs once, first, before any planning, and ends at a hard gate. A
**subtask** is a separate deliverable with its own worktree, branch, PR and
session, judged by the six Task shaping rules in `~/.claude-hobby/CLAUDE.md`.
This pipeline is one branch: everything it does lands as one change.

A subtask is cut from the default branch, so its PR holds only its own work.
Work this branch already holds reaches a subtask by `hwt subtask --take`: whole
commits are cherry-picked onto it and this branch is rebased onto the
subtask's, so the commits appear in exactly one PR and drop out of this
branch once it merges. A split therefore works on a branch that has commits;
uncommitted work has to be committed first, which the gate asks for rather
than does. Each subtask is worked now, as a tab of this session, or
postponed on its own card. Depth is bounded at one by `is_subtask` in the
state file `handed-off` seeds, rather than by asking the model to remember. A
subtask cut later with `/subtask` gets no state file: it is a barebones task
until someone writes its brief and runs `/task:build` there. Subtasks are
recorded once, in the parent's `subtasks.json`, which `hdash` reads.

Most tasks are one task, and the phase is expected to say so in a line.

## Where the standards fit

A repo's `structure.hstruct` states the code quality standards it holds itself to
and a budget for how far the code currently is from each. The pipeline touches it
at both ends, and the plan-time read is the valuable one: *"this adds a module
importing eight others and the standard says six"* is a design choice at `plan`
and rework at `review`.

At the `standards` phase the change has either improved a number — ratchet it, and
the tightened budget lands in the same diff that earned it — or moved away from
one, in which case the pipeline tries to fix it and reports a regression it
cannot.

**A budget is never raised by the pipeline.** Crossing one deliberately is a hand
edit, because it puts a line in the diff saying this project got further from its
own standard, and the PR body then has to say why. A machine widening a standard
so its own change fits is the failure the whole format exists to prevent.

A repo with no `structure.hstruct` skips both touchpoints silently. Most have
none, and a task is not the place to argue for one.

**Nothing reviews the code but me.** The derived delta is something for me to
read, `/comment-pass` enforces the comment rule, and `/structure` counts the
repo against the standards it already states. That is the whole of the
automation around the change, and none of it forms an opinion about it: one
describes, one enforces a written rule and one counts.
There is no lens pass over the diff: reading the change is the job GATE 2 exists
for, and a machine critique printed alongside it would be something to wade
through on the way to the diff rather than a shortcut to it. (An earlier draft
carried one, transplanted from the work `dev-flow`, where `complete-review` fans
six review skills over the diff. That is a work-context habit and it does not
belong here.)


## Stages

| phase | runs | ends |
|---|---|---|
| `assess-split` | precondition check, the cut tested against the shaping rules | hard stop |
| `handed-off` | `hwt subtask` per approved cut, seeded state, `journal:write` | terminal |
| `plan` | Gate 0 prior-art/size, Gate 0b the repo's standards, brief, reading the code, then the business-context questions | hard stop |
| `context` | records my answers verbatim, writes the plan file, seeds `todo` | falls through |
| `implement` | the change from the plan, run + test, the repo's CI workflows run locally | falls through |
| `delta` | `/hmml-extract --against <merge-base>` over the units the diff touches | falls through |
| `comments` | `/comment-pass` over the branch diff, matched against the recorded context | falls through |
| `standards` | `/structure` on the target repo — ratchet a budget the change improved, try to fix one it broke | falls through |
| `review` | prints the diff, evidence, comments and the proposed commit and PR messages for me to read | hard stop |
| `await-review` | nothing — I read the summary | `continue` |
| `pr` | commits the approved messages, pushes the task branch, opens the PR | falls through |
| `ci` | `hcoord ci` on the PR, repairing a red check on the diff's own for at most three rounds | falls through, or stops when blocked |
| `journal` | `journal:write` | `done` |

State lives in `build-task.json` in the task's notes dir, beside the plan and
the `.hmml` delta. An old record at `await-hmml-review` or `revise` resumes at
`implement`.

## Installation

```bash
just install-skills
```

Removing `skills/dev-flow/` is enough to retire `/dev-flow` — `install-skills`
prunes stale symlinks on the next run.

## Usage

```
/task:build "make the spawner respect the wave budget"
/task:build --issue
/task:build --notes
/task:build implement
/task:build context          # re-open the questions and rebuild off fuller answers
/task:build --no-split       # I already know this is one task
```

## Known Limitations

- Comments are checked by `/comment-pass`; commit messages and the PR body are
  written to the rules in `~/.claude-hobby/CLAUDE.md` but nothing verifies them.
- Inferring the starting phase from artifacts is a guess when a task was worked on
  outside the pipeline.

## It commits once the change has been read

The pipeline commits only after GATE 2, on the task branch, and never the notes
repo — `hwt sync` carries those. GATE 2 is where I review the local changes
before the PR opens, so it shows the proposed commits and PR alongside the
diff, and `continue` approves the messages along with the change. Several
commits in one PR are fine. Nothing goes to the default branch except through a
PR, unless I say otherwise in chat.
