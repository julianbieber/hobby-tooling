---
description: Journal programming task progress to hobby-mimisbrunnr
allowed-tools: Bash(git branch:*), Bash(git status:*), Bash(git rev-parse:*), Bash(ls:*), Bash(pwd:*), Bash(mkdir:*), Read, Write, Edit
---

Document what was accomplished and what remains open for the current task.

## Instructions

01. **Determine the task name** (the notes-directory name) using these methods
    (in order of preference):

    a. **From worktree path**: If current directory is under `~/fun_worktrees/`,
    extract the task from the path:

    - Path format: `~/fun_worktrees/PROJECT/TOPIC/TASK`
    - Run `pwd` and parse: if path matches `~/fun_worktrees/*/*/`, the **third**
      subdirectory is the task name — use it **verbatim, including any blurb
      suffix**. Do NOT shorten it to a bare `gh-<number>`.
    - Example: `~/fun_worktrees/bevy_survivor/features/gh-42-enemy-waves` -> task
      is `gh-42-enemy-waves`, topic is `features`
    - Example:
      `~/fun_worktrees/wusel/perf/gh-17-chunk-loading`
      -> task is `gh-17-chunk-loading` (NOT `gh-17`), topic is
      `perf`

    b. **From git branch**: Run `git branch --show-current`

    - Use the branch name **verbatim** as the task name (skip `main`, `master`,
      `laptop`, `develop`).
    - A `gh-<number>` prefix (older tasks carry one) is part of the name and
      must never replace the full task name.

    c. **From recent notes**: Check
    `~/fun_repos/hobby-mimisbrunnr/notes/` for recently modified
    files if above methods fail

02. **Determine the task directory**:
    `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`
    (lowercase)

    - The directory name must match the worktree task directory **exactly** (the
      full third-subdir name, blurb and all) so journals and PRs line up
      with the right worktree.
    - The project and topic are the **first** and **second** subdirectories under
      `~/fun_worktrees/`; the task is the **third**.
    - If project/topic can't be determined, search
      `~/fun_repos/hobby-mimisbrunnr/notes/*/*/` for the task directory
    - Create the directory with `mkdir -p` if it doesn't exist

03. **Task directory structure** — each task is a directory with these files:

    ```
    notes/{project}/{topic}/{task_id}/
    ├── implementation.md   # Requirements, plans, architecture decisions
    ├── insights.md         # Technical learnings, discoveries, and caveats
    ├── YYYY-MM-DD.md       # Daily progress journals (one per day)
    └── ...
    ```

    Additionally, the **topic** may carry a shared knowledge file for multi-arm
    tasks: `notes/{project}/{topic}/shared.md` (see step 07).

04. **Daily journal file** (`YYYY-MM-DD.md` for today's date):

    - **If the file doesn't exist**, first **read the most recent previous daily
      file** (`YYYY-MM-DD.md` sorted descending, if any). Then create today's
      file:

      ```markdown
      ---
      title: "{task_id} — {today's date}"
      created: {YYYY-MM-DD}
      tags:
        - journal
      ---

      ## Done

      - {list accomplishments}

      ## Open

      - {list remaining items}
      - {unresolved item} *(carried from {YYYY-MM-DD})*

      ## Notes

      {any extra context, commands used, links, etc.}
      ```

      **Carry-forward rule:** every item in the previous daily's `## Open` (and
      `## Action Items`) that was not completed or explicitly dropped in this
      session is copied into today's `## Open` with a
      `*(carried from YYYY-MM-DD)*` marker (keep the original date when
      re-carrying). Resume reads only the newest daily — an open item that isn't
      carried forward is silently lost. If an item is deliberately dropped, move
      it to `## Done` as `dropped: {reason}` instead of deleting it.

    - **If the file exists**, update the Done/Open/Notes sections with new
      information from this session. Append new items, don't overwrite existing
      ones.

05. **Implementation file** (`implementation.md`):

    - **Create on first journal** if it doesn't exist:

      ```markdown
      ---
      title: "{task_id} — Implementation"
      created: {YYYY-MM-DD}
      tags:
        - implementation
      ---

      ## Current State
      **Status:** Not Started
      **Focus:** —
      **Blockers:** None
      **Next:** Define goal and requirements

      ## Goal

      {Brief description of the task goal — ask user if unclear}

      ## Requirements

      {Key requirements, acceptance criteria}

      ## Plan

      {High-level approach, architecture decisions}

      ## Key Files

      {Important files/paths involved}
      ```

    - **Update** when requirements change, plans evolve, or architecture
      decisions are made during the session.

