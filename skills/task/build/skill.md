---
description: Run a task end to end as one human-gated pipeline with a recorded stage — assess once whether it splits into standalone subtasks, brief, HMML baseline, the business context only I hold, annotated delta, spec review loop, implement and test, comment pass, code quality standards, my review, PR, journal
argument-hint: '[<description> | #<issue> | --notes] [<phase>] [--no-hmml] [--no-context] [--no-split] [--resume-deep]'
allowed-tools: Skill, Read, Write, Edit, AskUserQuestion, Bash(hobby-struct-parser:*), Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(rg:*), Bash(git branch:*), Bash(git -C * status:*), Bash(git -C * diff:*), Bash(git -C * log:*), Bash(git -C * rev-parse:*), Bash(git -C * ls-files:*), Bash(git status:*), Bash(git diff:*), Bash(git log:*), Bash(gh pr list:*), Bash(gh pr view:*), Bash(gh issue view:*), Bash(gh issue list:*), Bash(gh repo view:*)
---

Run a task **end to end** as one predictable, human-gated pipeline. The individual
skills already exist (`journal:resume`, the HMML suite, `comment-pass`,
`structure`, `journal:write`); this is
the **thin orchestrator** that runs them in a fixed order, stops hard where I take over, and resumes in a
later session exactly where it left off.

**You are glue, not a reimplementation.** Never re-do what a sub-skill owns (HMML
grammar, delta-vs-full-generation). Invoke it and trust its contract. Keep the sequential spine in *this* main-loop context so each
phase reuses the last one's understanding — do not fan work out to subagents
except where a sub-skill already does (`journal:resume`).

## Four rules that override everything else

1. **Hard-stop at every human gate** — GATE S (the subtask assessment, once per
   task), GATE C (the business-context questions, before the delta is designed),
   GATE 1 (after the HMML spec review), and GATE 2 (once the change is
   implemented, tested and comment-passed). "Hard stop" means: finish the
   phase, write the state file, print a compact summary of what I need to do, and
   **end your turn**. Do not start the next phase, do not touch code, **do not
   create a worktree**, until I say so. **Reading the
   code is mine.** Nothing in this pipeline reviews the diff — `hmml-review` looks
   at the spec and `/comment-pass` at the comments, and that is deliberately all of
   it. Do not add a review pass of your own, and do not print a critique of your
   own change at GATE 2; give me the diff and the evidence.
2. **Never auto-commit** — not the source repo, not the notes repo where the
   `.hmml` baseline lives. That is my call, same contract as every sub-skill.
   Read-only git, `gh pr list` and `gh issue view` are fine. You do write the
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
   | `#<n>` or `gh-<n>` | `gh issue view <n>` — title, body, acceptance criteria |
   | `--notes`, or nothing | `journal:resume` over the task's notes dir |

   With no argument and a task named `gh-<n>-…`, do both: `journal:resume` **and**
   the issue. Record `brief_source` and the brief itself in the state file, and
   summarize the goal and next step in 2–4 lines. That framing carries through
   every later phase.

3. **State file** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/build-task.json`:

   ```json
   { "phase": "assess-split", "brief_source": "issue|description|notes",
     "brief": "<the brief>", "hmml_unit": "<unit>",
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
     "rounds": 0, "written": "<RFC 3339>", "phase_since": "<RFC 3339>" }
   ```

   `phase` is one of `assess-split`, `plan`, `context`, `await-hmml-review`,
   `revise`, `implement`, `comments`, `standards`, `review`, `await-review`,
   `pr`, `journal`, `handed-off` (this task became a coordinator and its work
   lives in its subtasks), `done`. `context` and `await-hmml-review` are the two
   waiting states: the pipeline has asked me something and stops until I answer.
   Every write sets `written` to that write's time; `phase_since` moves with it
   only when the write also changes `phase`, and otherwise keeps its old value.

   **`is_subtask`** marks a task this pipeline created. It is written into the
   seeded state file, never inferred, and it permanently skips `assess-split` —
   the split depth is exactly one. **`split`** records the one-time assessment so
   it is never re-asked, and **`subtasks`** lists what it produced; both are
   absent until `assess-split` runs.

   A leftover `dev-flow.json` in the same directory is read once for its `phase`
   and `hmml_unit`, then ignored — never written to again. A state file with no
   `split` record and a phase past `plan` gets
   `split: {"outcome": "refused", "reason": "task already in flight"}` rather
   than a back-filled assessment: the precondition below fails by construction
   once there are commits.

4. **Starting phase**, in this precedence: `phase: assess-split` outranks
   everything below — the subtask question is answered before any planning, and
   is skipped only when `is_subtask` is true, `no_split` is set, or `split` is
   already recorded. Else an explicit phase argument forces it; else `phase`
   from the state file; else **infer** from artifacts and say what you inferred
   — no `.hmml` → `plan`; an **unannotated** `.hmml` (baseline captured, no
   `# +` / `# -` markers yet) → `plan` resuming at the context step when
   `business-context.md` has no section for this task, or `context` resuming at
   the annotate step when it does (my answers are on disk, the delta is not); an
   **annotated** `.hmml` (`rg -c '^\s*# [+-]'` on it is non-zero) **and a clean
   worktree** → `await-hmml-review` — annotations are never stripped, so an
   annotated spec alone does not mean the change is unimplemented; uncommitted
   code changes → `implement`; an open PR for the branch → `done`.

   **`phase: handed-off` is terminal.** This task's work lives in its subtasks
   now. Print the `subtasks` list with the `hcdev wt <task>` command for each,
   remind me that each one runs its own `/task:build`, and end the turn. Do not
   plan, implement, or open a PR here — this task owns no code.

