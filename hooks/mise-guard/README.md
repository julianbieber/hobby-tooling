# mise-guard

**Event:** PreToolUse (matcher `Bash`)
**Install:** `~/.claude-hobby/hooks/mise-guard.sh`

Blocks bare invocations of mise-managed tools (cargo, node, python, …) when the
working directory has no `.mise.toml` but a subdirectory does, so the wrong tool
version cannot be used silently. Suggests `cd <dir> &&` or `mise exec -C <dir>`.

Verbatim copy — the hook keys off cwd, not the worktree layout. Mostly a no-op
here, since a hobby task dir is the repo root and usually carries its own
`.mise.toml`; kept for the case of a repo with per-crate tool pins.