06. **Insights file** (`insights.md`):

    - **Create** when there are technical learnings worth recording:

      ```markdown
      ---
      title: "{task_id} — Technical Insights"
      created: {YYYY-MM-DD}
      tags:
        - insights
      ---

      ## Caveats

      - {things a fresh session must not violate — footguns, fragile
        assumptions, "do not X"}

      ## Insights

      ### {short title of insight}

      {description of what was learned, why it matters, and when it's relevant}
      ```

    - **Append** new insights discovered during the session. These are things
      like:

      - Unexpected behavior found in APIs, libraries, or tools
      - Performance discoveries (what was slow, what fixed it)
      - Non-obvious gotchas or edge cases
      - Useful queries, commands, or patterns discovered

    - **Caveat routing rule (hard):** anything a fresh session *must not violate
      or should check before acting* — footguns, fragile assumptions,
      environment quirks, "do not X", "always Y before Z" — goes into the
      `## Caveats` list in `insights.md` (or `shared.md`, step 07). **Never only
      into a daily Notes section**: daily files older than the newest are not
      read on resume, so a caveat recorded only there has a one-day lifetime.
      Caveats are exempt from the trivia filter in step 08 — a gotcha discovered
      during dead-end debugging is still a caveat. Keep each caveat one line;
      drop caveats that stopped being true.

    - Don't create this file if there are no insights or caveats to record.

07. **Topic-shared knowledge file** (`notes/{project}/{topic}/shared.md`) — for tasks with
    sibling arms (several task dirs under one topic working the same overarching
    goal, e.g. `gh-17-*`):

    - For each new insight or caveat, ask: **does this hold for the whole task
      family, or just this arm?** Family-level → `shared.md`; arm-level → the
      task's `insights.md`. Especially route to `shared.md`:

      - **Ruled-out approaches** and why (what parallel arms most need from each
        other)
      - Cluster/environment conventions all arms must follow
      - Measurement/verification setup shared across arms
      - Facts about the target system that are arm-independent

    - **Create on first shared entry** if it doesn't exist:

      ```markdown
      ---
      title: "{topic} — Shared knowledge (all arms)"
      created: {YYYY-MM-DD}
      tags:
        - insights
        - shared
      ---

      ## Caveats

      - {family-wide footguns}

      ## Ruled out

      - {approach} — {why, which arm established it, date}

      ## Insights

      ### {short title}

      {arm-independent learning}
      ```

    - Skip this step entirely for single-arm tasks (no sibling task dirs sharing
      the goal).

08. **Summarize the conversation** to populate the daily file:

    - Review what was discussed, implemented, fixed, or explored
    - Note any blockers, questions, or next steps
    - Extract any technical insights into `insights.md`
    - Update `implementation.md` if the plan or requirements changed
    - **Update the `## Current State` section** in `implementation.md`: Replace
      everything between `## Current State` and `## Goal` with current status
      (Status, Focus, Blockers, Next — ~4 lines max). If the section doesn't
      exist (legacy files), insert it after the frontmatter `---` and before
      `## Goal`. Standing warnings do NOT belong here (they'd be squeezed out on
      the next update) — put them in `insights.md ## Caveats`.
    - **Extract action items**: If the session produced recommendations or
      follow-up actions that aren't part of the current task (e.g. "adjust alert
      threshold", "fix pre-commit hook", "notify customer"), add them to:
      - The daily journal's `## Action Items` section (create if needed, after
        Open)
      - `~/.claude-hobby/context/known-issues.md` Action Items section (for cross-task
        visibility)
    - Ask the user for clarification if the session context is unclear

    **Focus on medium and long-term impact.** Only journal items that matter for
    future sessions or other people. Skip trivial, transient work like:

    - Fixing compile errors, type errors, or import issues in undeployed code
    - Iterative debugging cycles that led nowhere or were resolved in-session
    - Routine refactors or formatting changes with no behavioral impact
    - Test fixes that were just keeping up with code changes

    **Do journal** things like:

    - Design decisions and why alternatives were rejected
    - New features, APIs, or integrations added
    - Schema changes, config changes, or infrastructure changes
    - Bugs found in deployed/shared code
    - Blockers, open questions, or decisions deferred to later
    - Changes to build pipelines, deployments, or shared tooling

    **Forcing question (always, last):** before finishing, answer explicitly —
    *"What would bite a fresh session that is not yet in `implementation.md`,
    `insights.md`, or `shared.md`?"* Write whatever the answer surfaces to the
    right file (caveats → step 06/07). Only then report done.

    **Long sessions:** if the session is long (context has been compacted or is
    close to it), don't wait for the end — journaling mid-session is safe: the
    skill appends and never overwrites. Early-session caveats are exactly what
    compaction eats first.

09. **Migration**: If an old single-file task exists (`{task_id}.md` instead of
    a directory), leave it as-is. Create the new directory structure alongside
    it for new entries.

10. If no task ID can be determined, ask the user for the task ID.

11. **Git:** The notes directory (`~/fun_repos/hobby-mimisbrunnr/`)
    is a git-backed repo. After writing/updating journal files, **do not run
    `git add` or `git commit`** — leave those decisions to the user. Just save
    the files.
