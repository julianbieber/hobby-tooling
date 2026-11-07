---
description: Add an aspect to the current task — a piece of the task's own work, research or code, with a self-contained brief — and start it now as a tab of this task's session, or keep it on the task to start later before the task is done
argument-hint: '<the work, in one sentence> [--later] [--code|--research] [--base <branch>]'
allowed-tools: AskUserQuestion, Bash(pwd:*), Bash(date:*), Bash(ls:*), Bash(test:*), Bash(nu -c:*), Bash(git status:*), Bash(git log:*), Read, Write, Edit
---

Add `$ARGUMENTS` to the current task as an **aspect**: work that is part of
this task, worked in a tab of this task's zellij session. Start it now, or add
it so it is not forgotten and start it later — before the task is finished,
since `hwt done` refuses a task with an unfinished aspect.

There are two kinds, and they start the same way:

- **research** — done when its answer is in `findings.md`;
- **code** — done when its PR has merged and its done-when holds.

Work that is **not part of this task** — planned for some later point — is a
task of its own: `/issue-create`, not this.

## Instructions

1. **Resolve the parent** from `pwd`: `~/fun_worktrees/PROJECT/TOPIC/TASK`
   (the task's worktree) or `~/fun_aspects/PROJECT/TOPIC/TASK/NAME` (one of its
   aspects — the new aspect is the task's, not that aspect's). Anywhere else,
   stop with "/aspect must run inside a task or aspect worktree". Notes dir:
   `~/fun_repos/hobby-mimisbrunnr/notes/PROJECT/TOPIC/TASK/`.

2. **The work.** `$ARGUMENTS`, or — when empty — ask me for it. Do not invent
   it. Rewrite it only to make it answerable: one question with what counts as
   an answer, or one change with what I should see once it is done.

3. **Kind, timing, base.** Take each from the arguments or my words:
   - kind: `--code` / `--research`, else what the work is — a question whose
     answer is a file is research, a change to the code is code;
   - timing: `--later`, or my saying it is for later, means do not start it;
   - base: `--base <branch>`, else the default branch. The aspect is cut from
     its base when started and its PR targets it.

   Whatever none of those settles, ask in one AskUserQuestion call.

4. **Name**: 2–4 words, lowercase, `-`-separated, ≤30 chars, unique among
   `notes/.../TASK/aspects/`.

5. **Add it** — notes dir, `aspect.json`, nothing checked out:

   ```
   nu -c "use ~/.config/nushell/hwt.nu *; hwt aspect new <name> --kind <kind> --goal '<one line>' [--base <branch>] --task TASK --project PROJECT"
   ```

6. **Write the brief** — overwrite `aspects/<name>/brief.md`. The tab's Claude
   has no memory of this conversation, so the brief is complete on its own:

   ```markdown
   ---
   title: "TASK — Aspect: <name>"
   created: YYYY-MM-DD
   tags:
     - aspect
     - <kind>
   ---

   ## Goal
   <research: the question, and what counts as an answer;
    code: the change, in what a player or caller can do afterwards>

   ## Done when
   <research: the answer is in findings.md;
    code: one observation someone can make, and what they should see>

   ## Why the task needs it
   <from the task's issue.md, business-context.md or this conversation,
   never invented>

   ## Already known
   <what this session has established or ruled out, with file:line or links>

   ## Where to look
   <files in the repo, crates, docs — the starting points, not a plan>

   ## Out of scope
   <what not to do, so the aspect stops>
   ```

   "Why the task needs it" follows the three-source rule in
   `~/.claude-hobby/CLAUDE.md`: if neither my words nor a verified fact gives
   the reason, ask me rather than writing one.

7. **Start it, unless it is for later** — check it out from the current tip of
   its base and open its tab:

   ```
   nu -c "use ~/.config/nushell/hwt.nu *; hwt aspect start <name> --task TASK --project PROJECT"
   ```

   A rebase conflict is aborted and reported; stop there and tell me.

8. **Report** in three lines: the aspect name and kind, its notes dir, and
   either that its tab is open (zellij's tab mode, or `hcdev tab <name>`,
   switches to it) or that it waits on the task's card as `open`, started later
   with `hwt aspect start <name>` or a click in hdash. Do not wait for it.

Never commit — neither the repo nor the notes repo.
