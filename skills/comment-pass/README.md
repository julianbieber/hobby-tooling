# comment-pass

| Field        | Value                                                   |
| ------------ | ------------------------------------------------------- |
| Type         | skill                                                   |
| Install path | `~/.claude-hobby/commands/comment-pass.md`              |
| Owner        | @julianbieber                                           |
| Dependencies | `scripts/comment-candidates.py`, python3, `business-context.md` (optional) |
| Status       | experimental                                            |
| Supersedes   | `comment-review`, `explain-back`'s comment pass         |

## Description

Enforces the comment rule in `dotfiles/claude-md/hobby.CLAUDE.md` — a module
header, a public element's contract, a test's justification, nothing else — over
the branch diff or a given path.

**It is a two-way pass.** The skill it replaces (`comment-review`) could only
delete, and paired with `explain-back` to do the writing; when both were removed
with the AI-policy stack there was nothing left. Deleting a disallowed comment and
writing a missing one are one job here, because the same reading of the code
decides both.

## Stages

| stage | does |
|---|---|
| 0 | run `comment-candidates.py`, report counts, set vendored crates to report-only |
| 1 | resolve the five mechanical categories — delete, relocate, or write |
| 2 | read the legal comments against the code; rewrite the wrong ones |
| 3 | report, including shape-decision notes for the commit message and the context gaps |

## Two sources for a sentence, and no third

The pass may assert a reason it **verified in the code**, or one it **read in
the recorded business context** — the `business-context.md` the `/task:build`
pipeline writes at GATE C, in my own words, before the delta was designed. A
site that needs a *why* neither supplies is reported as a **context gap** with
its `file:line`, never filled in.

That is what makes the pass able to write a contract at all. Most of what a
caller needs is in the code; the part that is not — which rule sets the bound,
what an upstream system guarantees, why the obvious shape was rejected — is
something only I know, and asking for it after the code exists is asking me to
judge code I did not write. So it is collected first and matched here.

A gap is closed by extending the context, which is `/task:build context`. It is
never closed by the pass writing the sentence itself: a confident, plausible,
wrong comment is load-bearing for the next reader and invisible, where a missing
one is merely missing.

## The detector

`scripts/comment-candidates.py` finds the five states the rule can decide without
reading the code:

| category | means |
|---|---|
| `BARE-COMMENT` | a comment where the rule allows none — a body, a banner, a stray |
| `PRIV-DOC` | a doc comment on a private element |
| `PUB-UNDOC` | a public element with no doc comment |
| `MOD-UNDOC` | a module with no header comment |
| `TEST-UNJUSTIFIED` | a test with no line saying why it exists |

Rust, nushell, shell and the justfile, using the module/public mapping from the
rule. Two exceptions are built in: a comment directly above a test is that test's
justification and not a violation, and `// SAFETY:` is kept because
`clippy::undocumented_unsafe_blocks` requires it.

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
/comment-pass --no-context     # no recorded context exists, and that is deliberate
```

## Known Limitations

- The detector decides *placement*, never content. A module header that lists its
  contents, or a `///` that restates the signature, is stage 2's job and needs the
  code read.
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
