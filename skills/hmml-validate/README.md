# HMML Validate

| Field        | Value                                                                                                                                                                                                                 |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                                                                                 |
| Install path | `~/.claude-hobby/commands/hmml-validate.md`                                                                                                                                                                                              |
| Owner        | @julianbieber                                                                                                                                                                                                         |
| Dependencies | HMML grammar (`lsps/hobby-mml-lsp/grammar.md`), per-task specs in the notes repo (`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/`), `tree-extract` + `lsp-mcp`, `hmml-{extract,translate}` |
| Status       | experimental                                                                                                                                                                                                          |

## Description

Read-only **conformance / drift check**: does the implementation still do what
its [`.hmml`](../../lsps/hobby-mml-lsp/grammar.md) spec says? This is the
closing step of the HMML loop and the mirror of `hmml-translate`:

- `hmml-extract` — code → `.hmml`
- `hmml-translate` — an edited `.hmml` → a code change
- **`hmml-validate` — code vs the current `.hmml` → a drift report** (edits
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
the spec is the intent, it points you at `hmml-translate`; if the code is
the truth, at `hmml-extract`. Deciding which side is right is your call.

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
either, and prints a drift report. Typical follow-ups it will recommend:
`/hmml-translate <unit>` (make code match an intended spec) or
`/hmml-extract` (refresh a spec the code has outgrown).

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
