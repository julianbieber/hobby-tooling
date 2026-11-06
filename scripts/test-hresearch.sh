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
export XDG_CACHE_HOME="$TMP/.cache"
WORKTREE="$TMP/fun_worktrees/p/t/k"
NOTES="$TMP/fun_repos/hobby-mimisbrunnr/notes/p/t/k"
mkdir -p "$WORKTREE" "$NOTES" "$TMP/fixtures"
cd "$WORKTREE"

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

CARD_MD="$NOTES/research/$(basename "$DIR").hresearch"

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
GH_MD="$NOTES/research/$(basename "$GH_DIR").hresearch"
if grep -q "cubes.rs#L2-L5" "$GH_MD" 2>/dev/null; then
    pass snippet-line-range
else
    fail snippet-line-range "no #L2-L5 fragment found"
fi

# check-detects-hand-edit: a manually edited card file fails `check`
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

MDIR=$(cd "$WORKTREE" && "$HRESEARCH" new "which techniques exist to simulate water flow")
"$HRESEARCH" fetch "$MDIR" "$PAGE_URL" >/dev/null
MATRIX_MD="$NOTES/research/$(basename "$MDIR").hresearch"

render_matrix() {
    "$HRESEARCH" render "$MDIR" - <<< "$1"
}

OK_MATRIX='{
  "shape": "matrix",
  "criteria": ["looks like", "cost", "effort"],
  "options": [
    {"name": "heightfield pipes", "cells": {
        "looks like": {"text": "rivers, pools"},
        "cost": {"text": "O(cells)"},
        "effort": {"text": "yes"},
        "precedent": {"src": 1}}},
    {"name": "SPH particles", "cells": {
        "looks like": {"text": "droplets"},
        "cost": {"text": "O(n log n)"},
        "effort": {"text": "small counts"},
        "precedent": {"none": true}}}
  ],
  "verdict": {"option": "heightfield pipes", "text": "start here"}
}'

# matrix-render-ok: a valid matrix renders the question, every criterion, verdict, and a footnote
if render_matrix "$OK_MATRIX" >/dev/null 2>&1 && [ -f "$MATRIX_MD" ]; then
    missing=""
    grep -qF "question: which techniques exist to simulate water flow" "$MATRIX_MD" || missing="$missing question-line"
    for label in "looks like" "cost" "effort" "precedent" "verdict"; do
        grep -q "^$label" "$MATRIX_MD" || missing="$missing $label"
    done
    grep -qF "[1] $PAGE_URL" "$MATRIX_MD" || missing="$missing footnote"
    [ -z "$missing" ] && pass matrix-render-ok || fail matrix-render-ok "missing:$missing"
else
    fail matrix-render-ok "render failed"
fi

# matrix-slug: the survey stopwords leave only the subject in the stem
case "$(basename "$MDIR")" in
    *-simulate-water-flow) pass matrix-slug ;;
    *) fail matrix-slug "unexpected stem: $(basename "$MDIR")" ;;
esac

# matrix-rejects-long-cell: a 5-word cell is refused, no md written
rm -f "$MATRIX_MD"
LONG_CELL_MATRIX='{
  "shape": "matrix",
  "criteria": ["looks like", "cost", "effort"],
  "options": [
    {"name": "heightfield pipes", "cells": {
        "looks like": {"text": "one two three four five"},
        "cost": {"text": "O(cells)"},
        "effort": {"text": "yes"},
        "precedent": {"none": true}}},
    {"name": "SPH particles", "cells": {
        "looks like": {"text": "droplets"},
        "cost": {"text": "O(n log n)"},
        "effort": {"text": "small counts"},
        "precedent": {"none": true}}}
  ],
  "verdict": {"option": "heightfield pipes", "text": "start here"}
}'
if render_matrix "$LONG_CELL_MATRIX" >/dev/null 2>/tmp/hresearch-test-err; then
    fail matrix-rejects-long-cell "render succeeded"
elif [ -f "$MATRIX_MD" ]; then
    fail matrix-rejects-long-cell "md written despite rejection"
else
    pass matrix-rejects-long-cell
fi

# matrix-rejects-missing-criterion: an option missing a criterion is refused
rm -f "$MATRIX_MD"
MISSING_CRIT_MATRIX='{
  "shape": "matrix",
  "criteria": ["looks like", "cost", "effort"],
  "options": [
    {"name": "heightfield pipes", "cells": {
        "looks like": {"text": "rivers, pools"},
        "cost": {"text": "O(cells)"},
        "precedent": {"none": true}}},
    {"name": "SPH particles", "cells": {
        "looks like": {"text": "droplets"},
        "cost": {"text": "O(n log n)"},
        "effort": {"text": "small counts"},
        "precedent": {"none": true}}}
  ],
  "verdict": {"option": "heightfield pipes", "text": "start here"}
}'
if render_matrix "$MISSING_CRIT_MATRIX" >/dev/null 2>/tmp/hresearch-test-err; then
    fail matrix-rejects-missing-criterion "render succeeded"
elif [ -f "$MATRIX_MD" ]; then
    fail matrix-rejects-missing-criterion "md written despite rejection"
else
    pass matrix-rejects-missing-criterion
fi

# matrix-rejects-bad-verdict: a verdict naming no column is refused
rm -f "$MATRIX_MD"
BAD_VERDICT_MATRIX='{
  "shape": "matrix",
  "criteria": ["looks like", "cost", "effort"],
  "options": [
    {"name": "heightfield pipes", "cells": {
        "looks like": {"text": "rivers, pools"},
        "cost": {"text": "O(cells)"},
        "effort": {"text": "yes"},
        "precedent": {"none": true}}},
    {"name": "SPH particles", "cells": {
        "looks like": {"text": "droplets"},
        "cost": {"text": "O(n log n)"},
        "effort": {"text": "small counts"},
        "precedent": {"none": true}}}
  ],
  "verdict": {"option": "cellular automaton", "text": "start here"}
}'
if render_matrix "$BAD_VERDICT_MATRIX" >/dev/null 2>/tmp/hresearch-test-err; then
    fail matrix-rejects-bad-verdict "render succeeded"
