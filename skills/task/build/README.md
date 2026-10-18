# Build Task

| Field        | Value                                                                  |
| ------------ | ---------------------------------------------------------------------- |
| Type         | skill                                                                  |
| Install path | `~/.claude-hobby/commands/task/build.md` → `/task:build`               |
| Owner        | @julianbieber                                                          |
| Dependencies | `journal:resume`, `hmml-*`, `comment-pass`, `structure`, `subtask`, `journal:write` |
| Status       | experimental                                                           |
| Supersedes   | `dev-flow`                                                             |

## Description

The whole task as one pipeline with a recorded stage. Asks once whether this is
really one task; takes a brief from a description, the task's `issue.md`, or the
task notes; specs the unit as it is today; collects the business context only I have;
writes the change on top of that baseline as `# +` / `# -` annotations; loops on
my review until the delta is right; implements and tests; runs `/comment-pass`
and `/structure`; stops for me to read the diff; then the PR and the journal
entry.

Four hard stops: the subtask assessment, the business-context questions, the
HMML review, and the PR.

## The questions come before the code, not after it

A comment pass that quizzes the author line by line after the code is written
asks for a judgment about code they did not author, at a point where the only
honest answer is often "you tell me". So the questions moved to GATE C, between
the baseline and the delta: they are derived from the goal read against what the
code does today, before anything is designed.

The answers are recorded verbatim in `business-context.md` in the task notes,
and that file is then the only text — besides a fact verified in the code — that
any later phase may take a *why* from. It reaches the delta, then the plan, then
the code, and only last a comment. `/comment-pass` becomes a **matcher**: it
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

The gate refuses on a branch that already has commits or a dirty tree. That
refusal is what lets a subtask's PR target the default branch — `/subtask` cuts
from this branch, so at assessment time the two are the same thing and each
subtask is genuinely independent. Later, subtasks would inherit unmerged work.
Depth is bounded at one by `is_subtask` in the seeded state file rather than by
asking the model to remember.

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

**Nothing reviews the code but me.** `hmml-review` reviews the *spec* — the
`# +`/`# -` delta, before any code exists — `/comment-pass` enforces the comment
rule, and `/structure` counts the repo against the standards it already states.
That is the whole of the automated review in this pipeline. None of the three
forms an opinion about the change: two enforce a written rule and one counts.
There is no lens pass over the diff: reading the change is the job GATE 2 exists
for, and a machine critique printed alongside it would be something to wade
through on the way to the diff rather than a shortcut to it. (An earlier draft
carried one, transplanted from the work `dev-flow`, where `complete-review` fans
six review skills over the diff. That is a work-context habit and it does not
belong here.)

**The baseline comes before any thinking about the change.** `hmml-extract` runs
first and writes the unit as it is *today*, unannotated; only then is the delta
written on top of it as grammar §10 change annotations. That ordering is what
gives the delta something to be a delta *of* — a spec written straight to the
intended behaviour has no baseline to state the change against, and
`hmml-translate` is then guessing at what moved. The annotations stay in the file
after the code lands, so the spec goes on representing the change rather than
becoming indistinguishable from a fresh baseline.

## Stages

| phase | runs | ends |
|---|---|---|
| `assess-split` | precondition check, the cut tested against the shaping rules | hard stop |
| `handed-off` | `/subtask` per approved cut, seeded state, `journal:write` | terminal |
| `plan` | Gate 0 prior-art/size, Gate 0b the repo's standards, brief, `hmml-extract` \| `hmml-new` for the baseline, then the business-context questions | hard stop |
| `context` | records my answers verbatim, then the `# +`/`# -` delta, then `hmml-review` | hard stop |
| `await-hmml-review` | nothing — I edit the baseline or the delta | `continue` or `revise` |
| `revise` | `hmml-review` again on the edited spec | back to `await-hmml-review` |
| `implement` | `hmml-translate`, run + test, the repo's CI workflows run locally, `hmml-validate` | falls through |
| `comments` | `/comment-pass` over the branch diff, matched against the recorded context | falls through |
| `standards` | `/structure` on the target repo — ratchet a budget the change improved, try to fix one it broke | falls through |
| `review` | prints the diff, evidence and comments for me to read | hard stop |
| `await-review` | nothing — I read the summary | `continue` |
| `pr` | gathers the facts, opens the PR | falls through |
| `ci` | `hcoord ci` on the PR, repairing a red check on the diff's own for at most three rounds | falls through, or stops when blocked |
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
/task:build "make the spawner respect the wave budget"
/task:build #42
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

## TODO(jb-doc): why the pipeline stops exactly where it does

<!-- What it should cover: what makes those four points the ones worth a hard stop. -->
