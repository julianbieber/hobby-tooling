---
description: Run a task end to end as one human-gated pipeline with a recorded stage and a live todo list — assess once whether it splits into standalone subtasks, brief, the business context only I hold, a short plan, implement and test, read the implemented change back as an HMML delta derived from the diff, comment pass, code quality standards, my review, PR, journal
argument-hint: '[<description> | --issue | --notes] [<phase>] [--no-hmml] [--no-context] [--no-split] [--resume-deep]'
allowed-tools: Skill, Read, Write, Edit, AskUserQuestion, Bash(hobby-struct-parser:*), Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(rg:*), Bash(git branch:*), Bash(git -C * status:*), Bash(git -C * diff:*), Bash(git -C * log:*), Bash(git -C * rev-parse:*), Bash(git -C * ls-files:*), Bash(git -C * merge-base:*), Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(git merge-base:*), Bash(gh pr list:*), Bash(gh pr view:*), Bash(gh issue view:*), Bash(gh issue list:*), Bash(gh repo view:*), Bash(hcoord ci:*), Bash(gh run list:*), Bash(gh run rerun:*)
---

Run a task **end to end** as one predictable, human-gated pipeline. The individual
skills already exist (`journal:resume`, `hmml-extract`, `comment-pass`,
`structure`, `journal:write`); this is
the **thin orchestrator** that runs them in a fixed order, stops hard where I take over, and resumes in a
later session exactly where it left off.

**You are glue, not a reimplementation.** Never re-do what a sub-skill owns (HMML
grammar, how a delta is derived from a diff). Invoke it and trust its contract. Keep the sequential spine in *this* main-loop context so each
phase reuses the last one's understanding — do not fan work out to subagents
except where a sub-skill already does (`journal:resume`).

**Only facts are settled before code**: what the task is, and the business
context only I hold. Design is not reviewed up front. Once the change is
implemented and tested, it is read back as an HMML delta derived from the diff —
the HMML exists for me to understand what the code change is doing, so it
represents the implemented change, never a plan.

## Four rules that override everything else

1. **Hard-stop at every human gate** — GATE S (the subtask assessment, once per
   task), GATE C (the business-context questions, before anything is designed),
   and GATE 2 (once the change is implemented, tested, read back as a delta and
   comment-passed). "Hard stop" means: finish the
   phase, set `waiting_on`, write the state file, print a compact summary of what I need to do, and
   **end your turn**. Do not start the next phase, do not touch code, **do not
   create a worktree**, until I say so. **Reading the
   code is mine.** Nothing in this pipeline reviews the diff — the derived delta
   is for me to read, `/comment-pass` looks at the comments and `/structure`
   counts, and that is deliberately all of it. Do not add a review pass of your
   own, and do not print a critique of your own change at GATE 2; give me the
   diff, the delta and the evidence.
2. **Never auto-commit** — not the source repo, not the notes repo where the
   `.hmml` delta lives. That is my call, same contract as every sub-skill.
   Read-only git and `gh pr list` are fine. You do write the
   commit *message* when I ask for one, to the rule in `~/.claude-hobby/CLAUDE.md`
   — terse, a record of the decisions, and **no justification you were not
   given**. If a decision in the diff has no justification from my prompts or from
   what we settled, ask rather than inventing one.
3. **Never write a reason I did not give you.** The comment rule and the
   commit-message rule in `~/.claude-hobby/CLAUDE.md` both allow exactly three
   sources for a justification: my prompts, what we settled in discussion, or a
   fact you verified in the code. GATE C is where the first two get written
   down, verbatim, in `business-context.md`. A *why* no recorded passage and no
   verified fact supports is **reported as a gap**, never filled in: a missing
   explanation is something I can fix by extending the context, an invented one
   is wrong forever and nothing later contradicts it.
4. **No machine attribution** in anything you write — no `Co-Authored-By:
   Claude`, no `Claude-Session:` trailer, no `Generated with Claude Code` footer,
   no 🤖. It is advertising and these repos do not carry it. Comments, commit
   messages, PR descriptions and issue text have rules in
   `~/.claude-hobby/CLAUDE.md`, and you follow them.

## Step 0 — Resolve context, brief and phase

1. **Task context** from the worktree path (`pwd`), which is
   `~/fun_worktrees/PROJECT/TOPIC/TASK`: first dir project, second topic, third
   the task ID, verbatim, blurb suffix and all. There is no repo level — the task
   dir is the worktree. Fall back to `git branch --show-current`, skipping
   `master`/`main`/`develop`.

