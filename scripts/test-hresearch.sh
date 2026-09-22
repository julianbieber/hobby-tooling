#!/usr/bin/env bash
# Self-test for `hresearch`: an isolated HOME with a fake task, two fixture
# sources fetched over file://, and one PASS/FAIL line per property the
# acceptance checks grep for. No network is used.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
HRESEARCH="$REPO/bin/hresearch"

TMP=""
FAILED=0

cleanup() {
    [ -n "$TMP" ] && rm -rf "$TMP"
}
trap cleanup EXIT

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1 $2"; FAILED=1; }

TMP="$(mktemp -d)"
export HOME="$TMP"
WORKTREE="$TMP/fun_worktrees/p/t/k"
NOTES="$TMP/fun_repos/hobby-mimisbrunnr/notes/p/t/k"
mkdir -p "$WORKTREE" "$NOTES" "$TMP/fixtures"

cat > "$TMP/fixtures/page.html" <<'EOF'
<html><body>
<p>A collider without a rigid&nbsp;body is <code>treated</code> as <code>static</code>.</p>
</body></html>
EOF

cat > "$TMP/fixtures/example.rs" <<'EOF'
fn setup() {
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(1.0, 1.0, 1.0),
    ));
}
EOF

PAGE_URL="file://$TMP/fixtures/page.html"
CODE_URL="file://$TMP/fixtures/example.rs"

# outside-task: `new` outside both WORKTREE_BASE and NOTES_BASE exits 2
if (cd "$TMP" && "$HRESEARCH" new "orphan question" >/dev/null 2>/tmp/hresearch-test-err); [ "$?" -eq 2 ]; then
    pass outside-task
else
    fail outside-task "expected exit 2"
fi

# new-dir: `new` creates a sources dir under the task's notes dir
DIR=$(cd "$WORKTREE" && "$HRESEARCH" new "how do I set up colliders with avian?")
case "$DIR" in
    "$NOTES/research/sources/"*) [ -d "$DIR" ] && [ -f "$DIR/question.txt" ] && pass new-dir || fail new-dir "dir or question.txt missing" ;;
    *) fail new-dir "unexpected dir: $DIR" ;;
esac

# fetch-html-text: the split, entity-escaped quote is findable after extraction
"$HRESEARCH" fetch "$DIR" "$PAGE_URL" >/dev/null
"$HRESEARCH" fetch "$DIR" "$CODE_URL" >/dev/null
if python3 -c "
import sys
text = open('$DIR/1.txt', encoding='utf-8').read().replace(' ', ' ')
sys.exit(0 if 'rigid body is treated as static' in text else 1)
"; then
    pass fetch-html-text
else
    fail fetch-html-text "quote not found after extraction"
fi

CARD_MD="$NOTES/research/$(basename "$DIR").md"

render_card() {
    "$HRESEARCH" render "$DIR" - <<< "$1"
}

OK_CARD='{
  "verdict": [{"text": "Add Collider + RigidBody to the entity", "src": 2}],
  "how": [{"text": "spawn with RigidBody and Collider", "src": 2}],
  "snippet": {"code": "commands.spawn((\n    RigidBody::Dynamic,\n    Collider::cuboid(1.0, 1.0, 1.0),\n));", "src": 2},
  "breaks": [{"quote": "A collider without a rigid body is treated as static.", "src": 1}]
}'

# render-ok: a verbatim card renders, and carries all five slot labels
if render_card "$OK_CARD" >/dev/null 2>&1 && [ -f "$CARD_MD" ]; then
    missing=""
    for label in verdict how snippet breaks source; do
        grep -q "$label" "$CARD_MD" || missing="$missing $label"
    done
    [ -z "$missing" ] && pass render-ok || fail render-ok "missing labels:$missing"
else
    fail render-ok "render failed"
fi

# render-rejects-paraphrase: a breaks quote not on the page is refused
PARAPHRASE_CARD='{
  "verdict": [{"text": "x"}], "how": [{"text": "y"}],
  "snippet": {"empty": "n/a"},
  "breaks": [{"quote": "This sentence never appears on the page.", "src": 1}]
}'
rm -f "$CARD_MD"
if render_card "$PARAPHRASE_CARD" >/dev/null 2>/tmp/hresearch-test-err; then
    fail render-rejects-paraphrase "render succeeded"
