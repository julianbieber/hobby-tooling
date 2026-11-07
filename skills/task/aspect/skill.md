---
description: Work one aspect of a task in its own tab — a research aspect answered into findings.md, a code aspect built through /task:build to a merged PR against its base
allowed-tools: Skill, Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(rg:*), Bash(git status:*), Bash(git log:*), Bash(git diff:*), Bash(cargo:*), Bash(hresearch:*), WebSearch, Read, Write, Edit
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
`business-context.md`.

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

Set `"state": "active"` in `aspect.json` (Edit, keep every other key).

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

Set `"state": "reported"` in `aspect.json`. Print the Answer paragraph and the
path to `findings.md`, and stop. Follow-up questions from me in this tab
continue the same aspect: update `findings.md`, keep the state `reported`.

## 6. A code aspect

Run `/task:build` (Skill tool) with no argument, in this worktree. It resolves
the aspect from `pwd`, takes `brief.md` as its brief and keeps its own state in
the aspect notes dir; its gates are mine as in any task. It sets
`aspect.json` `state` to `pr` with the PR's URL once the PR is open, and
`done` once it has merged. Do not re-do what it owns.

Never commit to the notes repo. Never touch the task's own worktree.
