# Auto Task

| Field        | Value                                                          |
| ------------ | -------------------------------------------------------------- |
| Type         | skill                                                          |
| Install path | `~/.claude-hobby/commands/task/auto.md` → `/task:auto`         |
| Owner        | @julianbieber                                                  |
| Dependencies | `task:build` (read for the phase mechanics), the `hmml-*` suite, `comment-pass`, `structure`, `journal:write` |
| Status       | experimental                                                   |
| Derived from | `task:build`                                                   |

## Description

`/task:build` with its four gates collapsed to two. **Acceptance** at the start,
asked only when the brief does not already say what done looks like; **play**
at the end, where the application is launched for me and the PR waits for my
`continue`. Everything between — baseline, delta, spec review, implementation,
tests, comment pass, standards — runs without stopping.

It is the pipeline `/task:coordinate` runs in each task window, and it also
works typed by hand in a worktree.

## Why the gates moved to the ends

`/task:build` stops four times because each stop is a place I read something:
the split, the questions, the spec delta, the diff. For work where I care about
what the change *does* and not how it is built, three of those reads are cost
without return. What I still want is to say what done means before anything is
designed, and to try the result before it becomes a PR. So the two gates that
survive are exactly those, and the middle two fold into them: GATE C's
questions are asked at the acceptance gate if they are asked at all, and GATE 1's
review plan is implemented rather than shown.

The play gate is a deliberately different kind of stop from GATE 2. GATE 2 hands
me a diff and evidence; GATE P hands me a **running application** and the one
observation the task promised. Feedback there is recorded verbatim and becomes
another round of the pipeline, not a conversation about the code.

## What it commits, and why that is different from `/task:build`

`/task:build` never commits; every sub-skill has that contract. This pipeline
does, once, after the play gate — because a PR cannot open without commits and
nobody is at the keyboard between `continue` and the PR. The message rule is
unchanged: terse, a record of decisions, justified only from the brief, the
acceptance answer, the recorded context and verified facts. The notes repo is
still never committed here.

## The state file is what the coordinator watches

`build-task.json` is written at every transition, as in `/task:build`, and the
two waiting phases are named `await-acceptance` and `await-play`. `hcoord wait`
returns on any `await-*` phase and on `done`, which is how `/task:coordinate`
knows when a task wants a person and when it is finished without either session
talking to the other. A coordinator seeds the file before the session starts —
`brief`, `acceptance`, `auto`, `coordinator` — and that seed is the brief.