2. **The brief**, from the first argument:

   | argument | brief |
   |---|---|
   | free text | the text itself, recorded verbatim |
   | `--issue` | the task's `issue.md` — title, body, acceptance criteria, progress |
   | `--notes`, or nothing | `journal:resume` over the task's notes dir |

   With no argument and an `issue.md` in the task's notes dir, do both:
   `journal:resume` **and** the issue. Record `brief_source` and the brief itself in the state file, and
   summarize the goal and next step in 2–4 lines. That framing carries through
   every later phase.

3. **State file** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/build-task.json`:

   ```json
   { "phase": "assess-split", "brief_source": "issue|description|notes",
     "brief": "<the brief>", "plan": "<plan file name>",
     "hmml_units": ["<unit>"],
     "no_hmml": false, "no_context": false, "no_split": false,
     "is_subtask": false,
     "split": { "assessed": "<YYYY-MM-DD>", "outcome": "none|refused|split",
                "reason": "<one line>" },
     "subtasks": [
       { "task": "<new task id>", "goal": "<one-line brief>",
         "created": "<YYYY-MM-DD>" }
     ],
     "size_budget": "", "prior_art": "",
     "context_questions": [], "context_recorded": null, "context_gaps": [],
     "todo": [ { "text": "<step>", "status": "pending" } ],
     "waiting_on": null,
     "ci_rounds": 0,
     "written": "<RFC 3339>", "phase_since": "<RFC 3339>" }
   ```

   `phase` is one of `assess-split`, `plan`, `context`, `implement`, `delta`,
   `comments`, `standards`, `review`, `await-review`, `pr`, `ci`, `journal`,
   `handed-off` (this task became a coordinator and its work lives in its
   subtasks), `done`. `context` is the waiting state GATE C leaves: the pipeline
   has asked me something and stops until I answer.
   Every write sets `written` to that write's time; `phase_since` moves with it
   only when the write also changes `phase`, and otherwise keeps its old value.

   **`is_subtask`** marks a task this pipeline created. It is written into the
   seeded state file, never inferred, and it permanently skips `assess-split` —
   the split depth is exactly one. **`split`** records the one-time assessment so
   it is never re-asked, and **`subtasks`** lists what it produced; both are
   absent until `assess-split` runs.

   **Migrate an old record** before anything else, and say so in one line: a
   phase of `await-hmml-review` or `revise` becomes `implement` (its annotated
   `.hmml` and review plan stay in the notes dir as notes; nothing is
   re-reviewed), `hmml_unit` becomes `hmml_units`, and `rounds` is dropped. A
   leftover `dev-flow.json` in the same directory is read once for its `phase`,
   then ignored — never written to again. A state file with no
   `split` record and a phase past `plan` gets
   `split: {"outcome": "refused", "reason": "task already in flight"}` rather
   than a back-filled assessment: the precondition below fails by construction
   once there are commits.

4. **Starting phase**, in this precedence: `phase: assess-split` outranks
   everything below — the subtask question is answered before any planning, and
   is skipped only when `is_subtask` is true, `no_split` is set, or `split` is
   already recorded. Else an explicit phase argument forces it; else `phase`
   from the state file; else **infer** from artifacts and say what you inferred
   — `business-context.md` has no section for this task and there is no
   `plan-*.md` → `plan`; a section but no plan file → `context`, resuming at the
   plan step (my answers are on disk, the plan is not); uncommitted code changes
   or commits ahead of the default branch → `implement`; an open PR for the
   branch → `done`.

   **`phase: handed-off` is terminal.** This task's work lives in its subtasks
   now. Print the `subtasks` list with the `hcdev wt <task>` command for each,
   remind me that each one runs its own `/task:build`, and end the turn. Do not
   plan, implement, or open a PR here — this task owns no code.

5. **Flags:** `--no-hmml` skips the `delta` phase, for a change I can read
   straight from the diff. `--no-context` skips GATE C — for a change I have no
   domain background to give; the comment pass then has nothing to write an
   unverifiable *why* from, so it reports every such site as a gap.
   `--no-split` skips `assess-split` for this task, for when I already know it
   is one thing. `--resume-deep` uses `journal:resume-deep`. Record each as its
   state-file flag so a resume does not silently re-add the phase.

6. **Rehydrate before running a phase that is not `plan`.** A resumed session has
   none of the understanding the earlier phases built, and does not need to guess
   — read, in this order: the state file's `brief`, `todo`, `context_gaps` and
   `evidence`; the plan file; `business-context.md` — the domain facts I
   dictated at GATE C, in my own words, which are what the change was designed
   from and the only text the comment pass may write an unverifiable *why* from;
   the derived `.hmml` delta when `delta` has run; and `git status --short` /
   `git diff`. Then say in two lines what the task is doing and where it stopped.

Announce the resolved `{project, topic, task, brief_source, phase}` in one line,
then run from that phase onward. Never redo an earlier phase. Write the
state file at **every** transition; do not `git add`/`commit` it.

## Live state

`hdash` shows `todo` and `waiting_on` on the task's card, so keep both current
while you work — they are how I see where a session is without opening it.

- **`todo` is the plan.** Seed it from the plan file's steps when the plan is
  written, one line each, every item `pending`. Items only change status —
  `active`, `done`, `dropped`; a step found on the way is inserted where it
  runs. Exactly one item is `active` while work is under way; flip it to `done`
  in the same write that starts the next one. Open items that outlive the task
  go to the journal at `journal`, never into `todo`.
- **`waiting_on`** is one line naming what I decide or do, set at every hard
  stop and when a sub-skill asks me something; `null` again in the write that
  starts the next step.

______________________________________________________________________

## Phase `assess-split` → GATE S (once per task)

The **first** phase of a new task, before any planning. Ask once whether this is
really one task, or several deliverables that each want their own worktree,
branch, PR and session. Then record the answer so it is never asked again.

Skip the phase entirely — set `phase: plan` and continue — when `is_subtask` is
true, `no_split` is set, or `split` is already recorded.

**S1 — Check the precondition, before thinking about the cut at all.** A
subtask's PR targets the default branch, which is only sound while this task has
no unmerged work of its own for the subtask to miss. Both must hold:

```bash
git status --short                              # must be empty
git log --oneline "origin/$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name)..HEAD"   # must be empty
```

If either fails the split is **refused** — not deferred, not worked around.
Record `split: {"outcome": "refused", "reason": "<which check failed>"}`, say in
one line that a mid-task split would make subtasks branch off unmerged work, set
`phase: plan`, and carry on. Do not ask me to approve a split you already know
is unsound.

**S2 — Read the whole task.** Step 0 already resolved the brief; if it came from
a bare description, also run `journal:resume` over the notes dir. This is the
only phase that sees the task before it is carved up, so read all of it.

**S3 — Draft the cut as deliverables, not as components.** Write the candidate
list as the order in which a player or a caller gains capability, *before*
looking at the code layout. Starting from the architecture is what produces
layer tasks, and shaping rule 1 exists to reject exactly those.

**S4 — Test every candidate against the six Task shaping rules** in
`~/.claude-hobby/CLAUDE.md`. That file is the standard and this skill
deliberately does not restate it. State each failure rather than silently fixing
it — a candidate that had to be re-cut is information about the shape of the
work, and I want to see it at the gate. The re-cuts that fix nearly everything:

- **Layer candidate → vertical slice.** "Add the component" becomes one narrow
  thing that is visible in the running game.
- **Too large → narrow the case, not the stack.** One enemy kind, one biome,
  happy path only. Slicing by case keeps it end-to-end; slicing by layer does
  not.
- **Carries an open question (rule 5) → make the decision its own item,** and
  make whatever depends on the answer wait. A question is not a deliverable.
- **Leaves state to be remembered (rule 6) → pull the cleanup into the same
  subtask,** or make it a subtask that exists now.

**S5 — Check the set, not just the items.** Three properties the list as a whole
must have, all three printed at the gate:

- **Disjoint files** pairwise. Two subtasks that edit the same file cannot be
  worked in parallel sessions; name the file, then fold them together or move
  the boundary.
- **Every subtask stands alone** — if backing one out forces another out, they
  are one subtask cut on the wrong axis (rule 3).
- **Nothing is left to remember** (rule 6) once every subtask has merged — no
  flag, no dual path, no half-done migration.

**S6 — Decide whether to split at all.** Fewer than two surviving candidates
means this is one task: record `split: {"outcome": "none"}`, say so in one line,
set `phase: plan`, and continue into the normal pipeline. **This is the expected
answer for most tasks** — do not manufacture a decomposition to justify the
phase.

**→ GATE S: HARD STOP.** Write the state file with `phase: assess-split` still
set, then print:

- the **end state** in one or two sentences: what is true once every subtask has
  landed;
- a table of proposed subtasks — blurb, the user-visible behaviour, the
  done-when observation (rule 2), the files it owns, and any `enabling` label
  with the task it unlocks;
- the **rejected candidates**, with the rule each failed and how it was re-cut;
- the three set-level checks from S5, each `pass` or the specific pair that
  fails;
- the exact commands the approval will run, one line each, so I can see the
  blast radius before it happens;
- *"Edit, merge or drop any subtask, then say continue and I'll create them. Say
  `no split` and I'll carry on as one task."*

End your turn. **Create no worktree, file no issue.**

______________________________________________________________________

## Phase `handed-off` — on my go at GATE S

Only after I approve the decomposition. Per approved subtask, in order:

1. **Create it** with `/subtask "<the full brief>"`. That skill owns the
   worktree, the branch, the notes seed, the `subtasks.json` entry and the
   issue decision — do not reimplement any of it. Pass the brief as a
   complete, self-contained deliverable description: goal, file paths,
   constraints, done-when. It is read by a session that has no memory of this
   conversation.
2. **Seed its pipeline state** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{new_task}/build-task.json`:
   ```json
   { "phase": "plan", "is_subtask": true, "brief_source": "description",
     "brief": "<the full brief>", "no_hmml": false, "no_context": false,
     "todo": [], "waiting_on": null }
   ```
   `is_subtask: true` is what bounds the depth at one — without it the subtask's
   own session would assess a split of its own.

