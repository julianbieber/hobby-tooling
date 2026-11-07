# comment-pass

| Field        | Value                                                   |
| ------------ | ------------------------------------------------------- |
| Type         | skill                                                   |
| Install path | `~/.claude-hobby/commands/comment-pass.md`              |
| Owner        | @julianbieber                                           |
| Dependencies | `scripts/comment-candidates.py`, python3 |
| Status       | experimental                                            |
| Supersedes   | `comment-review`, `explain-back`'s comment pass         |

## Description

Enforces the comment rule in `dotfiles/claude-md/hobby.CLAUDE.md` — a module
header, and a public element's comment when it says what the signature does not —
over the branch diff or a given path.

**It is mostly a filter.** The comments a model writes by default restate the
name, walk through the body, or belong to the conversation that produced them
(`notes/hobby-tooling/skills/review_too_strict/business-context.md`
`## intent-comments`). The pass removes those, and writes only missing module
headers and the public comments a caller actually needs.

## Stages

| stage | does |
|---|---|
| 0 | run `comment-candidates.py`, report counts, set vendored crates to report-only |
| 1 | resolve the four placement categories — delete, relocate, or write |
| 2 | read the legal comments against the code and signature; cut restatement, detail and conversation |
| 3 | report, including shape-decision notes for the commit message |

## A comment states what the code does, so the code is its source

A comment is pitched at the caller: what the item does for them, not why it was
built this way. That is checkable against the code, so the pass needs no
recorded context. A *why* about the body's shape is a commit-message note, which
the pass hands on rather than writing.

The pass used to require a justification line above every test and to treat
every undocumented public item as work left over, taking unverifiable reasons
from `business-context.md` and reporting "context gaps". Both pushed it to write
comments, and the comments it wrote were the restatements and conversation the
rule is meant to remove; both are gone.

## The detector

`scripts/comment-candidates.py` finds the four placement states the rule can
decide without reading the code:

| category | means |
|---|---|
| `BARE-COMMENT` | a comment where the rule allows none — a body, a banner, a stray |
| `PRIV-DOC` | a doc comment on a private element |
| `PUB-UNDOC` | a public element with no doc comment — a candidate; leaving it uncommented is the default |
| `MOD-UNDOC` | a module with no header comment |

Rust, nushell, shell and the justfile, using the module/public mapping from the
rule. A comment above a test is a `BARE-COMMENT` — the test's name says what it
pins. `// SAFETY:` is exempt because `clippy::undocumented_unsafe_blocks`
requires it.

Whether a comment is conversational is not detected: in this repo gates, issues
and notes files are the domain, so a pattern over those words flags legitimate
contracts. That judgment is stage 2's.

It is python rather than bash — the repo's other scripts are bash, but this one
parses four languages and correctness beat consistency.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/comment-pass                  # the branch diff
/comment-pass main             # diff against an explicit base
/comment-pass tools/hdash      # sweep a path
/comment-pass . --report-only  # count without changing anything
```

## Known Limitations

- The detector decides *placement*, never content. A module header that lists its
  contents, a `///` that restates the signature or refers to the conversation, is
  stage 2's job and needs the code read.
- "Public" in a shell script is not mechanically knowable — the script guesses
  from a leading underscore and the skill is expected to correct it.
- Vendored crates (`mcps/*`, the four copied `lsps/*`) are report-only by default,
  because a rewrite there widens the drift a re-sync has to reconcile.
- Heredoc bodies in shell and justfile recipes produce false `BARE-COMMENT` hits:
  a `#` inside an indented heredoc is data, and the detector reads it as a
  comment. Verified, not theoretical.
- Python is not covered — `scripts/comment-candidates.py` is the repo's only
  python file, and it does not scan itself.
- `--diff` scopes on `git diff <base>` plus untracked files, not `<base>...HEAD`,
  because the pass runs before the work is committed. That means it also sees
  unrelated dirt in the working tree.
