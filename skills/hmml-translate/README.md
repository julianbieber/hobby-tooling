# HMML Translate

| Field        | Value                                                                                                                                                            |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                            |
| Install path | `~/.claude-hobby/commands/hmml-translate.md`                                                                                                                                        |
| Owner        | @julianbieber                                                                                                                                                    |
| Dependencies | HMML grammar (`lsps/hobby-mml-lsp/grammar.md`), per-task specs in the notes repo `~/fun_repos/hobby-mimisbrunnr`, `tree-extract` & `lsp-mcp` MCPs |
| Status       | experimental                                                                                                                                                     |

## Description

Translates an **annotated** [`.hmml`](../../lsps/hobby-mml-lsp/grammar.md)
(Mental Model Language) spec into the **code change** that makes the real system
match it — spec → code, not prose → code. This is the payoff step of the HMML
loop: `hmml-extract` turns code into a `.hmml` baseline, the human annotates it
with the change they want, and this skill turns those annotations into a minimal,
reviewable diff against the real repo.

It works **delta-based**, and the delta is normally in the file: the spec holds
the baseline plus `# +` / `# -` change annotations (grammar §10), which are the
work list — the unannotated lines around them are code that already exists. An
unannotated spec falls back to the git delta (specs live in the notes repo
`~/fun_repos/hobby-mimisbrunnr`, a git repo, so the last committed `.hmml` is the
baseline and the working-tree edit is the delta). The skill applies it to the
task's **worktree** checkout of the source repo — never the shared
`~/fun_repos/<repo>` — verifies it (compile/tests), and reports the diff marker
by marker. Full generation is the last resort, for a brand-new spec with neither.
It never edits the spec, including stripping annotations — the markers stay after
the code lands, so the spec keeps representing the change.

Produces:

- A code change applied to the task's worktree checkout of `source.repo`, ready
  for the human to review and commit.
- (Fallback, when no writable worktree is available) a `.patch` file against the
  source checkout, with verification skipped.

Applies the change and stops — reviewing and committing the code is the human's
job (that is the contract, same as extract). It never runs `git add` /
`git commit`, and never touches the spec in the notes repo or other specs.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/hmml-translate/skill.md \
  ~/.claude-hobby/commands/hmml-translate.md
```

## Usage

```
/hmml-translate terrain-erosion/mesh-terrain-chunks
```

Point it at a `.hmml` (a path, or a `system/unit` pair) that you have edited. It
diffs the spec against its last commit, translates the delta into code in the
task's worktree, runs the project's compile/tests, and shows you the diff.

## Known Limitations

- The spec is checked with `hobby-mml-parser <path> --lint` before translating;
  a clean lint means well-formed, not correct.
- **Delta fidelity is model-dependent.** The value is a minimal, correct diff
  from the spec edit; treat the output as a change to review, never as
  automatically correct. In particular, a reworded step should map to no change
  — over-eager rewriting is the main risk.
- **Deletion is flagged, not auto-applied.** A removed behavior step does not
  license deleting code the spec never modeled (the spec is lossy); ambiguous
  removals are surfaced for the human to decide.
- **Requires a committed baseline for the no-op guarantee.** Extract → no edit →
  translate is a genuine no-op only in delta mode with the extracted spec
  committed; running full-generation on an unedited spec will drift.
- **Never edits `~/fun_repos/<repo>`** (the shared checkout) and never creates a
  worktree for you — worktree creation stays with the human.
- Applies and stops — it does not iterate with you, and it never runs `git add`
  / `git commit` in any repo, nor touches the spec in the notes repo or other
  specs.