elif [ -f "$MATRIX_MD" ]; then
    fail matrix-rejects-bad-verdict "md written despite rejection"
else
    pass matrix-rejects-bad-verdict
fi

# matrix-rejects-precedent-without-src: a precedent cell with text only is refused
rm -f "$MATRIX_MD"
PRECEDENT_TEXT_MATRIX='{
  "shape": "matrix",
  "criteria": ["looks like", "cost", "effort"],
  "options": [
    {"name": "heightfield pipes", "cells": {
        "looks like": {"text": "rivers, pools"},
        "cost": {"text": "O(cells)"},
        "effort": {"text": "yes"},
        "precedent": {"text": "some crate"}}},
    {"name": "SPH particles", "cells": {
        "looks like": {"text": "droplets"},
        "cost": {"text": "O(n log n)"},
        "effort": {"text": "small counts"},
        "precedent": {"none": true}}}
  ],
  "verdict": {"option": "heightfield pipes", "text": "start here"}
}'
if render_matrix "$PRECEDENT_TEXT_MATRIX" >/dev/null 2>/tmp/hresearch-test-err; then
    fail matrix-rejects-precedent-without-src "render succeeded"
elif [ -f "$MATRIX_MD" ]; then
    fail matrix-rejects-precedent-without-src "md written despite rejection"
else
    pass matrix-rejects-precedent-without-src
fi

matrix_with_criteria() {
    python3 -c '
import json, sys
crit = json.loads(sys.argv[1])
cell = lambda c: {"none": True} if c == "precedent" else {"text": "x"}
rows = [c for c in crit if isinstance(c, str)] + ["precedent"]
opts = [{"name": n, "cells": {c: cell(c) for c in rows}} for n in ("a", "b")]
card = {"shape": "matrix", "options": opts, "verdict": {"option": "a", "text": "pick a"}}
if sys.argv[1] != "null":
    card["criteria"] = crit
print(json.dumps(card))
' "$1"
}

# matrix-criteria-per-question: criteria the question chose, unrelated to games, render as rows
rm -f "$MATRIX_MD"
if render_matrix "$(matrix_with_criteria '["fits nushell", "maintenance"]')" >/dev/null 2>&1 \
    && grep -q "^fits nushell" "$MATRIX_MD" && grep -q "^maintenance" "$MATRIX_MD" \
    && ! grep -q "jam-sized\|bevy precedent" "$MATRIX_MD"; then
    pass matrix-criteria-per-question
else
    fail matrix-criteria-per-question "render failed or rows missing"
fi

# matrix-rejects-bad-criteria: no criteria list, too many, a duplicate, or a fixed row name is refused
for bad in 'null' '[]' '["a", "b", "c", "d", "e"]' '["a", "a"]' '["verdict"]' '["precedent"]' '["one two three four five"]'; do
    rm -f "$MATRIX_MD"
    if render_matrix "$(matrix_with_criteria "$bad")" >/dev/null 2>&1 || [ -f "$MATRIX_MD" ]; then
        fail matrix-rejects-bad-criteria "accepted criteria $bad"
        continue
    fi
    pass "matrix-rejects-bad-criteria $bad"
done

# matrix-verdict-one-column: the verdict row has exactly one non-blank cell
rm -f "$MATRIX_MD"
render_matrix "$OK_MATRIX" >/dev/null 2>&1
if [ "$(awk -F'|' '/^verdict /{n=0; for(i=2;i<NF;i++) if ($i ~ /[^ ]/) n++; print n}' "$MATRIX_MD")" = "1" ]; then
    pass matrix-verdict-one-column
else
    fail matrix-verdict-one-column "expected exactly one non-blank verdict cell"
fi

# matrix-check-ok: check prints shape: matrix and exits 0 on a valid matrix
if "$HRESEARCH" check "$MATRIX_MD" 2>/dev/null | grep -qx "shape: matrix"; then
    pass matrix-check-ok
else
    fail matrix-check-ok "no 'shape: matrix' line"
fi

# matrix-check-detects-hand-edit: a manually edited matrix file fails check
echo "tampered" >> "$MATRIX_MD"
if "$HRESEARCH" check "$MATRIX_MD" >/dev/null 2>&1; then
    fail matrix-check-detects-hand-edit "check passed on a tampered file"
else
    pass matrix-check-detects-hand-edit
fi

FDIR=$(cd "$WORKTREE" && "$HRESEARCH" new "how does a flow field pathfinder work")
"$HRESEARCH" fetch "$FDIR" "$PAGE_URL" >/dev/null
FIGURE_MD="$NOTES/research/$(basename "$FDIR").hresearch"

render_figure() {
    "$HRESEARCH" render "$FDIR" - <<< "$1"
}

FIGURE_LINES='goal *\n+-------+     +-------+\n| . . * | --> | 4 3 0 |\n| . # . |     | 5 # 1 |\n+-------+     +-------+'
CAPTION_TEXT='one BFS run per frame; every agent reads an arrow'

OK_FIGURE=$(python3 -c "
import json
print(json.dumps({
    'shape': 'figure',
    'figure': '$FIGURE_LINES',
    'caption': {'text': '$CAPTION_TEXT', 'src': 1},
}))
")

# figure-render-ok: a valid figure renders the question, indented lines, and a sourced caption
if render_figure "$OK_FIGURE" >/dev/null 2>&1 && [ -f "$FIGURE_MD" ]; then
    missing=""
    grep -qF "question: how does a flow field pathfinder work" "$FIGURE_MD" || missing="$missing question-line"
    grep -qF "    goal *" "$FIGURE_MD" || missing="$missing indent"
    grep -qF "an arrow [1]" "$FIGURE_MD" || missing="$missing caption"
    grep -qF "[1] $PAGE_URL" "$FIGURE_MD" || missing="$missing footnote"
    [ -z "$missing" ] && pass figure-render-ok || fail figure-render-ok "missing:$missing"
else
    fail figure-render-ok "render failed"
fi

# figure-slug: "does"/"work" join the stopwords, leaving only the subject in the stem
case "$(basename "$FDIR")" in
    *-flow-field-pathfinder) pass figure-slug ;;
    *) fail figure-slug "unexpected stem: $(basename "$FDIR")" ;;
esac

