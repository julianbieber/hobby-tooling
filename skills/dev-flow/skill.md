---
description: Run my default hobby development loop as a predictable, human-gated pipeline — read task, HMML-plan, self-review, (human gate), implement, self-review, test, (human gate) PR, journal
argument-hint: '[plan|implement|pr|journal] [--no-hmml] [--resume-deep]'
allowed-tools: Skill, Read, Write, Bash(pwd:*), Bash(ls:*), Bash(git branch:*), Bash(git -C * status:*), Bash(git -C * diff:*), Bash(git -C * log:*), Bash(git -C * rev-parse:*), Bash(git -C * ls-files:*), Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(gh pr list:*), Bash(gh pr view:*), Bash(gh issue view:*), Bash(gh issue list:*)
---

Run my **default development loop** as one predictable, human-gated pipeline. I
have the individual skills already (`journal:resume`, the HMML suite, `pr-facts`,
`journal:write`); this skill is the **thin orchestrator** that runs them in a fixed
order, stops hard at the two points where I take over, and can resume in a later
session exactly where it left off.

**You are glue, not a reimplementation.** Never re-do what a sub-skill owns (HMML
grammar, delta-vs-full-generation, PR body). Invoke the sub-skill and trust its
contract. Keep the sequential "spine" in *this* main-loop context so each step
reuses the last step's understanding — do **not** fan work out to subagents
except where a sub-skill already does (`journal:resume`).

## Two rules that override everything else

1. **Hard-stop at the two human gates** (after HMML self-review; before creating
   the PR). "Hard stop" means: finish the phase, write the state file, print a
   compact summary of what I need to do, and **end your turn**. Do not start the
   next phase, do not touch code, do not create the PR, until I explicitly tell
   you to continue.
2. **Never auto-commit** — not the source repo, not the notes repo (where the
   `.hmml` baseline lives). Committing the `.hmml` baseline and the code is *my*
   call, same contract as every sub-skill. You may run read-only git,
   `gh pr list`, and `gh issue view`.
3. **Never author text that goes public** — commit messages, PR bodies, issue
   bodies, comments. Those go through the `human-authored-text` gate, and every
   phase below that produces one hands off to it.

## Step 0 — Resolve task context and current phase

1. **Detect project/topic/task** from the worktree path (`pwd`). Format is
   `~/fun_worktrees/PROJECT/TOPIC/TASK`: the **first** dir is the project, the
   **second** the topic, the **third** the task ID (use it verbatim, blurb suffix
   and all). There is no repo level — the task dir is the worktree. Fall back to
   `git branch --show-current` (skip `master`/`main`/`develop`). This is the same
   parsing `journal:resume` and `pr-facts` use.