Then, once per task:

3. Record every subtask under `subtasks` with `task`, `goal` and `created`, set
   `split: {"outcome": "split", …}` and `phase: handed-off`, and write the state
   file.
4. Run `journal:write` once, recording the decomposition and *why* each cut fell
   where it did. The rejected candidates belong here too — that reasoning is the
   part that would otherwise be lost, and the subtask journals will not have it.
5. Print the handoff: each subtask with its worktree path and its
   `hcdev wt <task>` command, a note that `hdash` lists them under this task's
   card, and a reminder that each session runs its own `/task:build` and gates
   independently.

**Nothing is launched here.** `/subtask` sets up directories, as it says it
does; opening a session is a click in `hdash` or an `hcdev wt` I run myself.
**This task implements nothing** — it owns no code, opens no PR, and does
not poll the subtasks. End your turn.

______________________________________________________________________

## Phase `plan` → GATE C

**Gate 0 — prior art and size.** Before any code. No sweeping changes
without prior discussion, and do not accelerate past someone else's active work.

- `gh pr list --search "<keywords>" --state all` on the repo this task targets,
  and `rg -il "<keywords>" ~/fun_repos/hobby-mimisbrunnr/notes/<project>/*/*/issue.md`
  for the issues in the notes. Report anything overlapping, with its state.
