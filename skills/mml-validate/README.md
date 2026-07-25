# MML Validate

| Field        | Value                                                                                                                                                                                                                 |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                                                                                 |
| Install path | `~/.claude-hobby/commands/mml-validate.md`                                                                                                                                                                                              |
| Owner        | @julianbieber                                                                                                                                                                                                         |
| Dependencies | hobby MML grammar (`lsps/hobby-mml-lsp/grammar.md`), per-task specs in the notes repo (`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`), `tree-extract` + `lsp-mcp`, `mml-{extract,translate}` |
| Status       | experimental                                                                                                                                                                                                          |

## Description

Read-only **conformance / drift check**: does the implementation still do what
its [`.mml`](../../lsps/hobby-mml-lsp/grammar.md) spec says? This is the
closing step of the MML loop and the mirror of `mml-translate`:

- `mml-extract` — code → `.mml`
- `mml-translate` — an edited `.mml` → a code change
- **`mml-validate` — code vs the current `.mml` → a drift report** (edits
  nothing)

Over time code and spec drift apart — someone changes the code and forgets the
spec, or edits a spec but never translates it. Validate routes each spec claim
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
the spec is the intent, it points you at `mml-translate`; if the code is
the truth, at `mml-extract`. Deciding which side is right is your call.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/mml-validate/skill.md \
  ~/.claude-hobby/commands/mml-validate.md
```

## Usage

```
/mml-validate mesh-terrain-chunks
/mml-validate get-product-identifier-mapping
```

Point it at a bare `unit` (or a `.mml` path). It resolves to the spec in the
current task's notes directory, reads the spec and the code, never writing
either, and prints a drift report. Typical follow-ups it will recommend:
`/mml-translate <unit>` (make code match an intended spec) or
`/mml-extract` (refresh a spec the code has outgrown).

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
- No `.mml` parser is wired in yet by default (task 06 `mml-parse`); structural
  validity is checked by eye against the grammar until it is.