5. **Flags:** `--no-hmml` skips the spec phases entirely (plan the change inline in
   a sentence or two, then ask the context questions off that inline plan instead
   of off a delta). `--no-context` skips GATE C — for a change I have no domain
   background to give; the comment pass then has nothing to write an unverifiable
   *why* from, so it reports every such site as a gap. `--no-split` skips
   `assess-split` for this task, for when I already know it is one thing.
   `--resume-deep` uses `journal:resume-deep`. Record each as its state-file flag
   so a resume does not silently re-add the phase.

6. **Rehydrate before running a phase that is not `plan`.** A resumed session has
   none of the understanding the earlier phases built, and does not need to guess
   — read, in this order: the state file's `brief`, `context_gaps` and `evidence`;
   the annotated `.hmml` (**the delta is the intent**, `# +` / `# -` and all);
   the `hmml-review` plan; `business-context.md` — the domain facts I dictated at
   GATE C, in my own words, which are what the delta was designed from and the
   only text the comment pass may write an unverifiable *why* from; and
   `git status --short` / `git diff`. Then say in two lines what the task is
   doing and where it stopped.

Announce the resolved `{project, topic, task, brief_source, phase}` in one line,
then run from that phase onward. Never redo an earlier phase. Write the
state file at **every** transition; do not `git add`/`commit` it.

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
   GitHub-issue decision — do not reimplement any of it. Pass the brief as a
   complete, self-contained deliverable description: goal, file paths,
   constraints, done-when. It is read by a session that has no memory of this
   conversation.
2. **Seed its pipeline state** at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{new_task}/build-task.json`:
   ```json
   { "phase": "plan", "is_subtask": true, "brief_source": "description",
     "brief": "<the full brief>", "no_hmml": false, "no_context": false,
     "rounds": 0 }
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

**Gate 0 — prior art and size.** Before any spec or code. No sweeping changes
without prior discussion, and do not accelerate past someone else's active work.

- `gh pr list --search "<keywords>" --state all` and
  `gh issue list --search "<keywords>" --state all` on the repo this task targets.
  Report anything overlapping, with its state and author.
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
Carry the reading into the spec so GATE 1 sees it.

A repo with no `structure.hstruct` skips this silently — most have none, and a
task is not the place to argue for one.

**The HMML baseline: spec the *current* version first.** Before working out what
the change should be — before any design thinking at all — capture what the unit
does **today**. That spec is the baseline the whole task is stated against;
skipping it leaves the delta with nothing to be a delta *of*. Pick the entry from
what the task is:

- adjusting existing code → `hmml-extract` on that unit
- new behaviour → still baseline first: `hmml-extract` on the code that will
  **host** it (the plugin, the system, the endpoint it lands in), or `hmml-new`
  asked for a blank template when there is genuinely nothing to extract. A sparse
  baseline is a fine baseline.

