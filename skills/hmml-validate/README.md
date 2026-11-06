# HMML Validate

| Field        | Value                                                                                                                                                                                                                 |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                                                                                 |
| Install path | `~/.claude-hobby/commands/hmml-validate.md`                                                                                                                                                                                              |
| Owner        | @julianbieber                                                                                                                                                                                                         |
| Dependencies | HMML grammar (`lsps/hobby-mml-lsp/grammar.md`), per-task specs in the notes repo (`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`), `tree-extract` + `lsp-mcp`, `hmml-extract` |
| Status       | experimental                                                                                                                                                                                                          |

## Description

Read-only **conformance / drift check**: does the implementation still do what
its [`.hmml`](../../lsps/hobby-mml-lsp/grammar.md) spec says?

- `hmml-extract` — code → a `.hmml` baseline, or with `--against` the branch's
  change as a delta
- **`hmml-validate` — code vs the current `.hmml` → a drift report** (edits
  nothing)

A spec may carry a change as `# +` / `# -` annotations on top of the baseline
(grammar §10), and that splits the report in two: a `# +` claim is
intent, so its absence before implementation is expected and its absence after is
unfinished work; a `# -` line is behaviour the change removes, so finding it
still in the code after implementation is a failure to conform. The annotations
are never stripped — they stay after the change lands.

Over time code and spec drift apart — someone changes the code and forgets the
spec, or edits a spec but never implements it. Validate routes each spec claim
to the real code (via the module `@ path` anchors, `tree-extract`, and
`lsp-mcp`) and reports, per section, one of four verdicts:

- **CONFORMS** — the claim holds in the code.
- **SPEC-AHEAD** — the spec asserts something the code doesn't do (the usual
  real drift: a behavior step with no code, an invariant with no enforcement, a
  missing field/module).
- **CODE-AHEAD** — the code does something significant the spec omits (only
  flagged when it matters — the spec is a lossy view by design).
- **CANNOT-VERIFY** — needs a runtime check that couldn't be run.

It also cross-checks the spec against itself (a behavior step that contradicts
an invariant).

Produces: a per-section verdict table + an overall CONFORMS/DRIFTED verdict,
with a **recommended remediation for each drift — but performs none of it.** If
the spec is the intent, the code is changed to match; if the code is the truth,
it points you at `hmml-extract`. Deciding which side is right is your call.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/hmml-validate/skill.md \
  ~/.claude-hobby/commands/hmml-validate.md
```

## Usage

```
/hmml-validate mesh-terrain-chunks
/hmml-validate get-product-identifier-mapping
```

Point it at a bare `unit` (or a `.hmml` path). It resolves to the spec in the
current task's notes directory, reads the spec and the code, never writing
either, and prints a drift report. Typical follow-ups it will recommend: a code
change (the spec is the intent) or `/hmml-extract` (refresh a spec the code has
outgrown).

## Known Limitations

- **Read-only by contract.** It reports drift and recommends a fix; it never
  edits code or spec and never runs `git add` / `git commit`. Reconciling the
  two is your job — deliberately, because only you know which side is
  authoritative.
- **The spec is a lossy view.** Undescribed code is expected, so CODE-AHEAD is
  reported conservatively (significant collaborators/branches only). It will not
  surface every helper the spec omits — that is by design, not a miss.
- **Static-first.** Behavior/invariant conformance is judged by reading the
  routed code. Invariants that need a runtime check are exercised best-effort
  via the repo's test command (`mise exec --cd <checkout>`); where infra (e.g.
  docker databases) is unavailable, they are marked CANNOT-VERIFY rather than
  failed.
- **Altitude-sensitive.** A reworded-but-equivalent behavior step conforms; the
  judgement of "equivalent" is the model's, same as extract's altitude calls.
- Structural validity comes from `hobby-mml-parser <path> --lint`; anything the
  grammar cannot express (altitude, intent) is still a judgement call.
