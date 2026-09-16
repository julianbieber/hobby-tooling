#!/usr/bin/env bash
# Self-test for `hcdev close`: builds a scratch task, starts a real zellij
# session, a victim process and an alacritty window in it, calls `hcdev close`
# from a deliberately adversarial command line (one whose own text contains the
# worktree path and the victim's path, so a name-based `pkill -f` would kill
# the caller before it could report success), and prints one PASS/FAIL line per
# property the acceptance checks grep for.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODULE="${1:-$SCRIPT_DIR/../dotfiles/nushell/hcdev.nu}"

PROJECT="hobby-tooling"
TOPIC="hcdev-close-selftest"
TASK="hcdev-close-selftest"

WT="$HOME/fun_worktrees/$PROJECT/$TOPIC/$TASK"
NOTES="$HOME/fun_repos/hobby-mimisbrunnr/notes/$PROJECT/$TOPIC/$TASK"
FAILED=0
SESSION=""

cleanup() {
    if [ -n "$SESSION" ]; then
        zellij kill-session "$SESSION" >/dev/null 2>&1 || true
        zellij delete-session --force "$SESSION" >/dev/null 2>&1 || true
    fi
    pkill -f "$WT/hcdev-close-victim" >/dev/null 2>&1 || true
    rm -rf "$WT" "$NOTES"
    rmdir "$HOME/fun_worktrees/$PROJECT/$TOPIC" 2>/dev/null || true
    rmdir "$HOME/fun_repos/hobby-mimisbrunnr/notes/$PROJECT/$TOPIC" 2>/dev/null || true
}
trap cleanup EXIT

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1 $2"; FAILED=1; }

mkdir -p "$WT"
mkdir -p "$NOTES"
printf '{"done":false,"blocked":null}' > "$NOTES/task.json"

cp /bin/sleep "$WT/hcdev-close-victim"

SESSION=$(nu -c "use $MODULE *; hcdev-session-name $PROJECT $TOPIC $TASK" | tr -d '\r\n')
if [ -z "$SESSION" ]; then
    echo "FAIL setup could not derive session name"
    exit 1
fi

# A victim whose cwd AND exe are both under the worktree.
( cd "$WT" && setsid -f "$WT/hcdev-close-victim" 600 >/dev/null 2>&1 )

setsid env TERM=xterm-256color script -q -c "zellij -s $SESSION" /dev/null >/dev/null 2>&1 &
for _ in $(seq 1 60); do
    if zellij list-sessions --no-formatting 2>/dev/null | grep -q "^$SESSION "; then
        break
    fi
    sleep 0.25
done
if ! zellij list-sessions --no-formatting 2>/dev/null | grep -q "^$SESSION "; then
    echo "FAIL setup zellij session $SESSION never appeared"
    exit 1
fi

if [ -n "${DISPLAY:-}" ]; then
    setsid alacritty --working-directory "$WT" --title "$SESSION" -e "$WT/hcdev-close-victim" 600 >/dev/null 2>&1 &
    WINDOW_UP=0
    for _ in $(seq 1 20); do
        if xdotool search --name "^$SESSION\$" >/dev/null 2>&1; then
            WINDOW_UP=1
            break
        fi
        sleep 0.25
    done
    if [ "$WINDOW_UP" -ne 1 ]; then
        fail window-gone "window never appeared to test against"
    fi
else
    fail window-gone "no DISPLAY"
fi

# The adversarial call: this snippet's own text names both the worktree path
# and the victim's full path, so a `pkill -f` on either string would kill this
# bash before it reached the echo. Reaching CALLER_ALIVE is the proof it did not.
SNIPPET="cd '$WT'; # victim $WT/hcdev-close-victim
nu -c \"use $MODULE *; hcdev close $TASK -p $PROJECT -t $TOPIC\"; rc=\$?
echo \"CALLER_ALIVE rc=\$rc\""

OUT=$(bash -c "$SNIPPET" 2>&1)
echo "$OUT"

if echo "$OUT" | grep -q "^CALLER_ALIVE rc=0$"; then
    pass caller-alive
else
    fail caller-alive "did not see CALLER_ALIVE rc=0: $OUT"
fi

if zellij list-sessions --no-formatting 2>/dev/null | grep -q "^$SESSION "; then
    fail session-gone "session $SESSION still listed"
else
    pass session-gone
fi

REMAINING=$(find /proc -mindepth 2 -maxdepth 2 -type l \( -name cwd -o -name exe \) \( -lname "$WT" -o -lname "$WT/*" \) 2>/dev/null)
if [ -z "$REMAINING" ]; then
    pass processes-gone
else
    fail processes-gone "still found: $REMAINING"
fi

if [ -n "${DISPLAY:-}" ]; then
    WINDOW_GONE=0
    for _ in $(seq 1 20); do
        if ! xdotool search --name "^$SESSION\$" >/dev/null 2>&1; then
            WINDOW_GONE=1
            break
        fi
        sleep 0.25
    done
    if [ "$WINDOW_GONE" -eq 1 ]; then
        pass window-gone
    else
        fail window-gone "window still present after close"
    fi
fi

SECOND=$(nu -c "use $MODULE *; hcdev close $TASK -p $PROJECT -t $TOPIC" 2>&1)
SECOND_RC=$?
if [ "$SECOND_RC" -eq 0 ]; then
    pass idempotent
else
    fail idempotent "second call exited $SECOND_RC: $SECOND"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "SELFTEST OK"
    exit 0
else
    echo "SELFTEST FAILED"
    exit 1
fi