If genuinely unclear which unit to baseline, ask **one** question. The sub-skill
writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml` —
**unannotated**, describing current behaviour only — and stops. Record
`hmml_unit`.

Do not think about the change while doing this, and do not let the intended
behaviour leak into the baseline: a baseline that already contains the change
cannot show what the change is.

**Ask for the business context.** The baseline says what the code does; the
change is not designed yet. This is the moment to collect what only I have — the
context the delta has to be designed inside: which rule sets a bound, what an
upstream system guarantees, what a later reader will break by tidying a shape
away, why the obvious approach was rejected before. Ask now and the answers
shape the delta, the plan, the code, the comments *and* the commit message. Ask
after the code exists and they can only decorate it.

Skip this step entirely on `--no-context`: set `phase: context` and carry on to
the annotate step in the same turn.

First **read what is already written down**: the brief, the baseline spec, the
GitHub issue, and the task notes. Ask for nothing recoverable from those — quote
it instead.

Then derive the questions from the goal read against the baseline: what does
building this need to know that the current code cannot tell you? **At most
seven questions**, each

- anchored to something named — a baseline line, a unit, a field, a rule in the
  brief;
- about the **system**: what produces this, what consumes it, which rule forces
  it, what goes wrong without it;
- open, with no draft answer and no leading guess — a proposed sentence with a
  question mark gets rubber-stamped, and then the reasoning is yours, not mine;
- carrying the one line you found while looking (`what I found: …`), or an
  explicit blank (`nothing in the notes or the issue covers this`).

**Nothing worth asking?** Say so in one line (`the brief is self-contained — no
business context needed`), set `phase: context`, and carry straight on to the
annotate step **without** stopping. A gate with nothing behind it is friction.

**→ GATE C: HARD STOP.** Otherwise set `phase: context`, write
`context_questions` verbatim (so a resumed session re-asks *these* rather than
inventing new ones) and the state file, then print the numbered questions and
one line: *"Answer any of these in your own words — `N: skip` if there is
nothing to say, and a skip is a real answer. Then say continue and I'll annotate
the delta off your answers."* End your turn.

______________________________________________________________________

## Phase `context` → GATE 1

The waiting state written at GATE C. Entered when my answers come back.

**The `context` phase argument re-opens it at any point** — the pipeline's one
loop, and how more of what I know gets in after I have seen something. Invoked
later, it derives its questions from whatever prompted the top-up (the delta I
just read at GATE 1, or the gaps the comment pass reported — each of those
carries its own question at a `file:line` you can now show me), appends the
answers to the same section, and returns to the phase it interrupted: the
annotate step when the top-up came from GATE 1, `comments` when the code is
already written.

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

Write `context_recorded` (today's date) and the state file, then carry on in the
same turn.

**Annotate the intended delta on top of the baseline.** *Now* think about the
change, with my answers in hand. Edit the `.hmml` so it also expresses the behaviour **this task
intends**, marking every difference with the annotation convention from grammar
§10 — the same file then holds both versions:

- `# - <text>` — a baseline line the change **removes**: comment it out in place,
  keeping its text verbatim (the marker's `-` stands in for the bullet, where the
  line had one) at its original indentation. **Never delete a baseline line
  outright.**
- `# +` — introduces **added** content: the line directly beneath it, plus any
  lines nested deeper than that line. Adjacent added siblings each take their own
  marker; a `struct`/`enum` head is covered through its closing brace.
- A **modified** line is a `# -` immediately followed by a `# +` — never an
  in-place rewrite.

The binding rule: dropping every `# +` block and un-commenting every `# -` line
must reproduce the baseline **exactly**. Re-lint with
`hobby-mml-parser <path> --lint` — annotations are plain comments, so the file
must still parse clean, and the linter checks recoverability directly, reporting
`unrecoverable-baseline` when un-annotating no longer gives a coherent spec (plus
`empty-change-removal` / `empty-change-addition` / `redundant-change-bullet` for
a marker that says nothing).

Cover every section the change touches, not just `## behavior` — an added
component, a new `## systems` entry with its access block, a removed invariant.
A `# +` step whose new component or new invariant is missing gives the review an
incomplete delta.

A recorded answer the delta has to honour belongs **in the spec**, as an
invariant or a `# +` line — not carried in your head to be remembered at the
comment pass. That is what makes GATE 1 able to check whether the design used
what I said.

**Review the delta.** Run `hmml-review` on the annotated `.hmml`: parallel
lens reviewers over the delta and the current code, synthesized into a prioritized
implementation plan in the task notes, scoped to what the annotations add and
remove. Name the `business-context.md` path when you invoke it, so its lenses
test the delta against the facts I gave rather than against their own
assumptions. Not `hmml-validate` — the annotations are deliberately ahead of the code,
so a conformance check would report the intended change as drift.

**→ GATE 1: HARD STOP.** Set `phase: await-hmml-review`, write the state file,
print:

- the spec path;
- **the annotated delta itself** — every `# -` and `# +` line, grouped by
  section, with a count. This is the change I am approving, so show it rather
  than describing it;