2. **State file** lives at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/dev-flow.json`:
   ```json
   { "phase": "plan", "hmml_unit": "<unit>", "project": "<project>",
     "no_hmml": false }
   ```
   `phase` is one of: `plan`, `await-hmml-review`, `implement`,
   `await-pr-review`, `journal`, `done`.
3. **Determine the starting phase**, in this precedence: a. An explicit phase
   arg (`plan|implement|pr|journal`) — forces it. Map `pr` → `await-pr-review`.
   b. Else the `phase` in `dev-flow.json` if the file exists. c. Else **infer**
   from artifacts and say what you inferred: no `.hmml` for this unit → `plan`;
   `.hmml` exists → `await-hmml-review`; the worktree has uncommitted code changes
   → `implement`; `gh pr list` shows an open PR for the branch → `done`.
4. **Flags:** `--no-hmml` sets `no_hmml: true` (skip steps 2–5 entirely — plan the
   change inline in a sentence or two, then go straight to `implement`).
   `--resume-deep` uses `journal:resume-deep` instead of `journal:resume` in step 1.
5. Announce the resolved `{project, topic, task, phase}` in one line, then run
   from that phase. Only run the phases from the current one onward — never redo
   an earlier phase.

Write `dev-flow.json` (create parent dirs if needed) at **every** phase
transition. Do not `git add`/`commit` it.

______________________________________________________________________

## Phase `plan` (steps 1–3) → GATE 1

**Step 0b — GATE 0: prior art and size.** Before any spec or code. Bevy AI policy
norms 4 and 5: no sweeping changes without prior discussion, and do not accelerate
past someone else's active work.

- Search for prior art on the repo this task targets:
  `gh pr list --search "<keywords>" --state all` and
  `gh issue list --search "<keywords>" --state all`. Report anything overlapping,
  with its state and its author.
- State an expected size: files to be touched, rough lines changed.
- Print both and **stop** if either of these holds: an open PR or an active issue
  covers this work, or the expected change exceeds **10 files or 500 lines**. Ask
  whether to proceed, narrow, or coordinate with the existing effort.

Record `size_budget` and `prior_art` in the state file. If the implementation
later crosses the stated budget, say so at Gate 2 rather than at the end.

On a repo that is mine alone, prior art is usually empty and this costs one
command. On an upstream repo it is the difference between a welcome PR and a
closed one.

**Step 1 — Read the task.** Invoke `journal:resume` (or `journal:resume-deep` if
`--resume-deep`) to load a focused digest of the notes for this task. If the task
carries a `gh-<number>` prefix, also run `gh issue view <number>` for the
acceptance criteria. Many hobby tasks have no issue at all — that is fine, the
notes and my description are then the whole brief. Summarize the goal + next step
in 2–4 lines; that framing carries through the rest of the pipeline.

**Step 2 — HMML plan.** Decide the entry from what the task is:

- **New behaviour** with no code yet → `hmml-new` (idea → skeleton `.hmml`).
- **Adjusting existing code** → `hmml-extract` (code → `.hmml`).

If it is genuinely unclear which, ask **one** question. The sub-skill writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml` and
stops. Record `hmml_unit` in the state file.

**Step 2b — Adjust the spec to the intended behaviour.** `hmml-extract` captures
the code's *current* behaviour — that is the starting point, not the plan. Edit
the `.hmml` so it expresses the behaviour **this task intends** (the delta you're
about to build): adjust the behaviour steps, data, and invariants toward the goal
from step 1. On the `hmml-new` path the first-pass draft already targets the
intended behaviour, so 2b is usually a no-op there. The spec now describes the
target and the code does not match it yet — which is exactly what step 3 reviews.

**Step 3 — Self-review the intended spec → implementation plan.** Run
`hmml-review` on the adjusted `.hmml`. It fans out parallel lens reviewers
(security, architecture, correctness, performance, maintainability) over the spec
and the current code, and synthesizes a prioritized **implementation plan**,
written to the task notes. Do **not** use `hmml-validate` here — the spec was just
edited *away* from the current code on purpose, so a conformance check would only
report the intended change as drift. `hmml-review` reviews the *intent* and plans
how to build it.

**→ GATE 1 (steps 4–5): HARD STOP.** Set `phase: await-hmml-review`, write the
state file, then print:

- the spec path,
- the `hmml-review` implementation plan (its path + an inline summary),
- a one-line instruction: *"Review the intended spec `<path>` and the plan;
  refine either, then commit the `.hmml` in the notes repo as the baseline and say
  continue. Re-edit as many times as you like (step 5 adjustments) — I'll wait."*

End your turn. Do nothing until I return.

______________________________________________________________________

## Phase `implement` (steps 6–8) → GATE 2

Entered when I say continue after GATE 1 (or directly via `implement` arg /
`--no-hmml`).

**Step 6 — Implement.** Unless `--no-hmml`, run `hmml-translate <unit>`: it diffs
the committed spec (in the notes repo) against my working-tree edit and applies
the **minimal** code change in the task worktree, compiles/tests, and stops. It
owns delta-vs-full-generation — do not second-guess it. For `--no-hmml`, make the
change directly in the worktree.

