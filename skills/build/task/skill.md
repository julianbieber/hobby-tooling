---
description: Run a task end to end as one human-gated pipeline with a recorded stage — brief, HMML plan, review loop, implement and test, explain-back, comment review, my review, PR, journal
argument-hint: '[<description> | #<issue> | --notes] [<phase>] [--no-hmml] [--resume-deep]'
allowed-tools: Skill, Read, Write, Edit, Bash(pwd:*), Bash(ls:*), Bash(git branch:*), Bash(git -C * status:*), Bash(git -C * diff:*), Bash(git -C * log:*), Bash(git -C * rev-parse:*), Bash(git -C * ls-files:*), Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(*/ai-policy-tier.sh:*), Bash(gh pr list:*), Bash(gh pr view:*), Bash(gh issue view:*), Bash(gh issue list:*)
---

Run a task **end to end** as one predictable, human-gated pipeline. The individual
skills already exist (`journal:resume`, the HMML suite, `explain-back`,
`comment-review`, `pr-facts`, `journal:write`); this is the **thin orchestrator**
that runs them in a fixed order, stops hard where I take over, and resumes in a
later session exactly where it left off.

Supersedes `/dev-flow`.

**You are glue, not a reimplementation.** Never re-do what a sub-skill owns (HMML
grammar, delta-vs-full-generation, comment judgement, PR body). Invoke it and
trust its contract. Keep the sequential spine in *this* main-loop context so each
phase reuses the last one's understanding — do not fan work out to subagents
except where a sub-skill already does (`journal:resume`).

## Three rules that override everything else

1. **Hard-stop at the human gates** — after the HMML self-review, and after the
   comment review. "Hard stop" means: finish the phase, write the state file,
   print a compact summary of what I need to do, and **end your turn**. Do not
   start the next phase until I say so.
2. **Never auto-commit** — not the source repo, not the notes repo where the
   `.hmml` baseline lives. That is my call, same contract as every sub-skill.
   Read-only git, `gh pr list` and `gh issue view` are fine.
3. **Never author text that goes public** — commit messages, PR bodies, issue
   bodies, comments. Those go through the `human-authored-text` gate, and every
   phase that produces one hands off to it.

## Step 0 — Resolve context, brief, tier and phase

1. **Task context** from the worktree path (`pwd`), which is
   `~/fun_worktrees/PROJECT/TOPIC/TASK`: first dir project, second topic, third
   the task ID, verbatim, blurb suffix and all. There is no repo level — the task
   dir is the worktree. Fall back to `git branch --show-current`, skipping
   `master`/`main`/`develop`.

2. **The brief**, from the first argument:

   | argument | brief |
   |---|---|
   | free text | the text itself, recorded verbatim |
   | `#<n>` or `gh-<n>` | `gh issue view <n>` — title, body, acceptance criteria |
   | `--notes`, or nothing | `journal:resume` over the task's notes dir |

   With no argument and a task named `gh-<n>-…`, do both: `journal:resume` **and**
   the issue. Record `brief_source` and the brief itself in the state file, and
   summarize the goal and next step in 2–4 lines. That framing carries through
   every later phase.

3. **The tier**, once, from `scripts/ai-policy-tier.sh` in the hobby-tooling
   checkout. Record it. Every later phase reads it from the state file rather than
   re-resolving, so one answer governs the whole task.

4. **State file** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/build-task.json`:

   ```json
   { "phase": "plan", "brief_source": "issue|description|notes",
     "brief": "<the brief>", "hmml_unit": "<unit>", "tier": "private|upstream",
     "no_hmml": false, "size_budget": "", "prior_art": "",
     "rounds": 0, "explain_back": {}, "comments": {} }
   ```

   A leftover `dev-flow.json` in the same directory is read once for its `phase`
   and `hmml_unit`, then ignored — never written to again.

5. **Starting phase**, in this precedence: an explicit phase argument forces it;
   else `phase` from the state file; else **infer** from artifacts and say what
   you inferred — no `.hmml` → `plan`; a `.hmml` exists → `await-hmml-review`;
   uncommitted code changes → `implement`; an open PR for the branch → `done`.

6. **Flags:** `--no-hmml` skips the spec phases entirely (plan the change inline in
   a sentence or two, then go to `implement`). `--resume-deep` uses
   `journal:resume-deep`.

Announce the resolved `{project, topic, task, tier, brief_source, phase}` in one
line, then run from that phase onward. Never redo an earlier phase. Write the
state file at **every** transition; do not `git add`/`commit` it.

______________________________________________________________________

## Phase `plan` → GATE 1

**Gate 0 — prior art and size.** Before any spec or code. Norms 4 and 5: no
sweeping changes without prior discussion, and do not accelerate past someone
else's active work.

- `gh pr list --search "<keywords>" --state all` and
  `gh issue list --search "<keywords>" --state all` on the repo this task targets.
  Report anything overlapping, with its state and author.
- State an expected size: files touched, rough lines changed.
- **Stop and ask** if an open PR or active issue covers this work, or the expected
  change exceeds **10 files or 500 lines**.

Record `size_budget` and `prior_art`. On a `private` repo this is usually one
empty search; on `upstream` it is the difference between a welcome PR and a closed
one.

**The HMML plan.** Pick the entry from what the task is:

- new behaviour, no code yet → `hmml-new`
- adjusting existing code → `hmml-extract`

If genuinely unclear, ask **one** question. The sub-skill writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml` and
stops. Record `hmml_unit`.

