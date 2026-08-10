#!/usr/bin/env bash
# PreToolUse hook: no message, body or comment reaches git or GitHub unless I
# typed it.
#
# TODO(jb-doc): why PreToolUse and not PermissionRequest, given
# hooks/edit-before-execute already opened an editor on these same commands
#
# TODO(jb-doc): why the hook denies and makes Claude re-issue the command
# against a file, rather than rewriting the command with updatedInput
#
# Input:  PreToolUse JSON on stdin { tool_name, tool_input: { command }, cwd }
# Output: nothing (allow), or a PreToolUse deny with instructions

set -uo pipefail

HERE="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
# shellcheck source=../../githooks/policy-lib.sh
. "$REPO/githooks/policy-lib.sh"

INPUT=$(cat)
[ "$(echo "$INPUT" | jaq -r '.tool_name // empty')" = "Bash" ] || exit 0

CMD=$(echo "$INPUT" | jaq -r '.tool_input.command // empty')
CWD=$(echo "$INPUT" | jaq -r '.cwd // empty')
[ -n "$CMD" ] || exit 0

policy_in_scope "${CWD:-$PWD}" || exit 0

GATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/hobby-authoring-gate"
mkdir -p "$GATE_DIR"

SENTINEL='----- 8< ----- everything below this line is discarded -----'

deny() {
    jaq -n --arg r "$1" '{
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "deny",
        permissionDecisionReason: $r
      }
    }'
    exit 0
}

# --- Pass 1: a command that only references already-authored files -----------
# TODO(jb-comment): what the .sha file is for, and what re-checking it prevents

referenced_files=$(printf '%s\n' "$CMD" \
    | grep -oE -- '(-F|--file|--body-file|--notes-file|--body-file=|--file=)[= ]?[^ "'"'"']+' \
    | sed -E 's/^(-F|--file|--body-file|--notes-file)[= ]?//' || true)

