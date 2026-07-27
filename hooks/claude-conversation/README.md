# claude-conversation

**Event:** `PostToolUse` (all tools), `Stop`
**Install:** `~/.claude-hobby/hooks/claude-conversation.sh` (`just install-hooks`)
**Viewer:** `bin/hx-claude` → `~/.local/bin` (`just install-conversation`), as `$EDITOR`
**Dependencies:** `jaq`, and `hx` for the viewer

Renders the live Claude Code conversation into a Markdown file so **ctrl-g** can
show it. ctrl-g opens `$EDITOR` on the prompt buffer; with `EDITOR=hx-claude`
that becomes Helix with the conversation so far in a vsplit beside it.

## The pair

- **`hook.sh`** — the dumper. Reads `transcript_path` from the hook's stdin JSON
  and rebuilds a clean, self-contained conversation:

  - user prompts (`## 🧑 User`), assistant prose (`## 🤖 Claude`);
  - every tool call with its real input — Bash commands verbatim in a ```` ```bash ````
    block, other tools as input JSON with long string values truncated at 500
    chars;
  - each call's result underneath (`_result:_` / `_result (error):_`), truncated
    at 800 chars;
  - consecutive same-role turns merged under one header; injected noise (system
    reminders, command tags, caveats, meta lines) skipped.

  The file is **overwritten** each call, never appended, and capped at the newest
  2000 lines. It always exits 0 and never blocks the tool.

- **`../../bin/hx-claude`** — the `$EDITOR` wrapper. ctrl-g invokes it with a temp
  file holding the prompt buffer; it opens that in Helix with the dump vsplit
  beside it, positioned on its last line. The conversation is passed first so the
  prompt buffer is the split left focused. With no dump yet, it's plain `hx` on
  the prompt buffer — an empty placeholder split would only be in the way.

Set `$EDITOR` yourself; it lives in the shell profile, not in this repo:

```nu
$env.EDITOR = "hx-claude"   # ~/.config/nushell/env.nu
```

`just doctor` warns when it doesn't point here, because a dump nothing opens is
just as invisible as no dump at all.

## Per-session keying

Both derive the same path, `/tmp/claude-conversation-<key>.md`, where `<key>` is
the sanitized `$ZELLIJ_SESSION_NAME` — so ctrl-g in one pane opens that pane's
conversation, not a neighbour's. Outside zellij it falls back to an `md5(PWD)`
hash.

**That derivation is the entire contract between the two scripts.** Change it in
one and the other silently opens a different file.

Neither name is `hobby-`-prefixed. `hx-claude` keeps its upstream name because
`$EDITOR` is one variable shared by both accounts — a second name would isolate
nothing. Installing from here replaces any link from the work checkout, which is
harmless: the script is account-agnostic, and `hcdev-…` / `cdev-…` session names
already give the two accounts disjoint keys inside the shared
`/tmp/claude-conversation-*.md` namespace.

## Freshness

The dump is current as of the last completed tool call, and ctrl-g re-opens it
each time, so each ctrl-g shows the conversation as of then. Helix has no file
watcher, so `:reload` is what refreshes a view left open across a turn.

## Testing it without a session

```bash
echo '{"transcript_path":"'"$HOME"'/.claude-hobby/projects/<proj>/<session>.jsonl"}' \
  | ~/.claude-hobby/hooks/claude-conversation.sh
bat "/tmp/claude-conversation-${ZELLIJ_SESSION_NAME:-$(printf '%s' "$PWD" | md5sum | cut -c1-8)}.md"
```

## Configuration

- **`CLAUDE_DUMP_DEBUG`** — unset by default. Set to `1` to log every invocation
  (key, transcript path, jaq stderr, `wrote=` line) to
  `/tmp/claude-conversation-<key>.log`. Off sends jaq stderr to `/dev/null`.
- The result cap (800 chars), argument cap (500 chars) and line cap (2000) are
  literals in `hook.sh`.

## Known limitations

- The dump refreshes on `Stop` and `PostToolUse`, so one read mid-tool-call shows
  the conversation as of the last completed call.
- `_result (error):_` tracks the tool framework's `is_error` flag, not shell exit
  codes — a Bash command that exits non-zero renders as a normal result, with the
  exit code visible in its output.
- The 2000-line cap is line-based, so once it overflows the file can start
  mid-turn or mid-code-block. The newest content at the bottom is always intact.