**Adjust the spec to the intended behaviour.** `hmml-extract` captures what the
code does *now* — the starting point, not the plan. Edit the `.hmml` until it
describes what this task intends. On the `hmml-new` path this is usually a no-op.
The spec now describes the target and the code does not match it; that is exactly
what the review is for.

**Self-review the spec.** Run `hmml-review`: parallel lens reviewers over the spec
and the current code, synthesized into a prioritized implementation plan in the
task notes. Not `hmml-validate` — the spec was just moved away from the code on
purpose, so a conformance check would report the intended change as drift.

**→ GATE 1: HARD STOP.** Set `phase: await-hmml-review`, write the state file,
print the spec path, the implementation plan (path plus an inline summary), and:
*"Review the spec and the plan. Refine either, then say `continue` — or say
`revise` and I'll re-review what you changed."* End your turn.

## Phase `await-hmml-review` and `revise` — the loop

- `revise` re-runs `hmml-review` on the edited spec, increments `rounds`, returns
  to `await-hmml-review`, and stops again. No cap: this loops until I am happy.
- Each round reports what changed in the spec since the previous one, so I can see
  whether my edit did what I meant.
- `continue` moves to `implement`. Committing the `.hmml` as the baseline is mine
  to do; if it is still uncommitted when I say continue, say so once and proceed.

______________________________________________________________________

## Phase `implement`

**Implement.** Unless `--no-hmml`, run `hmml-translate <unit>`: it diffs the
committed spec against my working-tree edit and applies the **minimal** code
change, compiles, tests, and stops. It owns delta-vs-full-generation — do not
second-guess it. With `--no-hmml`, make the change directly.

Write code under the tier from step 0. On `private` that means doc comments on
public items and nothing else; on `upstream`, placeholders only. Do not write
comments you expect the later phases to delete — the pipeline is a check, not a
laundry.

**Self-review.** The hobby account has no review skills installed, so do this
inline over `git diff`, in one pass, as the implementer: correctness (boundaries,
empty input), design (right shape, or fighting the architecture), performance
(quadratic, allocating in a hot loop, blocking a frame), clarity (naming, dead
code, leftover debugging). Fix confirmed critical/high findings; list medium/low
for me. Evaluate each — do not blanket-apply.

**Test.** Drive it end to end: run the app or the affected flow, plus the repo's
own test command (`cargo test`, `cargo clippy`, `just test`, whatever it uses).
Then, unless `--no-hmml`, `hmml-validate` once more — the code should report
**`CONFORMS`**. Report residual `SPEC-AHEAD` as unfinished work.

Record the evidence, then fall through to `explain-back` without stopping.

## Phase `explain-back`

Run `/explain-back`. The hunk-by-hunk quiz is norm 1 — *you must personally
understand every line you submit* — checked before the PR exists, and its comment
pass then corrects the comments in the diff against the code.

Record the tally and the comment-pass list. Any hunk I could not explain is
carried by name into the human gate; it is not a blocker on its own, but I decide
what happens to it. Fall through to `comment-review`.

## Phase `comment-review` → GATE 2

Run `/comment-review`. It judges every comment clause by clause and cuts what is
not required. Record what it changed.

**→ GATE 2: HARD STOP.** Set `phase: await-review`, write the state file, print:

- the diff summary — files, one line each on what changed;
- the self-review outcome: fixed vs deferred;
- the test evidence and the validate verdict;
- the explain-back tally, and every hunk I could not explain, by name;
- the surviving comments in full, and what was cut;
- the size against the Gate 0 budget, and by how much it was crossed;
- *"Review the above. Say continue to open the PR, or tell me what to change."*

**Do not propose a PR title or body.** Norm 8 bans AI-drafted PR descriptions, and
a proposal here is a draft I would then edit — which is what norm 6 asks me not to
do. Facts go to `pr-facts`; the words are mine. End your turn.

______________________________________________________________________

## Phase `pr` — on my go only

Run `pr-facts`. It gathers the evidence, runs `ai-use-facts` for the disclosure
record, and opens the create command with no title or body so the
`human-authored-text` gate hands me the buffer. I write the description, including
the mandatory `## AI use` section.

The `track-pr` hook records the PR to `prs.json`. If it did not fire, append the
PR there yourself (create with `[]` first; never store the PR's live state).

Hobby repos are mine, so a PR is optional — for a small solo change, pushing to
the default branch may be right. Ask if it is not obvious, and skip to `journal`
when I say no PR.

## Phase `journal`

Run `journal:write` — goal, what changed, evidence, open items. Set `phase: done`.
Print a one-line wrap-up: task, PR link if any, anything still open. End.

______________________________________________________________________

## Notes on scope

- If a sub-skill asks its own clarifying question, surface it — do not answer on
  my behalf.
- If artifacts and the state file disagree, trust the artifacts, say so, and
  correct the state file.
- If I ask for something off-pipeline mid-run, handle it, then re-anchor on the
  current `phase`.