elif [ -f "$CARD_MD" ]; then
    fail render-rejects-paraphrase "md written despite rejection"
else
    pass render-rejects-paraphrase
fi

# render-rejects-written-code: snippet code not found in its source is refused
WRITTEN_CARD='{
  "verdict": [{"text": "x"}], "how": [{"text": "y"}],
  "snippet": {"code": "let x = totally_made_up_call();", "src": 2},
  "breaks": {"empty": "n/a"}
}'
rm -f "$CARD_MD"
if render_card "$WRITTEN_CARD" >/dev/null 2>/tmp/hresearch-test-err; then
    fail render-rejects-written-code "render succeeded"
elif [ -f "$CARD_MD" ]; then
    fail render-rejects-written-code "md written despite rejection"
else
    pass render-rejects-written-code
fi

# unsourced-marked: an item with no `src` renders as [unsourced]
UNSOURCED_CARD='{
  "verdict": [{"text": "Add Collider + RigidBody to the entity"}],
  "how": [{"text": "spawn with RigidBody and Collider", "src": 2}],
  "snippet": {"empty": "n/a"},
  "breaks": {"empty": "n/a"}
}'
render_card "$UNSOURCED_CARD" >/dev/null 2>&1
if grep -q "\[unsourced\]" "$CARD_MD"; then
    pass unsourced-marked
else
    fail unsourced-marked "no [unsourced] tag found"
fi

# empty-slot: an {"empty": reason} slot renders as one `empty: <reason>` row
if grep -q "empty: n/a" "$CARD_MD"; then
    pass empty-slot
else
    fail empty-slot "empty row not rendered"
fi

# snippet-line-range: a github blob url gets #L<a>-L<b> at the code's real lines
GH_DIR="$(dirname "$DIR")/gh-range"
mkdir -p "$GH_DIR"
cp "$DIR/question.txt" "$GH_DIR/question.txt"
python3 - "$DIR/sources.json" "$GH_DIR/sources.json" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
data = json.load(open(src))
for s in data:
    if s["id"] == 2:
        s["url"] = "https://github.com/Jondolf/avian/blob/main/crates/avian3d/examples/cubes.rs"
json.dump(data, open(dst, "w"))
PY
cp "$DIR/2.txt" "$GH_DIR/2.txt"
cp "$DIR/1.txt" "$GH_DIR/1.txt"
"$HRESEARCH" render "$GH_DIR" - <<< "$OK_CARD" >/dev/null 2>/tmp/hresearch-test-err
GH_MD="$NOTES/research/$(basename "$GH_DIR").md"
if grep -q "cubes.rs#L2-L5" "$GH_MD" 2>/dev/null; then
    pass snippet-line-range
else
    fail snippet-line-range "no #L2-L5 fragment found"
fi

# check-detects-hand-edit: a manually edited card.md fails `check`
rm -f "$CARD_MD"
render_card "$OK_CARD" >/dev/null 2>&1
echo "tampered" >> "$CARD_MD"
if "$HRESEARCH" check "$CARD_MD" >/dev/null 2>&1; then
    fail check-detects-hand-edit "check passed on a tampered file"
else
    pass check-detects-hand-edit
fi
render_card "$OK_CARD" >/dev/null 2>&1

# check-live: --live re-fetches and re-verifies against a fresh copy
if "$HRESEARCH" check --live --require snippet --require breaks "$CARD_MD" 2>&1 | grep -q "verbatim in"; then
    pass check-live
else
    fail check-live "no 'verbatim in' line printed"
fi

# require-empty-fails: --require on an empty slot fails the check
EMPTY_CARD='{
  "verdict": [{"text": "x"}], "how": [{"text": "y"}],
  "snippet": {"empty": "no code needed"},
  "breaks": {"empty": "no caveat found"}
}'
render_card "$EMPTY_CARD" >/dev/null 2>&1
if "$HRESEARCH" check --require snippet "$CARD_MD" >/dev/null 2>&1; then
    fail require-empty-fails "check passed despite empty required slot"
else
    pass require-empty-fails
fi

if [ "$FAILED" -eq 0 ]; then
    echo "SELFTEST OK"
    exit 0
else
    echo "SELFTEST FAILED"
    exit 1
fi
