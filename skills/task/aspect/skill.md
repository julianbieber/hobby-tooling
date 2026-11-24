---
description: Work one aspect of a task in its own tab — a research aspect answered into findings.md, a code aspect built through /task:build to a merged PR against its base
allowed-tools: Skill, ListAgents, SendMessage, Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(rg:*), Bash(git status:*), Bash(git log:*), Bash(git diff:*), Bash(gh pr view:*), Bash(nu -c:*), Bash(cargo:*), Bash(hresearch:*), WebSearch, Read, Write, Edit
---

You are the Claude of one **aspect**: a piece of a task's own work, worked in
a tab of the task's session so it does not fill the task's own context. Its
`kind` in `aspect.json` says what done means, and this skill does the same
first two steps for every kind:

- **research** — answer the question into `findings.md` and stop (steps 3–5).
  No code goes into the task and no PR opens.
- **code** — build the change through `/task:build` in this worktree, to a PR
  against the aspect's base that has merged (step 6).

Change nothing outside the aspect's worktree and notes dir, and the task's
`business-context.md`. **`aspect.json` is the aspect's whole record**: the task
reads its outcome and open items there, so this session never writes the task's
`issue.md`, `implementation.md` or journal.

The argument `finish` goes from step 1 straight to step 7.

## 1. Resolve

`pwd` is `~/fun_aspects/PROJECT/TOPIC/TASK/NAME` — the aspect's worktree, on
branch `TASK@NAME`, cut from the task's committed state. Anywhere else, stop
with "/task:aspect runs in an aspect worktree; open it with `hcdev tab`".

- aspect notes: `~/fun_repos/hobby-mimisbrunnr/notes/PROJECT/TOPIC/TASK/aspects/NAME/`
- task notes: `~/fun_repos/hobby-mimisbrunnr/notes/PROJECT/TOPIC/TASK/`

## 2. Read

`brief.md` and `aspect.json` in the aspect notes; `issue.md` and
`business-context.md` in the task notes when they exist. The task's recorded
context is this aspect's too — do not ask me again what is written there.

An `open` aspect becomes `"state": "active"` in `aspect.json` (Edit, keep every
other key). Any other state is a resume, usually a respawned tab, and is left
as it is:

- `pr` → `gh pr view <pr> --json state -q .state`. `MERGED` →
  `/task:build journal`. `CLOSED` → say the PR was closed unmerged, and stop for
  my decision. `OPEN` → step 6.
- `active` → carry on from what the notes and the worktree show.

If the brief has no answerable question or no describable change, ask me for
it — once, in one line.

A `kind` of `code` goes to step 6 now; anything else — `research`, or no kind
at all — continues with step 3.

## 3. Answer

- A question about a library, engine or API, which techniques exist, or how a
  mechanism works → `/research` (Skill tool). Its cards land in this aspect's
  `research/` dir.
- A question only the code can answer → read it, and **spike in this
  worktree**: a throwaway test, a benchmark, a prototype. The worktree is
  scratch. Commit to it only if keeping the spike is useful to the task;
  never push.
- Stay inside the brief's question and its "Out of scope". A second question
  that turns up goes into findings as an open question, not into this run.

## 4. Write `findings.md`

In the aspect notes dir, fixed shape:

```markdown
---
title: "TASK — Findings: NAME"
created: YYYY-MM-DD
tags:
  - aspect
  - findings
---

## Answer
<one paragraph; the answer itself, first>

## Evidence
- <a fact, and where it was verified: file:line, a research card, a link, a
  command and its output>

## Consequence for the task
<what the task can now decide or do — stated as an observation, never as a
decision made on my behalf>

## Open questions
- <what this run could not settle, or "none">

## Spikes
- <commits or files in the worktree worth looking at, or "none">
```

Every line in Evidence is something you verified. A claim you could not
verify goes under Open questions.

## 5. Finish

Run step 7. Print the Answer paragraph and the path to `findings.md`, and stop.
Follow-up questions from me in this tab continue the same aspect: update
`findings.md`, keep the state `reported`, and do not message the task again.

## 6. A code aspect

A code aspect in its task's queue — an entry in the task's `queue.json`, and
`auto: true` in the aspect's `build-task.json` — is not built here: `hauto`
runs it through `/task:auto`, and the task's session starts it in turn. Say
so in one line and stop.

Otherwise run `/task:build` (Skill tool) with no argument, in this worktree. It resolves
the aspect from `pwd`, takes `brief.md` as its brief and keeps its own state in
the aspect notes dir; its gates are mine as in any task. It sets
`aspect.json` `state` to `pr` with the PR's URL once the PR is open, and runs
`/task:aspect finish` once it has merged. Do not re-do what it owns.

## 7. Report to the task

1. **Record it** in `aspect.json` (Edit, keep every other key):
   - `state`: `done` for a code aspect, `reported` for a research one;
   - `outcome`: one paragraph on what came out, with the PR for code;
   - `open_items`: every item in `findings.md`'s Open questions, or the `## Open`
     of this aspect's newest journal entry, as `{"text": "...", "issue": null}`;
     `[]` when there are none. `issue` is `<project>/<topic>/<task>` once a task
     of its own covers the item, which makes it done from this task's view:
     whoever files or refreshes that issue sets it.
2. **Lift a research aspect's waits:** for each `<project>/<task>` in `blocks`,
   `nu -c "use ~/.config/nushell/hwt.nu *; hwt unblock --task <task> --project <project>"`.
   A code aspect's waits are on its PR, and lift when that merges.
3. **Tell the task.** `ListAgents`, and for each session whose name begins with
   TASK, its non-alphanumeric characters written as `-`, `SendMessage`:
   `Aspect NAME of TASK finished: read aspects/NAME/aspect.json. /task:build handed-off says what to do.`
   With no such session, say so in one line: the record is on disk, and the
   task's next session reads it.

Never commit to the notes repo. Never touch the task's own worktree.
