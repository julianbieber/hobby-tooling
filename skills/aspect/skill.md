---
description: Carve a research question out of the current task into a research aspect — its own worktree, a self-contained brief, and a zellij tab in this task's session whose Claude answers it into findings.md
argument-hint: '<the question, in one sentence>'
allowed-tools: Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(test:*), Bash(nu -c:*), Bash(git status:*), Bash(git log:*), Read, Write, Edit
---

Carve `$ARGUMENTS` out of the current task as a **research aspect**: a part of
this task, not a task of its own, worked in a tab of this task's zellij session
by a Claude that answers the question into `findings.md` and stops. The parent
reads the findings; nothing merges.

A research aspect is for a question whose answer the task depends on and which
would otherwise be researched inline, filling this session's context with
sources, spikes and dead ends. If the request is really a piece of
implementation, say so and stop — that is a subtask (`/subtask`), not this.

## Instructions

1. **Resolve the parent** from `pwd`: `~/fun_worktrees/PROJECT/TOPIC/TASK`, the
   task directory is the worktree. Anywhere else, stop with "/aspect must run
   inside a task worktree". Notes dir:
   `~/fun_repos/hobby-mimisbrunnr/notes/PROJECT/TOPIC/TASK/`.

2. **Refuse under an unattended run.** If that notes dir's `build-task.json`
   has `"auto": true`, stop: an unattended task is one session, and a tab
   nobody watches has a gate nobody answers.

3. **The question.** `$ARGUMENTS`, or — when empty — ask me for it. Do not
   invent one. Rewrite it only to make it answerable: one question, with what
   would count as an answer.

4. **Name**: 2–4 words, lowercase, `-`-separated, ≤30 chars, unique among
   `notes/.../TASK/aspects/`.

5. **Create it** — worktree on `TASK@<name>` from the task's branch, notes dir,
   `aspect.json`:

   ```
   nu -c "use ~/.config/nushell/hwt.nu *; hwt aspect new <name> --goal '<question>'"
   ```

6. **Write the brief** — overwrite `aspects/<name>/brief.md`. The tab's Claude
   has no memory of this conversation, so the brief is complete on its own:

   ```markdown
   ---
   title: "TASK — Aspect: <name>"
   created: YYYY-MM-DD
   tags:
     - aspect
     - research
   ---

   ## Question
   <the question, and what counts as an answer>

   ## Why the task needs it
   <the decision in the task that waits on this answer — from the task's
   issue.md, business-context.md or this conversation, never invented>

   ## Already known
   <what this session has established or ruled out, with file:line or links>

   ## Where to look
   <files in the repo, crates, docs — the starting points, not a plan>

   ## Out of scope
   <what not to answer, so the aspect stops>
   ```

   "Why the task needs it" follows the three-source rule in
   `~/.claude-hobby/CLAUDE.md`: if neither my words nor a verified fact gives
   the reason, ask me rather than writing one.

7. **Open the tab**: `nu -c "use ~/.config/nushell/hcdev.nu *; hcdev tab <name>"`.

8. **Report** in three lines: the aspect name, its notes dir, and that the tab
   is open (zellij's tab mode, or `hcdev tab <name>`, switches to it). Do not
   wait for the findings — they arrive in `findings.md` and on the hdash card.

Never commit — neither the repo nor the notes repo.
