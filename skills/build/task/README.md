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
description, a GitHub issue, or the task notes; specs the unit as it is today;
writes the change on top of that baseline as `# +` / `# -` annotations; loops on
my review until the delta is right; implements and tests; runs `/comment-pass`
and `/structure`; stops for me to read the diff; then the PR and the journal
entry.

Two hard stops: after the HMML review, and before the PR.

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
| `plan` | Gate 0 prior-art/size, Gate 0b the repo's standards, brief, `hmml-extract` \| `hmml-new` for the baseline, then the `# +`/`# -` delta, then `hmml-review` | hard stop |
| `await-hmml-review` | nothing — I edit the baseline or the delta | `continue` or `revise` |
| `revise` | `hmml-review` again on the edited spec | back to `await-hmml-review` |
| `implement` | `hmml-translate`, run + test, `hmml-validate` | falls through |
| `comments` | `/comment-pass` over the branch diff | falls through |
| `standards` | `/structure` on the target repo — ratchet a budget the change improved, try to fix one it broke | falls through |
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
