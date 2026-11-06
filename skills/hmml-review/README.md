# HMML Review

| Field        | Value                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------ |
| Type         | skill                                                                                                  |
| Install path | `~/.claude-hobby/commands/hmml-review.md`                                                                                 |
| Owner        | @julianbieber                                                                                          |
| Dependencies | A `.hmml` spec in the task notes (`hmml-new` / `hmml-extract`); the source repo it links to |
| Status       | experimental                                                                                           |

## Description

Reviews an **annotated or new `.hmml` spec** — one that expresses the *intended*
behaviour of a task — through parallel engineering lenses (security,
architecture, correctness/QA, performance, maintainability) and synthesizes a
**concrete implementation plan** for making the code match that intent.

The spec normally arrives as a baseline with the change written on top of it as
`# +` / `# -` annotations (grammar §10). That split is the review's spine: the
delta is what gets pressure-tested, the baseline is the context it must keep
working in, and the plan covers the removals as their own steps rather than only
building the additions.

It is the counterpart to, not a duplicate of, `hmml-validate`:

- `hmml-validate` asks "does the **existing code** still conform to the spec?"
  (code-vs-spec drift). Useless right after you annotate a spec toward a new goal
  — it just reports the intended change as drift.
- `hmml-review` asks "the spec **is** the intent and the code does not match it
  yet — is the intent sound, and how do we implement it?" It produces the plan.

Parallel isolated subagents each review the spec + the current code from one
lens, then a synthesis pass filters and merges their concerns into a plan (goal
/ approach / ordered steps / risks / open questions), written to the task notes
as `plan-{YYYY-MM-DD}-{unit}.md` and echoed inline. Reviews and plans only —
never edits the spec or the code.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/hmml-review/skill.md \
  ~/.claude-hobby/commands/hmml-review.md
```

## Usage

```
/hmml-review                       # auto-detect the task's single .hmml
/hmml-review mesh-terrain-chunks  # a unit in the current task dir
/hmml-review path/to/spec.hmml      # explicit path
/hmml-review --min-priority high   # only high/critical concerns in the plan
```

It is a standalone tool for a spec I am designing by hand. `/task:build` no
longer runs it: the pipeline's spec is derived from the implemented change by
`/hmml-extract --against`, and there is nothing ahead of the code left to
pressure-test.

## Known Limitations

- Reviews and plans only — never edits the spec or the code.
- Quality of the plan depends on the spec's `source` block pointing at real
  code; a spec with no `source` yet gets a thinner, feasibility-light plan.
- Multi-lens fan-out spends more tokens than a single pass; scope it with
  `--min-priority` for small changes.
