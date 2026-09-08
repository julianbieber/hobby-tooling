#!/usr/bin/env bash
# Runs cases.txt against policy_classify_command.
#
# Usage: run.sh [-v]

set -uo pipefail

HERE="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
# shellcheck source=../../../githooks/policy-lib.sh
. "$REPO/githooks/policy-lib.sh"

VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

pass=0
fail=0

while IFS=$'\t' read -r want kind file cmd; do
    case "$want" in ''|'#'*) continue ;; esac
    real=$(printf '%b' "$cmd")
    policy_classify_command "$real"
    ok=1
    [ "$POLICY_VERDICT" = "$want" ] || ok=0
    if [ "$kind" != "-" ] && [ "$POLICY_KIND" != "$kind" ]; then ok=0; fi
    if [ "$file" != "-" ] && [ "$POLICY_FILE_ARG" != "$file" ]; then ok=0; fi
    if [ "$ok" = 1 ]; then
        pass=$((pass + 1))
        [ "$VERBOSE" = 1 ] && printf '  ok   %s\n' "$cmd"
    else
        fail=$((fail + 1))
        printf '\033[31mFAIL\033[0m %s\n' "$cmd"
        printf '     want %-7s kind=%-24s file=%s\n' "$want" "$kind" "$file"
        printf '     got  %-7s kind=%-24s file=%s\n' \
            "$POLICY_VERDICT" "$POLICY_KIND" "$POLICY_FILE_ARG"
    fi
done < "$HERE/cases.txt"

if [ "$fail" -eq 0 ]; then
    printf '\033[32m✓\033[0m %s cases passed\n' "$pass"
else
    printf '\033[31m✗\033[0m %s passed, %s failed\n' "$pass" "$fail"
fi
[ "$fail" -eq 0 ]