# figure-list-form: a figure given as a list of lines renders byte-identical to the string form
STRING_RENDERED="$(cat "$FIGURE_MD")"
rm -f "$FIGURE_MD"
LIST_FIGURE=$(python3 -c "
import json
print(json.dumps({
    'shape': 'figure',
    'figure': '$FIGURE_LINES'.split(chr(10)),
    'caption': {'text': '$CAPTION_TEXT', 'src': 1},
}))
")
render_figure "$LIST_FIGURE" >/dev/null 2>&1
if [ -f "$FIGURE_MD" ] && [ "$(cat "$FIGURE_MD")" = "$STRING_RENDERED" ]; then
    pass figure-list-form
else
    fail figure-list-form "list form did not match string form"
fi

# figure-rejects-prose-line: a 6-word run outside any drawing character is prose, not a figure
rm -f "$FIGURE_MD"
PROSE_FIGURE=$(python3 -c "
import json
lines = 'goal *\nthis sentence has six whole words\n+-------+\n| 4 3 0 |\n+-------+'
print(json.dumps({'shape': 'figure', 'figure': lines, 'caption': {'text': '$CAPTION_TEXT', 'src': 1}}))
")
if render_figure "$PROSE_FIGURE" >/dev/null 2>/tmp/hresearch-test-err; then
    fail figure-rejects-prose-line "render succeeded"
elif [ -f "$FIGURE_MD" ]; then
    fail figure-rejects-prose-line "md written despite rejection"
else
    pass figure-rejects-prose-line
fi

# figure-rejects-wide-line: a line over 78 columns can't be read in one look
rm -f "$FIGURE_MD"
WIDE_FIGURE=$(python3 -c "
import json
wide = '+' + '-' * 77 + '+'
lines = 'goal *\n' + wide + '\n+-------+'
print(json.dumps({'shape': 'figure', 'figure': lines, 'caption': {'text': '$CAPTION_TEXT', 'src': 1}}))
")
if render_figure "$WIDE_FIGURE" >/dev/null 2>/tmp/hresearch-test-err; then
    fail figure-rejects-wide-line "render succeeded"
elif [ -f "$FIGURE_MD" ]; then
    fail figure-rejects-wide-line "md written despite rejection"
else
    pass figure-rejects-wide-line
fi

# figure-rejects-no-drawing: a figure with no box/arrow character is prose wearing line breaks
rm -f "$FIGURE_MD"
NO_DRAWING_FIGURE='{
  "shape": "figure",
  "figure": "goal state\nmiddle state here\nfinal state now",
  "caption": {"text": "one BFS run per frame; every agent reads an arrow", "src": 1}
}'
if render_figure "$NO_DRAWING_FIGURE" >/dev/null 2>/tmp/hresearch-test-err; then
    fail figure-rejects-no-drawing "render succeeded"
elif [ -f "$FIGURE_MD" ]; then
    fail figure-rejects-no-drawing "md written despite rejection"
else
    pass figure-rejects-no-drawing
fi

# figure-rejects-caption-no-src: the caption line is the one place a figure must cite its source
rm -f "$FIGURE_MD"
NO_SRC_CAPTION_FIGURE=$(python3 -c "
import json
print(json.dumps({'shape': 'figure', 'figure': '$FIGURE_LINES', 'caption': {'text': '$CAPTION_TEXT'}}))
")
if render_figure "$NO_SRC_CAPTION_FIGURE" >/dev/null 2>/tmp/hresearch-test-err; then
    fail figure-rejects-caption-no-src "render succeeded"
elif [ -f "$FIGURE_MD" ]; then
    fail figure-rejects-caption-no-src "md written despite rejection"
else
    pass figure-rejects-caption-no-src
fi

# figure-rejects-two-line-caption: the caption is one line under the figure, never a paragraph
rm -f "$FIGURE_MD"
TWO_LINE_CAPTION_FIGURE=$(python3 -c "
import json
print(json.dumps({'shape': 'figure', 'figure': '$FIGURE_LINES', 'caption': {'text': 'line one\nline two', 'src': 1}}))
")
if render_figure "$TWO_LINE_CAPTION_FIGURE" >/dev/null 2>/tmp/hresearch-test-err; then
    fail figure-rejects-two-line-caption "render succeeded"
elif [ -f "$FIGURE_MD" ]; then
    fail figure-rejects-two-line-caption "md written despite rejection"
else
    pass figure-rejects-two-line-caption
fi

render_figure "$OK_FIGURE" >/dev/null 2>&1

# figure-check-ok: check prints shape: figure and exits 0 on a valid figure
if "$HRESEARCH" check "$FIGURE_MD" 2>/dev/null | grep -qx "shape: figure"; then
    pass figure-check-ok
else
    fail figure-check-ok "no 'shape: figure' line"
fi

# figure-require-na: --require has nothing to name on a figure, which has no slots
FIGREQ_OUT="$("$HRESEARCH" check --require caption "$FIGURE_MD" 2>&1)"
if grep -q -- "--require: not applicable to a figure" <<< "$FIGREQ_OUT"; then
    pass figure-require-na
else
    fail figure-require-na "expected not-applicable message"
fi

# figure-check-detects-hand-edit: a manually edited figure file fails check, like the other shapes
echo "tampered" >> "$FIGURE_MD"
if "$HRESEARCH" check "$FIGURE_MD" >/dev/null 2>&1; then
    fail figure-check-detects-hand-edit "check passed on a tampered file"
else
    pass figure-check-detects-hand-edit
fi

SHAPE_TABLE=(
    "how do I set up colliders with avian?|card"
    "how to set up colliders with avian|card"
    "what does Collider::trimesh_from_mesh do|card"
    "which techniques exist to simulate water flow|matrix"
    "what approaches are there for procedural terrain|matrix"
    "how does a flow field pathfinder work|figure"
    "how does a flow field pathfinder move many agents|figure"
    "how a flow-field pathfinder moves 200 agents|figure"
)

# shape-classify: classify_shape's ordered rules yield the plan's table, and
# `new` reports the winner as "(from question)"
CLASSIFY_FAIL=""
for row in "${SHAPE_TABLE[@]}"; do
    q="${row%%|*}"
    expected="${row##*|}"
    cdir=$("$HRESEARCH" new "$q" 2>/tmp/hresearch-test-classify-err)
    got="$(cat "$cdir/shape.txt" 2>/dev/null)"
    if [ "$got" != "$expected" ] || ! grep -qF "shape: $expected (from question)" /tmp/hresearch-test-classify-err; then
        CLASSIFY_FAIL="question '$q': expected $expected, got '$got', stderr: $(cat /tmp/hresearch-test-classify-err)"
        break
    fi
done
if [ -z "$CLASSIFY_FAIL" ]; then
    pass shape-classify
else
    fail shape-classify "$CLASSIFY_FAIL"
fi

# shape-recorded: `new` writes the classified shape to shape.txt in the dir
# it hands back, for each of the three shapes already in use above
if [ "$(cat "$DIR/shape.txt" 2>/dev/null)" = "card" ] \
    && [ "$(cat "$MDIR/shape.txt" 2>/dev/null)" = "matrix" ] \
    && [ "$(cat "$FDIR/shape.txt" 2>/dev/null)" = "figure" ]; then
    pass shape-recorded
else
    fail shape-recorded "DIR=$(cat "$DIR/shape.txt" 2>/dev/null) MDIR=$(cat "$MDIR/shape.txt" 2>/dev/null) FDIR=$(cat "$FDIR/shape.txt" 2>/dev/null)"
fi

# shape-override: `new --shape` records the flag's shape, not the classifier's,
# and reports it as an override
OVERRIDE_DIR=$("$HRESEARCH" new --shape matrix "how do I X" 2>/tmp/hresearch-test-override-err)
if [ "$(cat "$OVERRIDE_DIR/shape.txt" 2>/dev/null)" = "matrix" ] \
    && grep -qF "shape: matrix (override)" /tmp/hresearch-test-override-err; then
    pass shape-override
else
    fail shape-override "shape.txt=$(cat "$OVERRIDE_DIR/shape.txt" 2>/dev/null) stderr=$(cat /tmp/hresearch-test-override-err)"
fi

# shape-mismatch-rejected: render refuses a card whose shape disagrees with
# the one `new` recorded for the question, and writes neither card.json nor
# the .hresearch
MISMATCH_DIR=$("$HRESEARCH" new "which techniques exist to grow crystals" 2>/dev/null)
"$HRESEARCH" fetch "$MISMATCH_DIR" "$PAGE_URL" >/dev/null
"$HRESEARCH" fetch "$MISMATCH_DIR" "$CODE_URL" >/dev/null
MISMATCH_MD="$NOTES/research/$(basename "$MISMATCH_DIR").hresearch"
if "$HRESEARCH" render "$MISMATCH_DIR" - <<< "$OK_CARD" >/dev/null 2>/tmp/hresearch-test-mismatch-err; then
    fail shape-mismatch-rejected "render succeeded"
elif [ -f "$MISMATCH_MD" ] || [ -f "$MISMATCH_DIR/card.json" ]; then
    fail shape-mismatch-rejected "card.json or md written despite rejection"
elif grep -qF "render: shape: the question called for matrix, got card" /tmp/hresearch-test-mismatch-err; then
    pass shape-mismatch-rejected
else
    fail shape-mismatch-rejected "unexpected stderr: $(cat /tmp/hresearch-test-mismatch-err)"
fi

# A second task, to prove a session's answers stay inside their own task.
WORKTREE_B="$TMP/fun_worktrees/p/t/other"
NOTES_B="$TMP/fun_repos/hobby-mimisbrunnr/notes/p/t/other"
mkdir -p "$WORKTREE_B/src/deep" "$NOTES_B"

K_MD_COUNT_BEFORE=$(find "$NOTES/research" -maxdepth 1 -name '*.hresearch' | wc -l)

CDIR_B=$(cd "$WORKTREE_B" && "$HRESEARCH" new "how do I set up colliders with avian?" 2>/dev/null)
(cd "$WORKTREE_B" && "$HRESEARCH" fetch "$CDIR_B" "$PAGE_URL") >/dev/null
(cd "$WORKTREE_B" && "$HRESEARCH" fetch "$CDIR_B" "$CODE_URL") >/dev/null
(cd "$WORKTREE_B" && "$HRESEARCH" render "$CDIR_B" - <<< "$OK_CARD") >/dev/null

MDIR_B=$(cd "$WORKTREE_B" && "$HRESEARCH" new "which techniques exist to simulate water flow" 2>/dev/null)
(cd "$WORKTREE_B" && "$HRESEARCH" fetch "$MDIR_B" "$PAGE_URL") >/dev/null
(cd "$WORKTREE_B" && "$HRESEARCH" render "$MDIR_B" - <<< "$OK_MATRIX") >/dev/null

# session-subdir-cwd: `new` from a nested cwd inside the worktree still
# resolves to that same task, not the outside-task case
FDIR_B=$(cd "$WORKTREE_B/src/deep" && "$HRESEARCH" new "how a flow-field pathfinder moves 200 agents" 2>/dev/null)
(cd "$WORKTREE_B/src/deep" && "$HRESEARCH" fetch "$FDIR_B" "$PAGE_URL") >/dev/null
(cd "$WORKTREE_B/src/deep" && "$HRESEARCH" render "$FDIR_B" - <<< "$OK_FIGURE") >/dev/null
case "$FDIR_B" in
    "$NOTES_B/research/sources/"*) pass session-subdir-cwd ;;
    *) fail session-subdir-cwd "unexpected dir: $FDIR_B" ;;
esac

# session-own-files: three distinct answers land under the second task's own
# research/, and the first task's file count is unchanged by any of it
K_MD_COUNT_AFTER=$(find "$NOTES/research" -maxdepth 1 -name '*.hresearch' | wc -l)
OTHER_MD_COUNT=$(find "$NOTES_B/research" -maxdepth 1 -name '*.hresearch' 2>/dev/null | wc -l)
if [ "$OTHER_MD_COUNT" = 3 ] \
    && [ -d "$NOTES_B/research/sources/$(basename "$CDIR_B")" ] \
    && [ -d "$NOTES_B/research/sources/$(basename "$MDIR_B")" ] \
    && [ -d "$NOTES_B/research/sources/$(basename "$FDIR_B")" ] \
    && [ "$K_MD_COUNT_AFTER" = "$K_MD_COUNT_BEFORE" ]; then
    pass session-own-files
else
    fail session-own-files "other=$OTHER_MD_COUNT k_before=$K_MD_COUNT_BEFORE k_after=$K_MD_COUNT_AFTER"
fi

# session-list: `list` from the second task prints exactly one line per
# answer it holds, one of each shape
LIST_OUT=$(cd "$WORKTREE_B" && "$HRESEARCH" list)
LIST_LINE_COUNT=$(printf '%s\n' "$LIST_OUT" | grep -c .)
LIST_SHAPES=$(printf '%s\n' "$LIST_OUT" | cut -f2 | sort | tr '\n' ',')
if [ "$LIST_LINE_COUNT" = 3 ] && [ "$LIST_SHAPES" = "card,figure,matrix," ]; then
    pass session-list
else
    fail session-list "unexpected output: $LIST_OUT"
fi

# session-no-cross-task: from the second task's worktree, fetch/render
# against the first task's dir are refused and leave it byte-identical; a dir
# under this task's own sources/ that `new` never made is refused too, and
# never created
cp "$DIR/sources.json" /tmp/hresearch-test-cross-sources-before.json
cp "$CARD_MD" /tmp/hresearch-test-cross-card-before.md
GHOST_DIR="$NOTES_B/research/sources/ghost"
CROSS_OK=1
(cd "$WORKTREE_B" && "$HRESEARCH" fetch "$DIR" "$PAGE_URL") >/dev/null 2>&1 && CROSS_OK=0
(cd "$WORKTREE_B" && "$HRESEARCH" render "$DIR" - <<< "$OK_CARD") >/dev/null 2>&1 && CROSS_OK=0
cmp -s "$DIR/sources.json" /tmp/hresearch-test-cross-sources-before.json || CROSS_OK=0
cmp -s "$CARD_MD" /tmp/hresearch-test-cross-card-before.md || CROSS_OK=0
(cd "$WORKTREE_B" && "$HRESEARCH" fetch "$GHOST_DIR" "$PAGE_URL") >/dev/null 2>&1 && CROSS_OK=0
[ -d "$GHOST_DIR" ] && CROSS_OK=0
if [ "$CROSS_OK" = 1 ]; then
    pass session-no-cross-task
else
    fail session-no-cross-task "cross-task guard failed"
fi

SEARCH_NOTES="$TMP/fun_repos/hobby-mimisbrunnr/notes"

fixture_card() {
    local proj="$1" stem="$2" shape="$3" question="$4" urls="$5" card="$6"
    local research="$SEARCH_NOTES/$proj/topic/task/research"
    local dir="$research/sources/$stem"
    mkdir -p "$dir"
    printf '%s\n' "$question" > "$dir/question.txt"
    printf '%s\n' "$shape" > "$dir/shape.txt"
    python3 -c '
import json, sys
urls = sys.argv[1].split()
print(json.dumps([{"id": i + 1, "url": u, "fetched": "2026-10-02T00:00:00Z"} for i, u in enumerate(urls)]))
' "$urls" > "$dir/sources.json"
    printf '%s\n' "$card" > "$dir/card.json"
    printf 'rendered\n' > "$research/$stem.hresearch"
}

fixture_card sa 2026-10-01-tilemap-grid card "how to draw a tilemap grid" \
    "https://github.com/bevyengine/bevy/blob/v0.19.1/examples/2d/tilemap_chunk.rs https://docs.rs/bevy/0.19.1/bevy/sprite_render/struct.TilemapChunk.html https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_sprite_render/src/lib.rs" \
    '{"shape":"card","verdict":[{"text":"use the chunk","src":1}],"how":[{"text":"spawn it","src":1}],"snippet":{"code":"commands.spawn(TilemapChunk::default());","src":1},"breaks":{"empty":"none"}}'
fixture_card sb 2026-10-02-avian-gravity card "how to turn off gravity" \
    "https://github.com/avianphysics/avian/blob/v0.7.0/crates/avian2d/src/lib.rs https://docs.rs/avian2d/0.7.0/avian2d/struct.GravityScale.html" \
    '{"shape":"card","verdict":[{"text":"set the scale to zero","src":2}],"how":{"empty":"n/a"},"snippet":{"empty":"n/a"},"breaks":{"empty":"n/a"}}'
fixture_card sb 2026-10-03-unpinned-thing card "how to frobnicate widgets" \
    "https://docs.rs/frob/latest/frob/struct.Widget.html https://github.com/o/frob/blob/main/src/lib.rs" \
    '{"shape":"card","verdict":[{"text":"call frobnicate","src":1}],"how":{"empty":"n/a"},"snippet":{"empty":"n/a"},"breaks":{"empty":"n/a"}}'
fixture_card sb 2026-10-04-which-physics matrix "which physics approaches exist" \
    "https://docs.rs/avian2d/0.7.0/avian2d/index.html" \
    '{"shape":"matrix","options":[{"name":"avian"},{"name":"rapier"}],"verdict":{"option":"avian","text":"pick avian"}}'
UNRENDERED="$SEARCH_NOTES/sb/topic/task/research/sources/2026-10-05-unrendered-question"
mkdir -p "$UNRENDERED"
printf 'an unrendered quokka question\n' > "$UNRENDERED/question.txt"
printf '{"shape":"card"}\n' > "$UNRENDERED/card.json"

SEARCH_WT="$TMP/fun_worktrees/sw/topic/task"
mkdir -p "$SEARCH_WT/game"
write_lock() {
    cat > "$SEARCH_WT/game/Cargo.lock" <<EOF
version = 4

[[package]]
name = "bevy"
version = "$1"

[[package]]
name = "avian2d"
version = "0.7.0"

[[package]]
name = "frob"
version = "1.0.0"
EOF
}
write_lock 0.19.1

search() { (cd "$1" && shift && "$HRESEARCH" search "$@"); }

# search-across-projects: one term per project, both found from outside any task
if search "$TMP" tilemap | grep -q "how to draw a tilemap grid" \
    && search "$TMP" gravity | grep -q "how to turn off gravity"; then
    pass search-across-projects
else
    fail search-across-projects "a project's card was not found from $TMP"
fi

# search-docsrs-pinned: a docs.rs /<crate>/<ver>/ URL pins the card's crate
if search "$TMP" tilemap | grep -q " bevy@0.19.1 "; then
    pass search-docsrs-pinned
else
    fail search-docsrs-pinned "$(search "$TMP" tilemap)"
fi

# search-github-tag: blob/v<ver>/crates/<name>/ names the crate and strips the v
if search "$TMP" gravity | grep -q " avian2d@0.7.0 "; then
    pass search-github-tag
else
    fail search-github-tag "$(search "$TMP" gravity)"
fi

# search-unpinned: latest and main pin nothing, and a worktree tags them unpinned
if search "$SEARCH_WT" frobnicate | grep -q " frob@unpinned unpinned "; then
    pass search-unpinned
else
    fail search-unpinned "$(search "$SEARCH_WT" frobnicate)"
fi

# search-tag-current: a Cargo.lock one level below the worktree root tags a matching version current
if search "$SEARCH_WT" tilemap | grep -q " bevy@0.19.1 current "; then
    pass search-tag-current
else
    fail search-tag-current "$(search "$SEARCH_WT" tilemap)"
fi

# search-tag-other-version: the same crate locked at another version is other-version
write_lock 0.18.0
if search "$SEARCH_WT" tilemap | grep -q " bevy@0.19.1 other-version "; then
    pass search-tag-other-version
else
    fail search-tag-other-version "$(search "$SEARCH_WT" tilemap)"
fi
write_lock 0.19.1

# search-prefix: a term matches as a prefix, so tile finds tilemap
if search "$TMP" tile | grep -q "how to draw a tilemap grid"; then
    pass search-prefix
else
    fail search-prefix "tile did not find the tilemap card"
fi

# search-ident: a name that is only in a docs.rs item URL finds its card
if search "$TMP" GravityScale | grep -q "how to turn off gravity"; then
    pass search-ident
else
    fail search-ident "GravityScale did not find the gravity card"
fi

# search-refresh: an edited question is what the next search prints, with no rebuild
printf 'how to switch gravity off entirely\n' > "$SEARCH_NOTES/sb/topic/task/research/sources/2026-10-02-avian-gravity/question.txt"
touch -d '+1 minute' "$SEARCH_NOTES/sb/topic/task/research/sources/2026-10-02-avian-gravity/question.txt"
if search "$TMP" gravity | grep -q "switch gravity off entirely"; then
    pass search-refresh
else
    fail search-refresh "$(search "$TMP" gravity)"
fi

# search-removed: a deleted card leaves the results on the next search
rm -r "$SEARCH_NOTES/sb/topic/task/research/sources/2026-10-03-unpinned-thing" \
    "$SEARCH_NOTES/sb/topic/task/research/2026-10-03-unpinned-thing.hresearch"
if ! search "$TMP" frobnicate >/dev/null; then
    pass search-removed
else
    fail search-removed "removed card still listed"
fi

# search-unrendered-skipped: a question with no rendered file is not a card
if ! search "$TMP" quokka >/dev/null; then
    pass search-unrendered-skipped
else
    fail search-unrendered-skipped "unrendered question listed"
fi

# search-filters: --shape, --project and --crate each narrow the listing
ALL_COUNT=$(search "$TMP" --project sb | grep -c .)
SHAPE_OUT=$(search "$TMP" --shape matrix --project sb)
PROJECT_OUT=$(search "$TMP" --project sa)
CRATE_OUT=$(search "$TMP" --crate avian2d@0.7.0)
if [ "$(printf '%s\n' "$SHAPE_OUT" | grep -c .)" = 1 ] && printf '%s' "$SHAPE_OUT" | grep -q "which physics" \
    && [ "$(printf '%s\n' "$PROJECT_OUT" | grep -c .)" = 1 ] && printf '%s' "$PROJECT_OUT" | grep -q tilemap \
    && [ "$(printf '%s\n' "$CRATE_OUT" | grep -c .)" = 2 ] && ! printf '%s' "$CRATE_OUT" | grep -q tilemap \
    && [ "$ALL_COUNT" = 2 ]; then
    pass search-filters
else
    fail search-filters "shape=[$SHAPE_OUT] project=[$PROJECT_OUT] crate=[$CRATE_OUT]"
fi

# search-no-hit: no hit exits 1 with nothing on stdout, so a shell can test it
NO_HIT=$(search "$TMP" zzznothing); NO_HIT_RC=$?
if [ "$NO_HIT_RC" = 1 ] && [ -z "$NO_HIT" ]; then
    pass search-no-hit
else
    fail search-no-hit "rc=$NO_HIT_RC out=[$NO_HIT]"
fi

symbols() { (cd "$1" && shift && "$HRESEARCH" symbols "$@"); }
sym_field() { python3 -c '
import json, sys
rows = json.load(sys.stdin)
for r in rows:
    if r["name"] == sys.argv[1]:
        print(r[sys.argv[2]])
        break
' "$1" "$2"; }

fixture_card sa 2026-10-06-line-card card "how to place a line marker" \
    "https://docs.rs/bevy/0.19.1/bevy/enum.LineMarker.html" \
    '{"shape":"card","verdict":[{"text":"use it","src":1}],"how":{"empty":"n/a"},"snippet":{"empty":"n/a"},"breaks":{"empty":"n/a"}}'
printf 'one\ntwo\nuse LineMarker here\n' > "$SEARCH_NOTES/sa/topic/task/research/2026-10-06-line-card.hresearch"
write_lock 0.19.1

# symbols-name-filter: only names containing every query word are returned, as the picker does not re-filter
SYM_OUT=$(symbols "$SEARCH_WT" TilemapChunk)
if [ "$(printf '%s' "$SYM_OUT" | sym_field TilemapChunk kind)" = struct ] \
    && [ -z "$(printf '%s' "$SYM_OUT" | sym_field "how to draw a tilemap grid" kind)" ]; then
    pass symbols-name-filter
else
    fail symbols-name-filter "$SYM_OUT"
fi

# symbols-container-tag: the container carries crate@version and the lock tag
if [ "$(printf '%s' "$SYM_OUT" | sym_field TilemapChunk container)" = "sa/task bevy@0.19.1 current" ]; then
    pass symbols-container-tag
else
    fail symbols-container-tag "$SYM_OUT"
fi

# symbols-line: the symbol points at the first line of the card that names the identifier
LINE_OUT=$(symbols "$SEARCH_WT" LineMarker)
if [ "$(printf '%s' "$LINE_OUT" | sym_field LineMarker line)" = 2 ] \
    && [ "$(printf '%s' "$LINE_OUT" | sym_field LineMarker kind)" = enum ]; then
    pass symbols-line
else
    fail symbols-line "$LINE_OUT"
fi

# symbols-no-hit: no hit prints an empty array and exits 0, so the language server never sees an error
NO_SYM=$(symbols "$SEARCH_WT" zzznothing); NO_SYM_RC=$?
if [ "$NO_SYM_RC" = 0 ] && [ "$NO_SYM" = "[]" ]; then
    pass symbols-no-hit
else
    fail symbols-no-hit "rc=$NO_SYM_RC out=[$NO_SYM]"
fi

STALE_CARD="$SEARCH_NOTES/sa/topic/task/research/2026-10-01-tilemap-grid.hresearch"
STALE_DIR="$SEARCH_NOTES/sa/topic/task/research/sources/2026-10-01-tilemap-grid"
stale() { (cd "$SEARCH_WT" && "$HRESEARCH" stale "$@"); }

# stale-behind: a card pinned below the lock is listed with the locked version, and one equal to it is not
write_lock 0.19.2
STALE_OUT=$(stale)
if printf '%s\n' "$STALE_OUT" | grep -q "bevy@0.19.1 -> 0.19.2 how to draw a tilemap grid" \
    && ! printf '%s\n' "$STALE_OUT" | grep -q "avian2d@0.7.0"; then
    pass stale-behind
else
    fail stale-behind "$STALE_OUT"
fi

# stale-current-quiet: a lock equal to every pin lists nothing and exits 1
write_lock 0.19.1
STALE_QUIET=$(stale); STALE_QUIET_RC=$?
if [ "$STALE_QUIET_RC" = 1 ] && [ -z "$STALE_QUIET" ]; then
    pass stale-current-quiet
else
    fail stale-current-quiet "rc=$STALE_QUIET_RC out=[$STALE_QUIET]"
fi

# stale-lock-flag: --lock reads the named file from outside any worktree and agrees with the worktree lock
write_lock 0.19.2
FLAG_OUT=$(cd "$TMP" && "$HRESEARCH" stale --lock "$SEARCH_WT/game/Cargo.lock")
if [ -n "$FLAG_OUT" ] && [ "$FLAG_OUT" = "$(stale)" ]; then
    pass stale-lock-flag
else
    fail stale-lock-flag "$FLAG_OUT"
fi

# stale-no-lock: outside a worktree without --lock there is nothing to compare against, which is exit 2
(cd "$TMP" && "$HRESEARCH" stale) >/dev/null 2>&1; NO_LOCK_RC=$?
if [ "$NO_LOCK_RC" = 2 ]; then
    pass stale-no-lock
else
    fail stale-no-lock "rc=$NO_LOCK_RC"
fi

# retract-unknown: retracting a card that does not exist exits 1 and writes nothing
"$HRESEARCH" retract "$SEARCH_NOTES/sa/topic/task/research/no-such-card.hresearch" "x" >/dev/null 2>&1; RETRACT_UNKNOWN_RC=$?
if [ "$RETRACT_UNKNOWN_RC" = 1 ]; then
    pass retract-unknown
else
    fail retract-unknown "rc=$RETRACT_UNKNOWN_RC"
fi

"$HRESEARCH" retract "$STALE_CARD" "wrong: fixture" >/dev/null

# retract-hides-search: a retracted card no longer comes back from search
if ! search "$TMP" tilemap | grep -q "how to draw a tilemap grid"; then
    pass retract-hides-search
else
    fail retract-hides-search "still listed"
fi

# retract-hides-symbols: a retracted card yields no workspace symbol, so space S does not list it
if ! symbols "$SEARCH_WT" TilemapChunk | grep -q "2026-10-01-tilemap-grid"; then
    pass retract-hides-symbols
else
    fail retract-hides-symbols "still listed"
fi

# retract-reason-kept: the reason is stored beside the card, not in the derived index
if [ "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["reason"])' "$STALE_DIR/retracted.json")" = "wrong: fixture" ]; then
    pass retract-reason-kept
else
    fail retract-reason-kept "$(cat "$STALE_DIR/retracted.json" 2>&1)"
fi

# retract-hides-stale: a retracted card is not reported as stale
if ! stale | grep -q "how to draw a tilemap grid"; then
    pass retract-hides-stale
else
    fail retract-hides-stale "still listed"
fi

# unretract: deleting retracted.json is the whole undo, and search lists the card again
rm "$STALE_DIR/retracted.json"
if search "$TMP" tilemap | grep -q "how to draw a tilemap grid"; then
    pass unretract
else
    fail unretract "not listed after rm"
fi
write_lock 0.19.1

blocked() {
    env http_proxy=http://127.0.0.1:9 https_proxy=http://127.0.0.1:9 \
        HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 \
        no_proxy= NO_PROXY= "$@"
}

PINNED_URL="https://docs.rs/bevy/0.19.1/src/bevy/example.rs.html"
LATEST_URL="https://docs.rs/frob/latest/frob/index.html"
RANGE_URL="https://docs.rs/bevy/0.19/bevy/index.html"
SEED_DIR=$("$HRESEARCH" new "how do I seed a collider card" 2>/dev/null)
{ cat "$TMP/fixtures/example.rs"; echo "A collider without a rigid body is treated as static."; } > "$SEED_DIR/1.txt"
echo "frob latest page" > "$SEED_DIR/2.txt"
echo "bevy range page" > "$SEED_DIR/3.txt"
python3 - "$SEED_DIR/sources.json" "$PINNED_URL" "$LATEST_URL" "$RANGE_URL" <<'PY_END'
import json, sys
urls = sys.argv[2:]
json.dump([{"id": i + 1, "url": u, "fetched": "2026-10-02T00:00:00Z"} for i, u in enumerate(urls)],
          open(sys.argv[1], "w"))
PY_END
SEED_CARD='{
  "verdict": [{"text": "Add Collider + RigidBody", "src": 1}],
  "how": [{"text": "spawn with RigidBody and Collider", "src": 1}],
  "snippet": {"code": "commands.spawn((\n    RigidBody::Dynamic,\n    Collider::cuboid(1.0, 1.0, 1.0),\n));", "src": 1},
  "breaks": [{"quote": "A collider without a rigid body is treated as static.", "src": 1}]
}'
"$HRESEARCH" render "$SEED_DIR" - <<< "$SEED_CARD" >/dev/null 2>&1
SEED_MD="$NOTES/research/$(basename "$SEED_DIR").hresearch"

# seed-stdout-dir: `new --seed` prints only the new dir on stdout, and stderr says which of
# the seed's sources were stored and which must be fetched again (a docs.rs semver range is not pinned)
SEED_OUT=$("$HRESEARCH" new --seed "$SEED_MD" "how do I reuse a collider seed" 2>/tmp/hresearch-test-seed-err)
if [ "$(printf '%s\n' "$SEED_OUT" | wc -l)" = 1 ] && [ -d "$SEED_OUT" ] \
    && grep -qF "seed: cached $PINNED_URL" /tmp/hresearch-test-seed-err \
    && grep -qF "seed: fetch again $LATEST_URL" /tmp/hresearch-test-seed-err \
    && grep -qF "seed: fetch again $RANGE_URL" /tmp/hresearch-test-seed-err; then
    pass seed-stdout-dir
else
    fail seed-stdout-dir "out=$SEED_OUT err=$(cat /tmp/hresearch-test-seed-err)"
fi

# seed-fetch-cached: a pinned URL stored by --seed is served with the network blocked, byte-identical
SEED_FETCH_OUT=$(blocked "$HRESEARCH" fetch "$SEED_OUT" "$PINNED_URL" 2>&1); SEED_FETCH_RC=$?
if [ "$SEED_FETCH_RC" = 0 ] && [[ "$SEED_FETCH_OUT" == *"(cached)" ]] \
    && cmp -s "$SEED_OUT/1.txt" "$SEED_DIR/1.txt"; then
    pass seed-fetch-cached
else
    fail seed-fetch-cached "rc=$SEED_FETCH_RC out=$SEED_FETCH_OUT"
fi

# seed-blocked-control: the block really stops the network, and an unpinned `latest` URL is never served from the store
CONTROL_OUT=$(blocked "$HRESEARCH" fetch "$SEED_OUT" "$LATEST_URL" 2>&1); CONTROL_RC=$?
if [ "$CONTROL_RC" = 1 ] && [[ "$CONTROL_OUT" != *"(cached)"* ]]; then
    pass seed-blocked-control
else
    fail seed-blocked-control "rc=$CONTROL_RC out=$CONTROL_OUT"
fi

# seed-semver-range-not-cached: docs.rs resolves /0.19/ to the newest 0.19.x, so it is never stored or served
RANGE_OUT=$(blocked "$HRESEARCH" fetch "$SEED_OUT" "$RANGE_URL" 2>&1); RANGE_RC=$?
if [ "$RANGE_RC" = 1 ] && [[ "$RANGE_OUT" != *"(cached)"* ]]; then
    pass seed-semver-range-not-cached
else
    fail seed-semver-range-not-cached "rc=$RANGE_RC out=$RANGE_OUT"
fi

# seed-render-identical: the seed's card renders against the cached copy, since render's verbatim check sees identical text
if "$HRESEARCH" render "$SEED_OUT" - <<< "$SEED_CARD" >/dev/null 2>/tmp/hresearch-test-seed-render-err \
    && [ -f "$NOTES/research/$(basename "$SEED_OUT").hresearch" ]; then
    pass seed-render-identical
else
    fail seed-render-identical "$(cat /tmp/hresearch-test-seed-render-err)"
fi

# seed-unknown-card: seeding from a card that does not exist exits 1 and leaves no new sources dir behind
SEED_DIRS_BEFORE=$(ls "$NOTES/research/sources" | wc -l)
"$HRESEARCH" new --seed /nonexistent/x.hresearch "q" >/dev/null 2>&1; SEED_UNKNOWN_RC=$?
SEED_DIRS_AFTER=$(ls "$NOTES/research/sources" | wc -l)
if [ "$SEED_UNKNOWN_RC" = 1 ] && [ "$SEED_DIRS_BEFORE" = "$SEED_DIRS_AFTER" ]; then
    pass seed-unknown-card
else
    fail seed-unknown-card "rc=$SEED_UNKNOWN_RC before=$SEED_DIRS_BEFORE after=$SEED_DIRS_AFTER"
fi

# fetch-stores-pinned-only: a file:// fetch is unpinned, so nothing is written to the source store for it
STORE_BEFORE=$(ls "$XDG_CACHE_HOME/hresearch/sources" | wc -l)
"$HRESEARCH" fetch "$SEED_OUT" "$PAGE_URL" >/dev/null 2>&1
STORE_AFTER=$(ls "$XDG_CACHE_HOME/hresearch/sources" | wc -l)
if [ "$STORE_BEFORE" = "$STORE_AFTER" ] && [ "$STORE_AFTER" = 1 ]; then
    pass fetch-stores-pinned-only
else
    fail fetch-stores-pinned-only "before=$STORE_BEFORE after=$STORE_AFTER"
fi

# retract-dir-arg: retract accepts a sources dir as well as the .hresearch path
if "$HRESEARCH" retract "$SEED_DIR" "wrong: dir arg" >/dev/null 2>&1 && [ -f "$SEED_DIR/retracted.json" ]; then
    pass retract-dir-arg
else
    fail retract-dir-arg "no retracted.json"
fi
rm -f "$SEED_DIR/retracted.json"

if [ "$FAILED" -eq 0 ]; then
    echo "SELFTEST OK"
    exit 0
else
    echo "SELFTEST FAILED"
    exit 1
fi