- State an expected size: files touched, rough lines changed.
- **Stop and ask** if an open PR or active issue covers this work, or the expected
  change exceeds **10 files or 500 lines**.

Record `size_budget` and `prior_art`. On my own repo this is usually one empty
search; on a repo I do not own it is the difference between a welcome PR and a
closed one.

**Gate 0b — the standards this repo holds.** If the target repo has a
`structure.hstruct` (at its root, or at the crate root in a workspace), read it
and run `hobby-struct-parser <repo>` for the current numbers. State in one line
each:

- the standards, and which the planned change is likely to move, in which
  direction;
- any reviewed (`review:`) standard whose subject the change touches.

This is the cheap end of the loop. "This adds a module importing eight others and
the standard says six" is a design choice at plan time and rework at review time.
Carry the reading into the plan file.

A repo with no `structure.hstruct` skips this silently — most have none, and a
task is not the place to argue for one.

**Read the code the change will touch.** Enough to know what it does today —
the units, the data, the call paths the brief lands in. No spec is written here:
the delta is derived from the diff once the change exists, and that is where the
baseline comes from.

**Ask for the business context.** The change is not designed yet. This is the
moment to collect what only I have — the context the change has to be designed
inside: which rule sets a bound, what an upstream system guarantees, what a
later reader will break by tidying a shape away, why the obvious approach was
rejected before. Ask now and the answers shape the plan, the code, the comments
*and* the commit message. Ask after the code exists and they can only decorate
it.

