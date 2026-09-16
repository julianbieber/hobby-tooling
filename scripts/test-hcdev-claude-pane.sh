#!/usr/bin/env bash
# Self-test for `hcdev claude-pane` and the startup-tip config: launches a real,
# throwaway `hobby-auto` zellij session from this worktree's own config, layout
# and `bin/hauto`, and checks the resolver's answer against ground truth
# established independently (dumping each pane's screen without focusing it,
# rather than going through the code under test).
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
MODULE="${1:-$REPO/dotfiles/nushell/hcdev.nu}"
CONFIG="$REPO/dotfiles/zellij/config.kdl"
LAYOUT="$REPO/dotfiles/zellij/layouts/hobby-auto.kdl"

PROJECT="hobby-tooling"
TOPIC="claude-pane-selftest"
TASK="claude-pane-selftest"

WT="$HOME/fun_worktrees/$PROJECT/$TOPIC/$TASK"
NOTES="$HOME/fun_repos/hobby-mimisbrunnr/notes/$PROJECT/$TOPIC/$TASK"
CONTROL_CONFIG=""
FAILED=0
SESSION=""
CONTROL_SESSION=""

cleanup() {
    for s in "$SESSION" "$CONTROL_SESSION"; do
        if [ -n "$s" ]; then
            zellij kill-session "$s" >/dev/null 2>&1 || true
            zellij delete-session --force "$s" >/dev/null 2>&1 || true
        fi
    done
    rm -rf "$WT" "$NOTES"
    rmdir "$HOME/fun_worktrees/$PROJECT/$TOPIC" 2>/dev/null || true
    rmdir "$HOME/fun_repos/hobby-mimisbrunnr/notes/$PROJECT/$TOPIC" 2>/dev/null || true
    [ -n "$CONTROL_CONFIG" ] && rm -f "$CONTROL_CONFIG"
}
trap cleanup EXIT

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1 $2"; FAILED=1; }

wait_for() {
    local secs="$1"; shift
    local tries=$((secs * 4))
    local n=0
    while [ "$n" -lt "$tries" ]; do
        if "$@" >/dev/null 2>&1; then return 0; fi
        sleep 0.25
        n=$((n + 1))
    done
    return 1
}

session_listed() { zellij list-sessions --no-formatting 2>/dev/null | grep -q "^$1 "; }

mkdir -p "$WT" "$NOTES"
printf '{"done":false,"blocked":null}' > "$NOTES/task.json"
printf '{"phase":"done"}' > "$NOTES/build-task.json"

SESSION=$(nu -c "use $MODULE *; hcdev-session-name $PROJECT $TOPIC $TASK" | tr -d '\r\n')
if [ -z "$SESSION" ]; then
    echo "FAIL setup could not derive session name"
    exit 1
fi

( cd "$WT" && setsid env TERM=xterm-256color PATH="$REPO/bin:$PATH" \
    script -q -c "zellij --config $CONFIG -s $SESSION -n $LAYOUT" /dev/null >/dev/null 2>&1 & )

if ! wait_for 20 session_listed "$SESSION"; then
    echo "FAIL setup zellij session $SESSION never appeared"
    exit 1
fi
if ! wait_for 20 test -s "$NOTES/claude-pane.json"; then
    echo "FAIL setup claude-pane.json never appeared"
    exit 1
fi

# Ground truth, established without going through hcdev claude-pane: the pane
# whose screen carries hauto's own "is done" line is the Claude pane.
PANES_JSON=$(zellij --session "$SESSION" action list-panes -a --json 2>/dev/null)
TERMINAL_IDS=$(echo "$PANES_JSON" | jaq -r '.[] | select(.is_plugin == false) | .id')
CLAUDE_ID=""
for id in $TERMINAL_IDS; do
    if zellij --session "$SESSION" action dump-screen --pane-id "terminal_$id" 2>/dev/null | grep -q "hauto:"; then
        CLAUDE_ID="terminal_$id"
        break
    fi