**Step 7 — Self-review.** The hobby account has **no review skills installed**
(`/complete-review` and the six review skills live only on the work account), so
do this inline rather than invoking a sub-skill. Read your own diff
(`git diff`) and review it across these lenses, in one pass, as the implementer:

- **Correctness** — does it do what the spec says? Off-by-one, boundary, and
  empty-input cases.
- **Design** — is this the right shape, or is it fighting the existing
  architecture?
- **Performance** — anything accidentally quadratic, allocating in a hot loop, or
  blocking a frame?
- **Clarity** — will I understand this in three months? Naming, dead code,
  leftover debugging.

Fix confirmed **critical/high** findings in this context; list medium/low for me
to decide. Do not blanket-apply — evaluate each. If I want the full parallel
review treatment, say so and I'll install the review skills.

**Step 8 — Test.** Drive the change end-to-end:

- `run` the app or the affected flow, plus the project's own test command
  (`cargo test`, `cargo clippy`, `just test`, whatever the repo uses).
- Then, unless `--no-hmml`, run `hmml-validate` once more: the code should now
  report **`CONFORMS`** to the committed spec. Report any residual `SPEC-AHEAD`
  as unfinished work.

**Step 8b — GATE 2.5: explain-back.** Run `explain-back`. It walks the branch diff
hunk by hunk and grades my explanation of each. Bevy AI policy norm 1 — *you must
personally understand every line you submit* — is the one rule no hook can check,
so it is checked here, before the PR exists.

Record the tally in the state file. Any hunk I could not explain is carried into
GATE 2 by name; it is not a PR blocker on its own, but I decide what happens to it
(read it properly, cut it, or rewrite it myself) before the PR opens.

**→ GATE 2 (step 9): HARD STOP.** Set `phase: await-pr-review`, write the state
file, then print:

- the worktree diff summary (files + a one-line-per-file what-changed),
- the step-7 review outcome (fixed vs deferred),
- the step-8 evidence (test names/output, validate verdict),
- the step-8b tally, and any hunk I could not explain, by name,
- the size against the Gate 0 budget, and by how much it was crossed,
- the instruction: *"Review the above. Say continue to open the draft PR, or tell
  me what to change."*

**Do not propose a PR title or body.** Norm 8 bans AI-drafted PR descriptions, and
a proposal at this gate is a draft I would then edit — which is what norm 6 asks
me not to do. The facts go to `pr-facts`; the words are mine.

End your turn. Do **not** run `pr-facts` yet.

______________________________________________________________________

## Phase `pr` (step 9, on my go)

Only after I say continue at GATE 2: run `pr-facts`. It gathers the evidence, runs
`ai-use-facts` for the disclosure record, and opens `gh pr create` with no title
or body so the `human-authored-text` gate hands me the buffer. I write the
description, including the mandatory `## AI use` section.
The `track-pr` hook records the PR to `prs.json`. If for any reason the hook did
not fire, append the PR to
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/prs.json` (create
with `[]` first; never store the PR's live state). Then move to `journal`.

Hobby repos are mine, so a PR is optional — for a small solo change, pushing to
the default branch may be the right call. Ask if it is not obvious, and skip
straight to `journal` when I say no PR.

______________________________________________________________________

## Phase `journal` (step 10)

Run `journal:write` to log progress for this task (goal, what changed, evidence, open
items). Set `phase: done` in the state file. Print a one-line wrap-up: task, PR
link (if any), and anything still open. End.

______________________________________________________________________

## Notes on scope

- If a phase's sub-skill asks its own clarifying question, surface it to me —
  don't answer on my behalf.
- If artifacts and the state file disagree (e.g. state says `implement` but there
  is no committed spec), trust the artifacts, say so, and correct the state file.
- This skill only orchestrates. If I ask for something off-pipeline mid-run,
  handle it, then re-anchor on the current `phase`.