- the implementation plan (path plus an inline summary);
- **which recorded context the delta leaned on** — one line per handle I
  answered and where it landed in the spec, plus any handle the delta ignored.
  This is the check on the annotate step: I can see whether the design used what
  I said, which is the whole reason the questions came before it;
- *"Review the baseline, the `# +`/`# -` delta on it, and the plan. Refine any of
  them, then say `continue` — or say `revise` and I'll re-review what you
  changed. If reading it makes me want to tell you more, `/task:build context`
  re-asks and rebuilds the delta off the fuller answers."*

End your turn.

## Phase `await-hmml-review` and `revise` — the loop

- `revise` re-runs `hmml-review` on the edited spec, increments `rounds`, returns
  to `await-hmml-review`, and stops again. No cap: this loops until I am happy.
- Each round reports what changed in the **delta** since the previous one — which
  markers were added, dropped or reworded — so I can see whether my edit did what
  I meant.
- `continue` moves to `implement`. Committing the annotated `.hmml` is mine to do;
  if it is still uncommitted when I say continue, say so once and proceed.
  Committing it does not erase the delta the way an unannotated in-place edit
  would — the markers are in the file.

______________________________________________________________________

## Phase `implement`

Entered when I say continue after GATE 1. The business context is already on
disk — GATE C collected it before the delta was designed — so nothing is asked
from here on.

Write comments to the rule as you go — a module header, a public element's
contract, a test's justification, nothing else. The `comments` phase is a check,
not a laundry: do not write comments expecting it to delete them. Where the
contract you are writing needs a *why* the code cannot show, take it from
`business-context.md` and from nowhere else.

**Implement.** Unless `--no-hmml`, run `hmml-translate <unit>`: it reads the
`# +` / `# -` annotations as the delta — the unannotated baseline around them is
code that already exists — and applies the **minimal** code change, compiles,
tests, and stops. It owns delta-vs-full-generation — do not second-guess it. A
`# -` it declines to delete (the spec is lossy, so absence ≠ dead code) comes back
as a flag for me, not as a silent skip: carry it into GATE 2. With `--no-hmml`,
make the change directly.

**Test.** Drive it end to end: run the app or the affected flow, plus the repo's
own test command (`cargo test`, `cargo clippy`, `just test`, whatever it uses).
Then, unless `--no-hmml`, `hmml-validate` once more — the code should report
**`CONFORMS`** to the annotated spec: every `# +` claim present in the code, every
`# -` behaviour gone from it. Report residual `SPEC-AHEAD` as unfinished work.

**Leave the annotations in place.** The `.hmml` keeps its `# +` / `# -` markers
after the code lands — the spec is the record of *this* change, and stripping it
back to a plain baseline would erase what the task did. Never edit the spec here
at all; committing it stays my call at GATE 2.

Record the evidence, then fall through to `comments` without stopping.

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

**→ GATE 2: HARD STOP.** Set `phase: await-review`, write the state file, print:

- the diff summary — files, one line each on what changed;
- the test evidence and the validate verdict, plus the spec's `# +` / `# -` delta
  as it now stands — it keeps its annotations, so any marker validate still
  reports as `SPEC-AHEAD` is outstanding work, not a leftover;
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

End your turn.

______________________________________________________________________

## Phase `pr` — on my go only

Write the description to the rule in `~/.claude-hobby/CLAUDE.md`: a title in the
commit-subject shape, then **What do we change** and **Why do we change** at one or
two sentences each, then **How can you test it by hand** as an ordered bullet list
whose last bullet says what I should see. No other sections, no `## AI use`, no
empty section, no attribution.

When the task has an issue — a `gh-<number>` in the task or branch name, or the one
`brief_source: issue` read — the body's first line is `Closes #<number>`, so the
issue closes when the PR merges. A task with no issue writes no such line, and a
number is never guessed.

The *why* is sourced, not invented — from my prompts, from what we settled, or from
something you verified in the `implement` phase. If you do not have one, ask before
opening the PR.

The manual-test bullets come from what you actually ran in `implement`. If nothing
there was run by hand, say so in one bullet and name what does cover it.

The `track-pr` hook records the PR to `prs.json`. If it did not fire, append the
PR there yourself (create with `[]` first; never store the PR's live state).

Hobby repos are mine, so a PR is optional — for a small solo change, pushing to
the default branch may be right. Ask if it is not obvious, and skip to `journal`
when I say no PR.

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
  and file the rest as issues.
- If artifacts and the state file disagree, trust the artifacts, say so, and
  correct the state file.
- If I ask for something off-pipeline mid-run, handle it, then re-anchor on the
  current `phase`.
