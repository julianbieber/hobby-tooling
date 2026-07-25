# tool-audit

**Event:** PreToolUse (all tools)
**Install:** `~/.claude-hobby/hooks/tool-audit.sh`

Appends a formatted record of every tool call to `~/.claude-hobby/tool-audit.log`.
Tail it in a side pane:

```bash
tail -f ~/.claude-hobby/tool-audit.log
```

Verbatim copy of the work hook except the log default, which points at the hobby
config dir so the two accounts do not interleave. Override with
`CLAUDE_TOOL_AUDIT_LOG`.
