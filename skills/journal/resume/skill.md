---
description: Resume a task by delegating journal reading to a subagent that filters notes down to a focused, next-step context
allowed-tools: Bash(git branch:*), Bash(git -C:*), Bash(ls:*), Bash(pwd:*), Read, Agent, SendMessage, AskUserQuestion
---

Resume work on the current task. Instead of loading every journal file into this
session, delegate the heavy reading to a subagent that surveys the notes, helps
settle on the next step, and returns a **focused** package: the goal narrowed to
that step plus only the context that bears on it. This session stays lean.

**Optional argument (`$ARGUMENTS`)**: a next-step bias hint describing what the
user wants to work on (e.g. `/journal:resume continue the proxy pool work`). Use
it to pre-select / shortcut the "pick" step below. If empty, run the full survey
→ pick → focus flow.

## Instructions

### 1. Extract the task ID (in order of preference)

a. **From worktree path**: If the current directory is under `~/fun_worktrees/`, run
`pwd` and parse `~/fun_worktrees/PROJECT/TOPIC/TASK`. The **third**
component is the task ID — use it **verbatim, including any blurb suffix**
(e.g. `gh-17-chunk-loading`, NOT the bare `gh-17`); the first is the project,
the second the topic. Example: `~/fun_worktrees/bevy_survivor/features/gh-42-enemy-waves` ->
task `gh-42-enemy-waves`, topic `features`.

b. **From git branch**: Run `git branch --show-current`. Use the branch name
**verbatim** as the task ID (skip `main`, `master`, `laptop`, `develop`).

c. **From user input**: If in `~/fun_repos` directly, ask which task to resume or
list recent `~/fun_repos/hobby-mimisbrunnr/notes/` files.

If no task ID can be determined, ask the user for it.

### 2. Resolve the notes dir and worktree

- Notes dir:
  `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task_id}/`
- The worktree is `~/fun_worktrees/{project}/{topic}/{task_id}` — the task
  directory is itself the repo, so there is no repo list.
- List the notes dir contents: `ls <notes_dir>` (to check what exists).

### 3. Graceful-degrade gate (fresh / empty tasks)

If the notes dir **doesn't exist** or contains **no journal `.md` files** (only
`task.json` / `prs.json`, or nothing), do NOT spawn a subagent. Instead:

- **Read the issue (fallback)**: if the notes dir holds an `issue.md`, read it —
  the problem statement and its `## Progress` entries. If there is none, skip
  this — a hobby task often has no issue at all.
- Summarize what little exists and **ask the user how to proceed**.
- Stop here — the rest of this skill is for tasks with real journal history.

### 4. Survey subagent (read + shortlist)

Spawn ONE subagent to do the reading — keep the big files out of this session.

- Tool: `Agent`, `subagent_type: general-purpose`, `run_in_background: false`
  (synchronous — we need its result before asking the user).
- Give it: the absolute notes-dir path, the worktree path, and the bias hint (if
  any). Instruct it to:
  - Read `implementation.md` — especially `## Current State` (Status / Focus /
    Blockers / Next / "Carried from …" lines) and `## Goal`.
  - Read the newest 1–2 daily files (`YYYY-MM-DD.md`, sort by name descending) —
    focus on `## Open`, `## Action Items`, and `## Done`.
  - Read only the `### ` heading **index** of `insights.md` (the titles), NOT
    its body.
  - Read `task.json` (done / blocked) and `prs.json` (outstanding PRs).
  - Legacy: if only a single `{task_id}.md` exists (old format), read that.
  - **Return only** (keep it short — this lands in THIS session's context):
    1. A 2–3 sentence state briefing (where things stand right now).
    2. 2–4 **ranked** candidate next steps, each one line + a short rationale +
       the source (which file / section it came from).
  - If a bias hint was given, surface and rank the matching candidate(s) first.
  - Explicitly: NO file-content dumps, no long quotes.

Keep the subagent's ID/name — step 6 continues it.

### 5. Pick the next step (work with the user)

- If the bias hint clearly resolves to a single candidate, confirm it in one
  line and proceed to step 6 without a menu.
- Otherwise use `AskUserQuestion` to present the candidate next steps as
  options. The user can pick one or type a custom step via "Other".

### 6. Focus subagent (filter to the chosen step)

Continue the **same** subagent via `SendMessage` (it still holds the full
journal detail, so filtering is cheap — no re-read). Pass the chosen next step
and request a **focused context package** for this session:

- The goal narrowed to this step (1–2 sentences).
- Only the prior work / decisions / `### ` insight entries / `## Done` bullets
  that bear on this step — reference commit hashes and `[[wikilinks]]` rather
  than inlining full content.
- The specific files involved (from `## Key Files` + relevant
  dailies).
- Relevant commands / queries / gotchas for this step.
- Any outstanding PRs from `prs.json` relevant to this step.
- Explicitly omit everything unrelated to the chosen step.

**Fallback**: if continuing the same agent isn't possible, spawn a fresh
`general-purpose` agent with the notes-dir path and the chosen step (accepting
the cost of a re-read).

### 7. Present and start

Present the focused package to the user, restate the chosen next step, and
signal you're ready to begin work on it.
