# MML Translate

| Field        | Value                                                                                                                                                            |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                            |
| Install path | `~/.claude-hobby/commands/mml-translate.md`                                                                                                                                        |
| Owner        | @julianbieber                                                                                                                                                    |
| Dependencies | MML grammar (`llm_mental_model/01-language-spec`), per-task specs in the notes repo `~/fun_repos/hobby-mimisbrunnr`, `tree-extract` & `lsp-mcp` MCPs |
| Status       | experimental                                                                                                                                                     |

## Description

Translates an **edited** [`.mml`](~/projects/team-constellation-mimisbrunnr/notes/llm_mental_model/DESIGN.md)
(Mental Model Language) spec into the **code change** that makes the real system
match it — spec → code, not prose → code. This is the payoff step of the MML
loop: `mml-extract` turns code into a `.mml`, the human edits the `.mml`
to express the change they want, and this skill turns that edit into a minimal,
reviewable diff against the real repo.

It works **delta-based**: specs live per-task in the notes repo
`~/fun_repos/hobby-mimisbrunnr` (a git repo), so the last committed
`.mml` there is the baseline and the human's working-tree edits are the delta.
The skill translates only the delta (a minimal diff), applies it to the task's
**worktree** checkout of the source repo — never the shared `~/fun_repos/<repo>`
— verifies it (compile/tests), and reports the diff. Full generation is a
fallback for a brand-new spec with no committed baseline.

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
ln -s ~/fun_repos/hobby-tooling/skills/mml-translate/skill.md \
  ~/.claude-hobby/commands/mml-translate.md
```

## Usage

```
/mml-translate terrain-erosion/mesh-terrain-chunks
```

Point it at a `.mml` (a path, or a `system/unit` pair) that you have edited. It
diffs the spec against its last commit, translates the delta into code in the
task's worktree, runs the project's compile/tests, and shows you the diff.

## Known Limitations

- **No `.mml` parser exists yet** (task 06), so the spec is validated against
  the grammar by eye until it does.
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
