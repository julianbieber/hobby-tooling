#!/usr/bin/env bash
# Self-test for what a split task reads about its aspects and follow-ups. Runs in
# a throwaway HOME with a stand-in `gh` and `chobby`, and prints one PASS/FAIL line
# per property. Exits non-zero when any fails.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT
FAILED=0

export HOME="$SANDBOX/home"
export GIT_CONFIG_GLOBAL=/dev/null
BIN="$SANDBOX/bin"
export PATH="$BIN:$PATH"
NOTES="$HOME/fun_repos/hobby-mimisbrunnr/notes"
TASK_NOTES="$NOTES/p/t/parent"
FOLLOW="$NOTES/other/t/follow"
ASPECT="$TASK_NOTES/aspects/code-one"
ASPECT_WT="$HOME/fun_aspects/p/t/parent/code-one"
PR="https://example.test/o/r/pull/7"

mkdir -p "$BIN" "$ASPECT" "$ASPECT_WT" "$FOLLOW" "$HOME/fun_worktrees/p/t/parent" "$HOME/fun_repos/p/.git" "$HOME/fun_repos/other/.git"
echo '{"done":false,"blocked":null}' > "$TASK_NOTES/task.json"
echo '{"done":false,"blocked":null,"backlog":true}' > "$FOLLOW/task.json"

cat > "$BIN/gh" <<GH
#!/usr/bin/env bash
echo call >> "$SANDBOX/gh-calls"
cat "$SANDBOX/gh-state"
GH
cat > "$BIN/chobby" <<'CHOBBY'
#!/usr/bin/env bash
printf '%s\n' "$*"
CHOBBY
chmod +x "$BIN/gh" "$BIN/chobby"

hwt() { nu -n -c "use $REPO/dotfiles/nushell/hwt.nu *; $1"; }
check() {
    if [ "$2" = "$3" ]; then
        echo "PASS: $1"
    else
        echo "FAIL: $1 (wanted '$3', got '$2')"
        FAILED=1
    fi
}
gh_calls() { wc -l < "$SANDBOX/gh-calls" 2>/dev/null | tr -d ' ' ; }
blocked_state() { hwt 'hwt blocked | where task == "follow" | get 0.state'; }

echo OPEN > "$SANDBOX/gh-state"
hwt "hwt block --pr $PR --task follow --project other" >/dev/null
check "block --pr names another project's task and reads the state" "$(jaq -r '.blocked.pr_state' "$FOLLOW/task.json")" OPEN
check "block keeps the task's other keys" "$(jaq -r '.backlog' "$FOLLOW/task.json")" true
check "an open PR reads as blocked" "$(blocked_state)" blocked

echo MERGED > "$SANDBOX/gh-state"
check "a merged PR reads as ready" "$(blocked_state)" ready
check "the merged state is written to task.json" "$(jaq -r '.blocked.pr_state' "$FOLLOW/task.json")" MERGED
before=$(gh_calls)
blocked_state >/dev/null
check "a merged PR is not asked about again" "$(gh_calls)" "$before"

hwt 'hwt unblock --task follow --project other' >/dev/null
check "unblock names another project's task" "$(jaq -r '.blocked' "$FOLLOW/task.json")" null

echo CLOSED > "$SANDBOX/gh-state"
hwt "hwt block --pr $PR --task follow --project other" >/dev/null
check "a closed PR holds the block" "$(blocked_state)" blocked
before=$(gh_calls)
blocked_state >/dev/null
check "a closed PR is not asked about again" "$(gh_calls)" "$before"

aspect_json() { printf '%s' "$1" > "$ASPECT/aspect.json"; }
aspect_json "{\"kind\":\"code\",\"state\":\"pr\",\"pr\":\"$PR\"}"
echo MERGED > "$SANDBOX/gh-state"
check "aspect list shows the PR's live state" "$(hwt 'hwt aspect list --task parent --project p | get 0.pr_state')" MERGED
if hwt 'hwt done parent --project p' >/dev/null 2>&1; then r=done; else r=refused; fi
check "done refuses an aspect still at pr" "$r" refused

aspect_json '{"kind":"code","state":"done","outcome":"shipped","open_items":[{"text":"web","issue":null}]}'
check "aspect list shows the outcome" "$(hwt 'hwt aspect list --task parent --project p | get 0.outcome')" shipped
check "aspect list shows open items" "$(hwt 'hwt aspect list --task parent --project p | get 0.open_items.0.text')" web
hwt 'hwt done parent --project p' >/dev/null
check "done passes once every aspect is finished" "$(jaq -r '.done' "$TASK_NOTES/task.json")" true

for row in "open:/task:aspect" "active:--continue /task:aspect" "pr:--continue /task:aspect" "reported:--continue" "done:--continue" "closed:--continue"; do
    state="${row%%:*}"
    aspect_json "{\"state\":\"$state\"}"
    check "haspect on $state" "$(cd "$ASPECT_WT" && ZELLIJ_PANE_ID='' "$REPO/bin/haspect")" "${row#*:}"
done

hook_out() { printf '{"cwd":"%s"}' "$HOME/fun_worktrees/p/t/parent" | "$REPO/hooks/task-context/hook.sh"; }
case "$(hook_out)" in
    *"hwt aspect list"*) r=yes ;;
    *) r=no ;;
esac
check "the session-start context names the status rule" "$r" yes

exit "$FAILED"
