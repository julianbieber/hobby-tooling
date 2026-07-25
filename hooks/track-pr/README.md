# track-pr

**Event:** PostToolUse (matcher `Bash`)
**Install:** `~/.claude-hobby/hooks/track-pr.sh`

When a `gh pr create` succeeds inside a hobby task, appends the PR to
`notes/<project>/<topic>/<task>/prs.json` so `hwt-overview` can show live status.

Deduplicates by URL. Effect-only: never blocks, never commits the notes repo.

Ported from the work hook; the only substantive change is the three-level path
parse (`~/fun_worktrees/PROJECT/TOPIC/TASK`).