Skip this step entirely on `--no-context`: set `phase: context` and carry on to
the plan step in the same turn.

First **read what is already written down**: the brief, the task's `issue.md`,
and the task notes. Ask for nothing recoverable from those — quote it instead.

Then derive the questions from the goal read against the code: what does
building this need to know that the current code cannot tell you? **At most
seven questions**, each

- anchored to something named — a file, a unit, a field, a rule in the brief;
- about the **system**: what produces this, what consumes it, which rule forces
  it, what goes wrong without it;
- open, with no draft answer and no leading guess — a proposed sentence with a
  question mark gets rubber-stamped, and then the reasoning is yours, not mine;
- carrying the one line you found while looking (`what I found: …`), or an
  explicit blank (`nothing in the notes or the issue covers this`).

**Nothing worth asking?** Say so in one line (`the brief is self-contained — no
business context needed`), set `phase: context`, and carry straight on to the
plan step **without** stopping. A gate with nothing behind it is friction.

**→ GATE C: HARD STOP.** Otherwise set `phase: context`, write
`context_questions` verbatim (so a resumed session re-asks *these* rather than
inventing new ones), set `waiting_on` to `answer the GATE C questions`, write
the state file, then print the numbered questions and one line: *"Answer any of
these in your own words — `N: skip` if there is nothing to say, and a skip is a
real answer. Then say continue and I'll plan and build off your answers."* End
your turn.

______________________________________________________________________

## Phase `context` → `implement`

The waiting state written at GATE C. Entered when my answers come back.

**The `context` phase argument re-opens it at any point** — the pipeline's one
loop, and how more of what I know gets in after I have seen something. Invoked
later, it derives its questions from whatever prompted the top-up (the delta I
just read at GATE 2, or the gaps the comment pass reported — each of those
carries its own question at a `file:line` you can now show me), appends the
answers to the same section, and returns to the phase it interrupted:
`implement` when the code has to change, `comments` when only the comments do.

**Record my answers, verbatim.** Append them to
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/business-context.md`
(create it with the notes front matter — `title: "{task} — business context"`,
today's `created`, tag `context`), one `## <short handle>` block per answer:

```markdown
## wave-cap
Q: why cap concurrent spawners rather than queueing the overflow?
The spawn budget is what keeps the frame time under the 16ms target on the
Steam Deck; a queue would hide the cap and stutter instead of dropping a wave.
```

**Copy my wording. Never paraphrase, summarize, complete or tidy it.** This file
is the only text the comment pass and the commit message may take an
unverifiable *why* from, so a sentence you smoothed here becomes a claim I never
made. Record a skip too — the handle with the body `skipped — no context to
give` — which is what tells the comment pass to drop that site instead of
hunting for a passage. A question I leave unanswered when I say continue is a
skip: record it as one rather than asking again.