if [ -n "$referenced_files" ]; then
    all_approved=1
    while read -r f; do
        [ -n "$f" ] || continue
        case "$f" in "$GATE_DIR"/*) ;; *) all_approved=0; continue ;; esac
        if [ ! -f "$f" ] || [ ! -f "$f.sha" ]; then all_approved=0; continue; fi
        if [ "$(sha256sum "$f" | cut -d' ' -f1)" != "$(cat "$f.sha")" ]; then
            deny "The authored file $f changed after you were given it. Nothing may edit it — re-run the original command to reopen the editor."
        fi
    done <<< "$referenced_files"
    [ "$all_approved" = "1" ] && exit 0
fi

# --- Pass 2: classify --------------------------------------------------------
# KIND       label shown to me
# FILEFLAG   the flag Claude must re-issue with; empty means no file form exists
# NEEDS_AI   the buffer carries a mandatory `## AI use` section
# TEMPLATE   commit | markdown | pr

KIND=""; FILEFLAG=""; NEEDS_AI=0; TEMPLATE="markdown"

case "$CMD" in
    *"git commit"*)
        # TODO(jb-comment): why --amend --no-edit and -C are exempt
        case "$CMD" in
            *--no-edit*|*--reuse-message*|*" -C "*) exit 0 ;;
        esac
        KIND="commit message"; FILEFLAG="-F"; TEMPLATE="commit" ;;
    *"git merge"*-m*|*"git merge"*--message*)
        KIND="merge commit message"; FILEFLAG="-F"; TEMPLATE="commit" ;;
    *"git tag"*-m*|*"git tag"*--message*|*"git tag"*-a*)
        KIND="tag message"; FILEFLAG="-F"; TEMPLATE="commit" ;;
    *"git notes"*-m*)
        KIND="git note"; FILEFLAG="-F"; TEMPLATE="commit" ;;
    *"gh pr create"*)
        KIND="pull request"; FILEFLAG="--body-file"; NEEDS_AI=1; TEMPLATE="pr" ;;
    *"gh pr edit"*--body*)
        KIND="pull request body"; FILEFLAG="--body-file"; NEEDS_AI=1; TEMPLATE="pr" ;;
    *"gh pr comment"*)
        KIND="PR comment"; FILEFLAG="--body-file" ;;
    *"gh pr review"*)
        KIND="PR review"; FILEFLAG="--body-file" ;;
    *"gh issue create"*)
        KIND="issue"; FILEFLAG="--body-file"; TEMPLATE="titled" ;;
    *"gh issue edit"*--body*)
        KIND="issue body"; FILEFLAG="--body-file" ;;
    *"gh issue comment"*)
        KIND="issue comment"; FILEFLAG="--body-file" ;;
    *"gh issue close"*--comment*)
        KIND="issue closing comment"; FILEFLAG="" ;;
    *"gh release create"*)
        KIND="release notes"; FILEFLAG="--notes-file" ;;
    *"gh gist create"*)
        KIND="gist"; FILEFLAG="" ;;
    *"gh api"*)
        # TODO(jb-comment): why gh api is matched at all, and only on a write
        case "$CMD" in
            *-f\ body=*|*--field\ body=*|*-F\ body=*|*--input*)
                KIND="GitHub API post"; FILEFLAG="" ;;
            *) exit 0 ;;
        esac ;;
    *) exit 0 ;;
esac

if [ -z "$FILEFLAG" ]; then
    deny "A $KIND has no --*-file form, so it cannot be routed through the authoring gate. Run it yourself: type \`! <command>\` in the prompt, with text you wrote. Bevy AI policy norm 6: public writing is synthesized by me, not revised from yours."
fi

# --- Pass 3: build the buffer ------------------------------------------------
ID=$(date +%s)-$$
BUF="$GATE_DIR/$ID.txt"
DONE="$BUF.done"

facts() {
    printf '%s\n' "$SENTINEL"
    if [ "$TEMPLATE" = "pr" ] || [ "$TEMPLATE" = "titled" ]; then
        printf 'LINE 1 OF THIS BUFFER IS THE TITLE. It may not be empty.\n\n'
    fi
    printf 'Bevy AI policy https://github.com/bevyengine/bevy-website/pull/2551\n'
    printf '  norm 1  you own every line, and no machine attribution\n'
    printf '  norm 6  your words, written from scratch\n'
    [ "$NEEDS_AI" = "1" ] && printf '  norm 2  the AI use section is required and specific to this PR\n'
    printf '\n'
    printf 'branch: %s\n' "$(git -C "${CWD:-$PWD}" branch --show-current 2>/dev/null || echo '?')"
    printf '\nstaged:\n'
    git -C "${CWD:-$PWD}" diff --cached --stat 2>/dev/null | sed 's/^/  /'
    printf '\nunstaged:\n'
    git -C "${CWD:-$PWD}" diff --stat 2>/dev/null | sed 's/^/  /'
    if [ "$TEMPLATE" = "pr" ]; then
        printf '\ncommits on this branch:\n'
        git -C "${CWD:-$PWD}" log --oneline "$(git -C "${CWD:-$PWD}" rev-parse --abbrev-ref '@{upstream}' 2>/dev/null || echo 'HEAD~10')..HEAD" 2>/dev/null | sed 's/^/  /'
    fi
    # TODO(jb-comment): why a skill hands facts over through a file instead of
    # putting them in the command
    # TODO(jb-comment): why facts.md survives until the gate is passed
    if [ -s "$GATE_DIR/facts.md" ]; then
        printf '\n--- gathered for this PR ---\n'
        cat "$GATE_DIR/facts.md"
    fi
    printf '\nClaude proposed text in the command it tried to run. It is not shown\n'
    printf 'here on purpose: norm 6 asks for your words, not an edit of its words.\n'
}

# TODO(jb-comment): why a rejected buffer is offered back instead of a blank one
if [ -s "$GATE_DIR/seed.md" ]; then
    { cat "$GATE_DIR/seed.md"; printf '\n'; facts; } > "$BUF"
    TEMPLATE_SEEDED=1
else
    TEMPLATE_SEEDED=0
fi

case "$TEMPLATE" in
    commit)
        [ "$TEMPLATE_SEEDED" = "1" ] || { printf '\n\n'; facts; } > "$BUF" ;;
    pr)
        [ "$TEMPLATE_SEEDED" = "1" ] || {
            printf '\n\n'   # line 1 is the PR title
            printf '## Why\n\n\n'
            printf '## Expected behavior after merge\n\n\n'
            printf '## Verification\n\n\n'
            printf '## Scope / out of scope\n\n\n'
            printf '## AI use\n\n'
            printf 'STUB — replace this line. What the AI did, what you did, what you\n'
            printf 'verified yourself. Specific to this PR.\n\n'
            facts
        } > "$BUF" ;;
    titled)
        # TODO(jb-comment): why line 1 is the title here too
        [ "$TEMPLATE_SEEDED" = "1" ] || { printf '\n\n'; facts; } > "$BUF" ;;
    *)
        [ "$TEMPLATE_SEEDED" = "1" ] || { printf '\n\n'; facts; } > "$BUF" ;;
esac

# --- Pass 4: open it ---------------------------------------------------------
# TODO(jb-comment): why the ZELLIJ variables decide the spawn method
# TODO(jb-comment): why every spawn must have its stdout closed off — `zellij run`
# prints the pane name, and one stray line ahead of the JSON breaks the hook
if [ -n "${ZELLIJ:-}" ]; then
    zellij run --floating --close-on-exit --name "write: $KIND" -- \
        bash -c "${EDITOR:-hx} '$BUF' && touch '$DONE'" >/dev/null 2>&1
elif command -v alacritty >/dev/null 2>&1; then
    alacritty --title "write: $KIND" -e bash -c "${EDITOR:-hx} '$BUF' && touch '$DONE'" >/dev/null 2>&1 &
elif command -v kitty >/dev/null 2>&1; then
    kitty --title "write: $KIND" bash -c "${EDITOR:-hx} '$BUF' && touch '$DONE'" >/dev/null 2>&1 &
else
    deny "No terminal to open an editor in. Write the $KIND yourself: \`\$EDITOR $BUF\`, then re-run with $FILEFLAG $BUF."
fi

while [ ! -f "$DONE" ]; do sleep 0.2; done
rm -f "$DONE"

# --- Pass 5: validate --------------------------------------------------------
AUTHORED=$(sed "/^$(printf '%s' "$SENTINEL" | sed 's/[].[^$*\/-]/\\&/g')/,\$d" "$BUF")
TRIMMED=$(printf '%s' "$AUTHORED" | sed '/^[[:space:]]*$/d')

if [ -z "$TRIMMED" ]; then
    rm -f "$BUF"
    deny "Nothing was written, so the $KIND is not authored. Bevy AI policy norm 1: I own what I submit. Re-run when I am ready to write it."
fi

# TODO(jb-comment): why the checks below run on $AUTHORED and never on $BUF
if printf '%s\n' "$AUTHORED" | policy_find_attribution - >/dev/null; then
    deny "The text I wrote contains machine attribution. Bevy AI policy norm 1 bans it. What I wrote is still at $BUF — re-run to reopen it."
fi

# TODO(jb-comment): why this compares against the whole command instead of the
# text extracted from it
norm() { tr -d '[:space:]'; }
if [ "$(printf '%s' "$TRIMMED" | wc -c)" -gt 40 ] \
    && printf '%s' "$CMD" | norm | grep -qF -- "$(printf '%s' "$TRIMMED" | norm)"; then
    deny "The text in the buffer is the text you proposed in the command. Norm 6 asks me to synthesize it from scratch, not paste yours. What I wrote is still at $BUF — re-run."
fi

if [ "$NEEDS_AI" = "1" ]; then
    AI_SECTION=$(printf '%s\n' "$AUTHORED" | sed -n '/^## AI use/,$p' | tail -n +2 | sed '/^[[:space:]]*$/d')
    if [ -z "$AI_SECTION" ]; then
        deny "The '## AI use' section is missing or empty. Bevy AI policy norm 2 requires disclosure in the PR description, with context about how AI was used. It is not optional and it is not a trailer. What I wrote is still at $BUF — re-run."
    fi
    if printf '%s' "$AI_SECTION" | grep -q '^STUB'; then
        deny "The '## AI use' section is still the stub. Norm 2 wants a disclosure specific to this PR. What I wrote is still at $BUF — re-run."
    fi
fi

# --- Pass 6: hand back the approved file -------------------------------------
printf '%s\n' "$AUTHORED" > "$BUF"
sha256sum "$BUF" | cut -d' ' -f1 > "$BUF.sha"
rm -f "$GATE_DIR/facts.md" "$GATE_DIR/seed.md"

if [ "$TEMPLATE" = "pr" ] || [ "$TEMPLATE" = "titled" ]; then
    TITLE=$(head -n1 "$BUF")
    # TODO(jb-comment): why an empty first line has to be its own rejection
    if [ -z "$(printf '%s' "$TITLE" | tr -d '[:space:]')" ]; then
        cp "$BUF" "$GATE_DIR/seed.md"
        deny "Line 1 of the buffer is the title and it is empty. What I wrote is kept — re-run and the buffer comes back with it, so only the title line needs adding."
    fi
    BODY="$GATE_DIR/$ID.body"
    tail -n +2 "$BUF" > "$BODY"
    sha256sum "$BODY" | cut -d' ' -f1 > "$BODY.sha"
    deny "Written. Re-issue the command with my text, unchanged:

  --title \"$TITLE\" --body-file $BODY

Keep every other flag you had. Do not add, summarize or reword any part of it."
fi

deny "Written. Re-issue the command with my text, unchanged:

  $FILEFLAG $BUF

Drop any -m / --body / --message you had, keep every other flag. Do not add, summarize or reword any part of it."