done
if [ -z "$CLAUDE_ID" ]; then
    echo "FAIL setup could not establish ground truth: no pane shows hauto's output"
    exit 1
fi

ID=$(nu -c "use $MODULE *; hcdev claude-pane $TASK -p $PROJECT -t $TOPIC" 2>&1 | tr -d ' \r\n')

if echo "$ID" | grep -qE '^terminal_[0-9]+$'; then
    pass resolve
else
    fail resolve "did not print a bare pane id: $ID"
fi

if [ "$ID" = "$CLAUDE_ID" ]; then
    pass pane-is-claude
else
    fail pane-is-claude "resolved $ID, ground truth is $CLAUDE_ID"
fi

OTHER_ID=""
for id in $TERMINAL_IDS; do
    if [ "terminal_$id" != "$CLAUDE_ID" ]; then OTHER_ID="terminal_$id"; fi
done
if [ -n "$OTHER_ID" ] && [ "$ID" != "$OTHER_ID" ]; then
    pass pane-not-editor
else
    fail pane-not-editor "resolver's id ($ID) matches the non-Claude pane ($OTHER_ID)"
fi

RECORDED_SESSION=$(jaq -r '.session' "$NOTES/claude-pane.json" 2>/dev/null)
RECORDED_PANE=$(jaq -r '.pane_id' "$NOTES/claude-pane.json" 2>/dev/null)
if [ "$RECORDED_SESSION" = "$SESSION" ] && [ "$RECORDED_PANE" = "$CLAUDE_ID" ]; then
    pass recorded-id
else
    fail recorded-id "claude-pane.json has session=$RECORDED_SESSION pane_id=$RECORDED_PANE, expected $SESSION / $CLAUDE_ID"
fi

if echo "$PANES_JSON" | jaq -e '[.[] | select(.plugin_url == "about")] | length == 0' >/dev/null 2>&1; then
    CLIENTS=$(zellij --session "$SESSION" action list-clients 2>/dev/null || true)
    if ! echo "$CLIENTS" | grep -q "zellij:about"; then
        pass no-startup-tip
    else
        fail no-startup-tip "list-clients shows the client focused on zellij:about: $CLIENTS"
    fi
else
    fail no-startup-tip "an 'about' plugin pane is present"
fi

# Negative control: without show_startup_tips, the same launch does show the tip
# — proves the check above can actually detect it, rather than finding nothing.
CONTROL_CONFIG=$(mktemp "${TMPDIR:-/tmp}/hcdev-claude-pane-control-XXXXXX.kdl")
sed '/show_startup_tips/d' "$CONFIG" > "$CONTROL_CONFIG"
# Clamped the same way `hcdev-session-name` clamps every other session name —
# appending unclamped to $SESSION can overrun the socket-path budget zellij
# itself enforces (it rejects the name outright rather than truncating it).
CONTROL_SESSION=$(nu -c "use $MODULE *; hcdev-clamp-session-name '${SESSION}-control'" | tr -d '\r\n')

( cd "$WT" && setsid env TERM=xterm-256color PATH="$REPO/bin:$PATH" \
    script -q -c "zellij --config $CONTROL_CONFIG -s $CONTROL_SESSION -n $LAYOUT" /dev/null >/dev/null 2>&1 & )

if wait_for 20 session_listed "$CONTROL_SESSION"; then
    sleep 1 # let the tip plugin finish loading
    CONTROL_PANES=$(zellij --session "$CONTROL_SESSION" action list-panes -a --json 2>/dev/null)
    if echo "$CONTROL_PANES" | jaq -e '[.[] | select(.plugin_url == "about")] | length > 0' >/dev/null 2>&1; then
        pass tip-detectable
    else
        fail tip-detectable "control session (no show_startup_tips) shows no 'about' pane either"
    fi
else
    fail tip-detectable "control session $CONTROL_SESSION never appeared"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "SELFTEST OK"
    exit 0
else
    echo "SELFTEST FAILED"
    exit 1
fi
