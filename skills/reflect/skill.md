---
description: Review AI task journals for recurring issues and workflow improvements
allowed-tools: Bash(ls:*), Bash(find:*), Bash(wc:*), Bash(pwd:*), Bash(mkdir:*), Bash(rm:*), Bash(cd:*), Read, Write, Edit, Glob, Grep, Agent
---

Analyze AI task journals to find recurring patterns, wasted effort, and workflow
improvements. Then implement concrete fixes (known-issues entries, reusable
tools, skill adjustments).

## Instructions

### Phase 1: Read journals

1. List all files under `~/fun_repos/hobby-mimisbrunnr/notes/` with
   `Glob **/*.md`
2. Use subagents (one per topic directory) to read all files in parallel. Each
   subagent should return per-file summaries with: file name, what it was about,
   key findings, actions taken, status.

### Phase 2: Identify patterns

Analyze the summaries for:

- **Repeated investigations**: Same alert/issue investigated multiple times with
  the same conclusion. These need known-issues entries.
- **Rediscovered procedures**: Investigation steps or queries rebuilt from
  scratch across tasks. These need persistent reusable tools in
  `~/tmp_repos/mcp_scripts/tools/`.
- **Lost action items**: Recommendations made in journals ("adjust threshold",
  "notify customer", "fix hook") that were never tracked. These need entries in
  `~/.claude-hobby/context/known-issues.md` Action Items.
- **Error-prone patterns**: Code patterns that caused bugs repeatedly (e.g.
  missing annotations, race conditions). These may need tests or architectural
  fixes.
- **Stale files**: Journal stubs with no real content that should be deleted.

### Phase 3: Present findings

Present a numbered list of findings to the user, grouped by category:

1. Recurring issues that should be in known-issues.md
2. Procedures to persist as reusable tools
3. Lost action items to track
4. Code/workflow patterns to fix
5. Stubs to delete

Ask the user which items to implement.

### Phase 4: Implement

For each approved item:

- **Known issues**: Edit `~/.claude-hobby/context/known-issues.md` — add/update
  entries with pattern, cause, action, and any open recommendations.
- **Reusable tools**: Create Rust CLI tools in
  `~/tmp_repos/mcp_scripts/tools/{name}/` following workspace conventions
  (Cargo.toml with workspace deps, `src/main.rs` using `db_mcp::*`, clap for
  args, JSON stdout). Build with `cargo +nightly build -p {name}`.
- **Action items**: Add to the Action Items checklist in
  `~/.claude-hobby/context/known-issues.md`.
- **Stubs**: Delete with `rm`.
- **Journal condensing**: If journals are verbose, use subagents to rewrite
  files shorter (remove session play-by-play, keep findings/decisions/status).
- **Skill/command adjustments**: Edit files in `~/.claude/commands/` or
  `~/.claude/plugins/`.

### Phase 5: Condense (optional)

If the user approves, condense verbose journal files:

- Use subagents per topic to read and rewrite files in parallel
- Remove: verbose logs, play-by-play narration, redundant context, large data
  tables, full code blocks
- Keep: root causes, key findings, decisions, actions, status, important
  commands/file paths
- Delete files that are pure stubs (title only, no real content)

## Key paths

- Journals: `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`
- Known issues: `~/.claude-hobby/context/known-issues.md`
- Reusable tools: `~/tmp_repos/mcp_scripts/tools/`
- Skills: `~/.claude/commands/`
- Agents: `~/.claude/plugins/local/mcp-scripter/agents/`
