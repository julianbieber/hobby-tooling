#!/bin/bash
# Rebuilds a readable, per-zellij-session Markdown view of the current Claude
# conversation from the live transcript. Registered on Claude Code's Stop and
# PostToolUse events — receives the hook JSON on stdin.
#
# The companion `bin/hx-claude` is the $EDITOR wrapper ctrl-g opens, and puts this
# file in a vsplit beside the prompt buffer. Both derive the file path
# identically; that derivation is the contract between them, so a change here
# needs the same change there.

input=$(cat)
transcript=$(echo "$input" | jaq -r '.transcript_path // empty' 2>/dev/null)
event=$(echo "$input" | jaq -r '.hook_event_name // empty' 2>/dev/null)
# At Stop, the final assistant text is often not yet flushed to the transcript,
# so the conversation would lag by one message. The Stop payload carries it in
# last_assistant_message; fold it back in below.
last_msg=$(echo "$input" | jaq -r '.last_assistant_message // empty' 2>/dev/null)

# Session-scoped key: prefer the zellij session (so ctrl-g in one pane opens that
# pane's conversation), fall back to a per-directory hash outside zellij.
#
# hcdev names its sessions `hcdev-<project>-<topic>-<task>`, so a hobby dump can
# never land on the same path as a work `cdev-…` one even though both accounts
# use the /tmp/claude-conversation-*.md namespace. That is deliberate: one
# viewer reads either account's conversation, and nothing has to be prefixed.
if [ -n "$ZELLIJ_SESSION_NAME" ]; then
    key=$(printf '%s' "$ZELLIJ_SESSION_NAME" | tr -c 'A-Za-z0-9._-' '_')
else
    key=$(printf '%s' "$PWD" | md5sum | cut -c1-8)
fi
conv="/tmp/claude-conversation-${key}.md"

# Debug: record every invocation so we can diagnose missing/misrouted writes.
# Off by default; set CLAUDE_DUMP_DEBUG=1 to enable (noisy under PostToolUse).
# Keyed like the dump itself, so two sessions debugging at once don't interleave.
logfile=/dev/null
[ -n "$CLAUDE_DUMP_DEBUG" ] && logfile="/tmp/claude-conversation-${key}.log"
if [ -n "$CLAUDE_DUMP_DEBUG" ]; then
{
  echo "=== $(date -Iseconds) ==="
  echo "ZELLIJ_SESSION_NAME=${ZELLIJ_SESSION_NAME:-<unset>}"
  echo "key=$key"
  echo "transcript=${transcript:-<none>}"
} >> "$logfile"
fi

[ -z "$transcript" ] && exit 0
[ -f "$transcript" ] || exit 0

# Render the transcript JSONL into a clean conversation: real user prompts,
# assistant prose, and compact tool-call markers. Consecutive same-role turns
# are merged so each speaker block gets a single header. Injected content
# (system reminders, command tags, caveats) and meta lines are skipped.
# Keep only the newest 2000 lines so the file stays bounded; pipefail ensures a
# jaq failure aborts before mv, rather than clobbering a good file with empty.
set -o pipefail
jaq -rs --arg event "$event" --arg lam "$last_msg" '
  # Map every tool_use_id to its result text (results arrive in the following
  # user message as tool_result items) so each call can show what it returned.
  (reduce (.[]
            | select(.type=="user")
            | (.message.content? // empty)
            | if type=="array" then .[] else empty end
            | select(.type=="tool_result")) as $tr
          ({};
           .[$tr.tool_use_id] = {
             text: ($tr.content
                    | if type=="string" then .
                      else [ .[]? | select(.type=="text") | .text ] | join("\n") end),
             err: ($tr.is_error // false)
           })) as $results
  | [ .[]
    | select((.isMeta // false) | not)
    | if .type=="user" and .message.role=="user" then
        (.message.content
          | if type=="string" then .
            else [ .[] | select(.type=="text") | .text ] | join("\n") end) as $t
        | select(
            ($t|gsub("\\s";"")) != ""
            and (($t|ltrimstr(" ")|startswith("<"))|not)
            and (($t|startswith("Caveat:"))|not))
        | {role:"user", text:$t}
      elif .type=="assistant" then
        ([ .message.content[]
            | if .type=="text" then .text
              elif .type=="tool_use" then
                ((.input // {}) as $i
                 | (if .name == "Bash" then
                      "_⚙️ Bash_\n\n```bash\n" + ($i.command // "") + "\n```"
                    else
                      # Truncate long string values (file contents, edit bodies)
                      # so the call stays readable.
                      ($i | walk(if type=="string" and (length > 500)
                                 then .[0:500] + "…" else . end)) as $i
                      | "_⚙️ " + .name + "_\n\n```json\n" + ($i | tojson) + "\n```"
                    end) as $call
                 | ($results[.id]) as $r
                 | $call
                   + (if ($r != null) and (($r.text | gsub("\\s";"")) != "")
                      then "\n\n" + (if $r.err then "_result (error):_" else "_result:_" end)
                           + "\n\n```\n"
                           + ($r.text | if (length > 800) then .[0:800] + "\n…[truncated]" else . end)
                           + "\n```"
                      else "" end))
              else empty end ] | join("\n\n")) as $a
        | select(($a|gsub("\\s";"")) != "")
        | {role:"assistant", text:$a}
      else empty end ]
  | reduce .[] as $m ([];
      if (length>0 and .[-1].role==$m.role)
      then .[0:-1] + [{role:$m.role, text:(.[-1].text + "\n\n" + $m.text)}]
      else . + [$m] end)
  # At Stop the transcript may not yet hold the final assistant text. If so,
  # fold last_assistant_message into (or onto) the trailing assistant turn so
  # the conversation never lags behind the response just shown.
  | ($lam | gsub("^\\s+|\\s+$";"")) as $lamt
  | (if $event == "Stop" and $lamt != "" then
       if (length>0) and (.[-1].role=="assistant") and (.[-1].text | contains($lamt))
       then .
       elif (length>0) and (.[-1].role=="assistant")
       then .[0:-1] + [{role:"assistant", text:(.[-1].text + "\n\n" + $lamt)}]
       else . + [{role:"assistant", text:$lamt}]
       end
     else . end)
  | .[]
  | (if .role=="user" then "\n## 🧑 User\n\n" else "\n## 🤖 Claude\n\n" end) + .text
' "$transcript" 2>>"$logfile" | tail -n 2000 > "${conv}.tmp" \
  && mv "${conv}.tmp" "$conv"

[ -n "$CLAUDE_DUMP_DEBUG" ] && echo "wrote=$conv" >> "$logfile"

exit 0