Write `context_recorded` (today's date), clear `waiting_on`, and carry on in the
same turn.

**Write a short plan.** One file,
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/plan-{YYYY-MM-DD}-{slug}.md`,
in the plan format of `~/.claude-hobby/CLAUDE.md` — Goal, Approach, Steps, Key
Files, Open Questions — and short: the steps are the order you will work in,
one line each, naming files. A recorded answer the change has to honour is
cited in the step it shapes by its `business-context.md` handle, so it is not
carried in your head to be remembered at the comment pass. A new dated file per
planning session; never overwrite an earlier one. Record its name as `plan` and
seed `todo` from its steps (**Live state**). The plan is not a gate: say in one
line where it is, set `phase: implement`, and carry on.

______________________________________________________________________

## Phase `implement`

Entered from `context` in the same turn, or on a resume. Work from the plan
file and keep `todo` moving as you go. A design question only I can answer is
asked when it comes up, as one line with the options you see — set
`waiting_on`, stop, and resume at the same `todo` item. It is not saved for a
gate, and it is not answered on my behalf.

Write comments to the rule as you go — a module header, a public element's
contract, a test's justification, nothing else. The `comments` phase is a check,
not a laundry: do not write comments expecting it to delete them. Where the
contract you are writing needs a *why* the code cannot show, take it from
`business-context.md` and from nowhere else.

**Test.** Drive it end to end: run the app or the affected flow, plus the repo's
own test command (`cargo test`, `cargo clippy`, `just test`, whatever it uses).
Then **run the repo's CI locally**: for every job in `.github/workflows/*.yml`
and `*.yaml` triggered by `pull_request` or `push`, its `run:` steps in order
from the worktree root, with the job's `env:` and each step's
`working-directory:`. A matrix runs once, on this machine's toolchain. Skip
`uses:` steps and any step that reads `secrets.` or needs a `services:`
container or another OS, and carry the skipped ones into GATE 2. A failing step
is fixed here like a failing test, so the diff I review is one CI is expected to
pass. No workflows → nothing to run.

Record the evidence, then fall through to `delta` without stopping.

## Phase `delta`

Skip with `--no-hmml`, and for a diff with no behaviour change — docs, tests
only, config values, a mechanical rename — saying so in one line.

Run `/hmml-extract --against <ref>`, with `<ref>` the merge base of the
worktree and the default branch
(`git merge-base HEAD "origin/$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name)"`).
It picks the units the diff touches, writes each as the worktree leaves it with
what the change did marked `# -` / `# +`, and lints the set; if it asks which
units, surface the question. Record `hmml_units`.

The delta is **derived, never maintained**: do not edit it, and rerun this phase
after any later change to the code's behaviour — a fix I ask for at GATE 2, a
CI repair. It is for me to read; it is not a review of the change and it does
not gate anything. Fall through to `comments`.

______________________________________________________________________

## Phase `comments`

Run `/comment-pass`, naming the `business-context.md` path (and passing
`--no-context` when the task carries that flag, so the skill knows the missing
file is deliberate). It enforces the comment rule over the branch diff in both
directions — deletes what the rule disallows, relocates what carried real
information, and writes the module headers, public contracts and test
justifications that are missing, taking any *why* it cannot verify from the code
out of the recorded context and from nowhere else.

Three parts of its report are inputs to the next phase rather than side effects:

- the **shape-decision notes** it pulled out of deleted body comments. Those are
  facts about why the code looks the way it does, and their home is the commit
  message. Carry them into the gate verbatim.
- every finding it **declined to act on** — a test whose justification could not
  be written, a module with no single responsibility, anything it could not verify
  from the code. Each is a decision for me.
- the **context gaps**: sites where the rule wants a comment, the code cannot
  supply the reason, and no recorded passage covers it. Where a gap sits
  somewhere a comment genuinely belongs, say so in one line and offer
  `/task:build context`, which re-opens the questions for those sites and comes
  back here to re-run the pass. Whether to take that is my call. **Never close a
  gap by writing the comment yourself.**

Record all three — the gaps to `context_gaps` — then fall through to
`standards`.

______________________________________________________________________

## Phase `standards`

Skip entirely if the target repo has no `structure.hstruct`.

Run `/structure` on the target repo. Three outcomes, and only one needs you to
do anything:

- **Unchanged** — nothing to report.
- **Slack** — the change improved a number. Run the ratchet; the tightened budget
  joins this task's diff, which is how an improvement gets recorded by the change
  that made it.
- **Exceeded** — the change moved the repo further from a standard it states.
  **Try to fix it**: read `--detail` to find what this change added, and repair
  it. A new cycle usually wants an event or a read-only query rather than an
  import; a file crossing the line usually wants the split its responsibility
  already implies. Re-run after the fix.

  **If the fix does not work, carry it to GATE 2 as a regression** — the standard,
  the count before and after, and what you tried. Do not route around it and do
  not raise the budget. Raising is a hand edit I make deliberately, because it
  says in the diff that this project got further from its own standard.

Then check the **reviewed** standards. A `review:` standard is only re-audited if
something asks, so read each one against this diff: if the change touches what it
covers, say so and ask whether to re-audit and bump `audited:`. Do not bump a date
for a standard the change did not touch, and do not treat an old date on an
unrelated standard as a finding.

Record the delta, then fall through to `review`.

______________________________________________________________________

## Phase `review` → GATE 2

**→ GATE 2: HARD STOP.** Set `phase: await-review`, set `waiting_on` to
`read the change at GATE 2`, write the state file, print:

- the diff summary — files, one line each on what changed;
- the test evidence, and the CI steps skipped locally;
- **the derived delta** — each `.hmml` path, then every `# -` and `# +` line
  grouped by section, with a count. This is the change at mental-model altitude,
  so show it rather than describing it; a line of it that surprises me is the
  cheapest place to catch a change that does something the brief did not ask;
- what `/comment-pass` changed, each written comment with the context handle
  behind it where it had one, and what it declined to act on;
- the **context gaps** with their `file:line` — a gap is a place the code
  carries no explanation and I gave none, so name it rather than burying it;
  closing one means re-running `/task:build context`, which is a decision for
  now, not after merge;
- the standards delta — each budget before and after, any tightened by the ratchet,
  and **any regression you could not fix**, with what you tried;
- the shape-decision notes it extracted, verbatim, for the commit message, and
  the `business-context.md` handles a commit body may cite — those are the
  written form of "something I said" and "something we settled", the two sources
  a justification may otherwise not be derived from;
- the size against the Gate 0 budget, and by how much it was crossed;
- *"Review the above. Say continue to open the PR, or tell me what to change."*

A change I ask for here goes back through `implement` and `delta` before the
gate is printed again. End your turn.

______________________________________________________________________

## Phase `pr` — on my go only

Write the description to the rule in `~/.claude-hobby/CLAUDE.md`: a title in the
commit-subject shape, then **What do we change** and **Why do we change** at one or
two sentences each, then **How can you test it by hand** as an ordered bullet list
whose last bullet says what I should see. No other sections, no `## AI use`, no
empty section, no attribution.

The *why* is sourced, not invented — from my prompts, from what we settled, or from
something you verified in the `implement` phase. If you do not have one, ask before
opening the PR.

The manual-test bullets come from what you actually ran in `implement`. If nothing
there was run by hand, say so in one bullet and name what does cover it.

The `track-pr` hook records the PR to `prs.json`. If it did not fire, append the
PR there yourself (create with `[]` first; never store the PR's live state).
Set `phase: ci` and continue.

Hobby repos are mine, so a PR is optional — for a small solo change, pushing to
the default branch may be right. Ask if it is not obvious, and skip to `journal`
when I say no PR.

## Phase `ci` — the PR is green before the pipeline ends

The local run in `implement` cannot reproduce `uses:` steps, secrets, service
containers or another OS, so the PR's own checks are the authority. Watch them
and repair them; never hand me a red PR as finished.

1. `hcoord ci <pr url> --spent <ci_rounds>`, Bash `timeout: 600000`. At most
   four calls on `timeout`; a fourth is step 5 with
   `checks still pending after ~38 minutes`.
2. `outcome: green` → done here, on to `journal`. `checks: none` while the repo
   has workflows is not green yet: `gh run list --commit <head sha> --json
   status` — a run listed means call 1 again; none listed means no workflow
   triggers on this PR.
3. `outcome: fix` → the failure is the diff's own. Repair it reading the files
   `log` names, run the failing step locally, `/comment-pass` over what changed,
   commit to the message rule, push, and rerun `delta` when the repair changed
   behaviour. Increment `ci_rounds`, back to 1. The repair is shown in the
   wrap-up, because it landed after GATE 2.
4. `outcome: rerun` → `gh run rerun --failed <run>` only. Increment
   `ci_rounds`, back to 1.
5. `outcome: blocked` (`not-mine`, `decision`, or the bound of three spent) →
   print the PR, the case, the failing check and at most three log lines, set
   `waiting_on`, and **stop**: the next step is mine.

## Phase `journal`

Run `journal:write` — goal, what changed, evidence, open items, with the context
gaps under the open items so they survive into the next session's resume. Set
`phase: done`.
Print a one-line wrap-up: task, PR link if any, anything still open. End.

______________________________________________________________________

## Notes on scope

- If a sub-skill asks its own clarifying question, surface it — do not answer on
  my behalf. The pipeline asks for wording of its own in exactly one place,
  GATE C, and what I answer there is recorded verbatim: no phase and no
  sub-skill may smooth it into prose of yours.
- **A subtask is a separate deliverable; this pipeline is one branch.** Work that
  wants its own worktree, branch and PR is a subtask, decided once at GATE S back
  before this branch had commits. Once it does, the answer is to finish this task
  and write the rest up as issues with `/issue-create`.
- If artifacts and the state file disagree, trust the artifacts, say so, and
  correct the state file.
- If I ask for something off-pipeline mid-run, handle it, then re-anchor on the
  current `phase`.
