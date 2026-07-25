---
description: Review an adjusted/new .mml spec through parallel engineering lenses (security, architecture, correctness, performance, maintainability) and synthesize an implementation plan for the intended behaviour
argument-hint: '[<unit> | path/to/spec.mml] [--min-priority critical|high|medium|low]'
---

# MML Spec Review → Implementation Plan

Review a `.mml` spec that expresses the **intended behaviour** of a task — a
freshly adjusted spec (extracted from code, then edited toward the task's goal)
or a new-feature spec — and turn it into a **concrete implementation plan**.

This is **not** `mml-validate`. `mml-validate` checks whether *existing code*
conforms to a spec; that is the wrong question here, because the spec was just
deliberately edited *away* from the current code to describe the change we want.
Here the spec **is** the intent, the code does **not** yet match it, and the job
is to pressure-test that intent from multiple engineering perspectives and
produce the plan for making the code match it.

Your perspective is the **orchestrator / tech lead**: you spawn independent
specialist reviewers of the spec, then critically synthesize their input into
one plan. Not every concern is worth acting on — evaluate each before it lands
in the plan.

## Step 1 — Resolve the spec and its code

1. **Locate the `.mml`.** From `$ARGUMENTS`: a path resolves directly; a bare
   `<unit>` resolves to `<unit>.mml` in the current task's spec directory
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/` (topic =
   first dir under `~/fun_worktrees`, task = second, from the worktree path — the
   same detection `resume-journal` uses). With no argument, if the task
   dir holds exactly one `.mml`, use it; if several, list them and ask which.
2. **Read the spec** in full. Note its four sections (data, behaviour,
   invariants, and the `source:` block). Run `mml-parser <path> --lint` first
   (if installed): a parse error means the spec can't be reviewed — report and
   stop; an `unresolved-source-path` warning means the `source:` linkage is
   broken — flag it in the plan so it gets fixed alongside the change.
3. **Locate the code** the change lands in: the `source.repo` value maps to the
   task's worktree checkout `~/fun_worktrees/{project}/{topic}/{task}` (fall back to
   `~/fun_repos/{repo}`); `source.paths` are the entry points. Read enough of
   that code to ground the review in what actually exists today.
4. **Parse `--min-priority`** (default `low` = all). Strip it from the argument.

## Step 2 — Fan out parallel lens reviewers

Spawn **one isolated subagent per lens, in parallel** (single message, multiple
Task/Agent calls). Give each the spec text, the `source` block, and the paths to
the current code. Each reviewer answers, **for the intended behaviour in this
spec**: what must the implementation get right, and what will bite us?

Lenses:

- **security** — trust boundaries the behaviour crosses, input/authz/secret
  handling implied by the spec, data exposure, injection/SSRF/deserialization
  surfaces the new behaviour introduces.
- **architecture** — fit with the existing system at `source.paths`: does the
  intended behaviour belong here, what collaborators/contracts/interfaces does
  it touch, migration/rollout/coupling concerns, simpler placements.
- **correctness** (QA lens) — do the behaviour steps + invariants actually hang
  together and cover the goal? Missing branches, edge cases, error/empty/
  concurrent paths, invariants the steps can violate, untestable assertions.
- **performance** — hot paths, N+1 / unbounded work / allocation implied by the
  behaviour, data volume at the `source` site, caching/batching the plan should
  bake in.
- **maintainability** — clarity of the resulting code, naming, duplication vs
  reuse of what already exists at `source.paths`, testability of the behaviour.

Each subagent returns a compact list of concerns, each with: a one-line
statement, a priority (critical/high/medium/low), and — crucially — **what the
implementation plan should do about it**.

## Step 3 — Synthesize the implementation plan

Collect the lens outputs. **Critically filter**: drop concerns that don't apply
to this change, merge duplicates, and keep those at or above `--min-priority`.
Then write an implementation plan with these sections:

- **Goal** — the intended behaviour in 2–3 lines (from the spec).
- **Approach** — the recommended strategy, reconciling the lenses.
- **Steps** — ordered, each naming the file(s) from `source.paths` it touches
  and the concern(s) it addresses.
- **Risks & edge cases** — the surviving critical/high concerns and how the plan
  handles them.
- **Open questions** — anything the spec leaves genuinely undecided.

Write the plan to the task notes following the CLAUDE.md plan convention:
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/plan-{YYYY-MM-DD}-{unit}.md`
(get the date via `date +%F`; use the zk front matter — `title`, `created`,
`tags: [plan]`). Do **not** `git add`/`commit` the notes repo. Then echo the
plan inline so the caller (and `dev-flow`'s GATE 1) can act on it.

## Notes

- This skill **only reviews and plans** — it never edits the spec or the code.
- Ground every concern in the spec or the real code; drop speculation. A concern
  with no bearing on this change does not belong in the plan.
- If invoked by `/julian:dev-flow` at step 3, the plan you produce is what the
  human reviews at GATE 1 alongside the `.mml`.
