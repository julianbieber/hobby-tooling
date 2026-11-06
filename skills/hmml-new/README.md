# HMML New

| Field        | Value                                                                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | skill                                                                                                                                                           |
| Install path | `~/.claude-hobby/commands/hmml-new.md`                                                                                                                                             |
| Owner        | @julianbieber                                                                                                                                                   |
| Dependencies | HMML grammar (`lsps/hobby-mml-lsp/grammar.md`), notes repo `~/fun_repos/hobby-mimisbrunnr`, `template{,-plugin,-system,-module}.hmml` (skill assets) |
| Status       | experimental                                                                                                                                                    |

## Description

Scaffolds a skeleton [`.hmml`](../../lsps/hobby-mml-lsp/grammar.md) (Mental
Model Language) spec for a **new** unit of behaviour — one that has no code yet
— so I can edit it into the shape I want and think the design through. This is the "building a new feature" entry to the HMML loop; its
sibling `hmml-extract` is the "adjusting an existing feature" entry (code
→ `.hmml`).

Two paths, chosen from what I give it:

- **Blank template** — I want to write the spec by hand; the skill drops a
  correctly-structured empty spec (from the skill asset matching the `kind`: `template.hmml`, `template-plugin.hmml`, `template-system.hmml`, or `template-module.hmml`) at the
  right path so I start well-formed.
- **Draft from a rough idea** — I describe the feature in a sentence or two and
  the skill drafts a first-pass `.hmml` (best-guess modules / data / behavior /
  invariants), clearly marked `# DRAFT`, that I then edit down. The drafted body
  is marked `# +` (grammar §10) — for a new unit that is most of the file, which
  is honest: it says everything here is new.

Either way the baseline comes first. When the task dir already holds a
`<unit>.hmml` extracted from the code that will host the new behaviour, the
skill drafts *into* that file as `# +` additions rather than writing a second
one, so the file keeps showing current-vs-intended side by side.

Produces:

- `~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml` —
  one spec, header + the four `##` sections, placed directly in the current
  task's notes dir. A new unit usually has no `source` block yet.

Writes the `.hmml` and stops — reviewing, editing, and committing it are my job.
It is a thinking aid, not a plan the code is generated from: once code exists,
`/hmml-extract --against` describes what was actually built.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`, or manually:

```bash
mkdir -p ~/.claude-hobby/commands/
ln -s ~/fun_repos/hobby-tooling/skills/hmml-new/skill.md \
  ~/.claude-hobby/commands/hmml-new.md
```

## Usage

```
/hmml-new a nightly job that purges price-history older than the retention window, per contract
/hmml-new blank endpoint spec for bevy-survivor called spawn-wave
```

Give it a rough idea (it drafts a first-pass spec) or ask for a blank template
to fill in yourself. Either way it writes
`~/fun_repos/hobby-mimisbrunnr/notes/{project}/{topic}/{task}/<unit>.hmml`; you
then edit it.

## Known Limitations

- **A draft is a best guess, not a spec to trust.** The value is a starting
  point to react to and edit down; the modules, data model, and behavior are the
  model's guess from a one-line idea, never ground truth.
- Writes and stops — it does not iterate with you, and it never runs `git add` /
  `git commit` in the notes repo.
- A new unit typically has **no `source` block** until you know the target repo
  and paths.
- Output is checked with `hobby-mml-parser <path> --lint`; a clean lint means
  well-formed, not that the draft says the right thing.
