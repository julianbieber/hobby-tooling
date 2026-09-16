#!/usr/bin/env bash
# Self-test for `hcoord ci`: builds the binary from this worktree, puts a fake `gh`
# ahead of the real one on PATH, drives it against eight fixture PRs, and prints a
# PASS/FAIL line per property the acceptance checks grep for. This is the run: nothing
# in this repo produces a real red check, so a faked `gh` boundary is what proves the
# classifier and the bound rather than asserting about them in isolation.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"

TMP=""
FAILED=0

cleanup() {
    [ -n "$TMP" ] && rm -rf "$TMP"
}
trap cleanup EXIT

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1 $2"; FAILED=1; }

cargo build --quiet --manifest-path "$REPO/tools/hcoord/Cargo.toml" || {
    echo "FAIL build cargo build failed"
    exit 1
}
HCOORD="$REPO/tools/hcoord/target/debug/hcoord"

TMP="$(mktemp -d)"
mkdir -p "$TMP/bin"

cat > "$TMP/bin/gh" <<'GH'
#!/usr/bin/env bash
set -u
args="$*"
case "$args" in
    *statusCheckRollup*) cat "$FIXTURE/rollup.json" ;;
    *baseRefName*) cat "$FIXTURE/pr.json" ;;
    *check-runs*) cat "$FIXTURE/base.json" ;;
    *"run view"*) cat "$FIXTURE/job.log" ;;
    *) exit 1 ;;
esac
GH
chmod +x "$TMP/bin/gh"

PR="https://github.com/acme/demo/pull/1"

write_fixture() {
    local dir="$TMP/fixtures/$1"
    local rollup="$2" base="${3:-}" log="${4:-}"
    mkdir -p "$dir"
    printf '%s' "$rollup" > "$dir/rollup.json"
    printf '{"url":"%s","baseRefName":"main"}' "$PR" > "$dir/pr.json"
    if [ -z "$base" ]; then
        base='{"check_runs":[{"name":"clippy","status":"completed","conclusion":"success"}]}'
    fi
    printf '%s' "$base" > "$dir/base.json"
    printf '%s' "${log:-note: nothing to see here}" > "$dir/job.log"
    echo "$dir"
}

failing_rollup() {
    printf '{"state":"OPEN","mergeable":"MERGEABLE","statusCheckRollup":[
        {"__typename":"CheckRun","name":"clippy","status":"COMPLETED","conclusion":"%s",
         "detailsUrl":"https://github.com/acme/demo/actions/runs/12/job/34"}]}' "$1"
}

run_ci() {
    FIXTURE="$1" PATH="$TMP/bin:$PATH" "$HCOORD" ci "$PR" "${@:2}"
}

check() {
    local name="$1" fixture="$2" field="$3" want="$4"
    shift 4
    local out
    out="$(run_ci "$fixture" "$@")"
    local got
    got="$(printf '%s' "$out" | jaq -r ".$field")"
    if [ "$got" = "$want" ]; then
        pass "$name"
    else
        fail "$name" "expected $field=$want, got: $out"
    fi
}

GREEN_EMPTY=$(write_fixture green-no-checks '{"state":"OPEN","mergeable":"MERGEABLE","statusCheckRollup":[]}')
check green-no-checks "$GREEN_EMPTY" outcome green

OWN_DIFF=$(write_fixture own-diff-fix "$(failing_rollup FAILURE)" "" "error: unused variable \`x\`")
check own-diff-fix "$OWN_DIFF" outcome fix
check own-diff-fix-case "$OWN_DIFF" case own-diff

NOT_MINE=$(write_fixture not-mine-blocked "$(failing_rollup FAILURE)" \
    '{"check_runs":[{"name":"clippy","status":"completed","conclusion":"failure"}]}' \
    "error: unused variable \`x\`")
check not-mine-blocked "$NOT_MINE" outcome blocked
check not-mine-blocked-case "$NOT_MINE" case not-mine

DECISION=$(write_fixture decision-blocked "$(failing_rollup FAILURE)" "" \
    "error: failed to select a version for \`bevy\`
error: Unable to update https://github.com/bevyengine/bevy?rev=abc123")
check decision-blocked "$DECISION" outcome blocked
check decision-blocked-case "$DECISION" case decision

FLAKE=$(write_fixture flake-rerun "$(failing_rollup CANCELLED)" "" "The runner has received a shutdown signal")
check flake-rerun "$FLAKE" outcome rerun
check flake-rerun-case "$FLAKE" case flake

check flake-exhausted "$FLAKE" outcome blocked --spent 3
check flake-exhausted-case "$FLAKE" case flake --spent 3

check bound-allows-three "$OWN_DIFF" outcome fix --spent 2
check bound-stops-at-three "$OWN_DIFF" outcome blocked --spent 3
check bound-stops-at-three-case "$OWN_DIFF" case own-diff --spent 3

if [ "$FAILED" -eq 0 ]; then
    echo "SELFTEST OK"
    exit 0
else
    echo "SELFTEST FAILED"
    exit 1
fi
