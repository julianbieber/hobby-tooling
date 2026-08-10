---
description: Deep context resume — loads journal, schemas, git history, and known issues
allowed-tools: Bash(git log:*), Bash(git branch:*), Bash(git -C:*), Bash(ls:*), Bash(pwd:*), Read
---

Resume work on the current task by front-loading all available context in a
single structured pass.

## Instructions

1. **Extract task ID** using these methods (in order of preference):

   a. **From worktree path**: If current directory is under `~/fun_worktrees/`,
   extract task from path:

   - Path format: `~/fun_worktrees/PROJECT/TOPIC/TASK`
   - Run `pwd` and parse: if path matches `~/fun_worktrees/*/*/`, the **second**
     subdirectory is the task ID — use it **verbatim, including any blurb
     suffix** (e.g. `gh-17-chunk-loading`, NOT the bare
     `gh-17`); the first is the topic
   - Also list sibling directories to see all projects in this task

   b. **From git branch**: Run `git branch --show-current`

   - Use the branch name **verbatim** as the task ID (skip `main`, `master`,
     `develop`). A `gh-<number>` pattern in it is the **GitHub issue number**,
     for `gh issue` calls only — never the notes-directory name.

   c. **From user input**: If above methods fail, ask which task to resume or
   list recent `~/fun_repos/hobby-mimisbrunnr/notes/` files

2. **Quick state** — Read just the `## Current State` section from
   `implementation.md` (between `## Current State` and `## Goal`). Print it
   immediately so the user sees status right away before full context loads.

3. **Full journal** — Read the task directory at
   `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task_id}/`: a. Read
   `implementation.md` for the goal, requirements, and plan b. Read the most
   recent daily file (`YYYY-MM-DD.md`, sorted by name descending) for latest
   progress and open items c. Read `insights.md` if it exists, for technical
   context and gotchas d. Legacy support: If only a single `{task_id}.md` file
   exists (old format), read that instead

4. **Schema snapshots** — Check for pre-cached schemas: a. Read
   `~/.claude/context/schemas/_last_refreshed.txt` if it exists

   - If the file exists, check the timestamp — warn if older than 7 days
   - If the file doesn't exist, note: "Schema cache not found. Run
     `~/tmp_repos/mcp_scripts/target/debug/refresh-schemas` to populate." b.
     (No pre-cached schema files for hobby projects — skip this step.) c.
     Read the project `README.md` / `AGENTS.md` if present

5. **Git history** — For each repo in `~/fun_worktrees/{project}/{topic}/{task_id}/*/`:

   - Run `git -C <repo> log --oneline -10` to get recent commits
   - If not in a worktree, run `git log --oneline -10` on cwd instead

6. **Known issues** — Read `~/.claude-hobby/context/known-issues.md` if it exists.
   Note any entries relevant to the current task's domain.

7. **Summarize the context** for the user in this structure:

   - **Current State**: Status, focus, blockers, next (from step 2)
   - **Goal & Plan**: What the task aims to achieve (from implementation.md)
   - **Recent Progress**: What was done recently (from latest daily file)
   - **Open Items**: What remains (from latest daily file's Open section)
   - **Key Files**: List important files from implementation.md (listed, NOT
     read)
   - **Insights**: Relevant technical learnings (from insights.md)
   - **Git Commits**: Recent commit summaries per repo
   - **Schema Availability**: Which schemas are cached and how fresh they are
   - **Known Issues**: Any relevant patterns from known-issues.md

8. **Suggest next steps** — Based on the open items and current state, propose
   2-3 logical next actions for the user to choose from.
