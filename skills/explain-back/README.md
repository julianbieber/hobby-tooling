# Explain Back

| Field        | Value                                     |
| ------------ | ----------------------------------------- |
| Type         | skill                                     |
| Install path | `~/.claude-hobby/commands/explain-back.md` |
| Owner        | @julianbieber                             |
| Dependencies | none                                      |
| Status       | experimental                              |

## Description

Walks the branch diff hunk by hunk. For each one it shows the code, asks what it
does, waits, and grades the answer **right / incomplete / wrong**. Ends with a
list of the hunks I could not explain.

Enforces Bevy AI policy norm 1 — *you must personally understand every line you
submit* — which no hook can check.

Runs at Gate 2.5 of `/dev-flow`, before the PR gate.

## Installation

```bash
just install-skills
```

## Usage

```
/explain-back
```

## Known Limitations

- Long diffs take a long time. That is the cost of the norm, not a defect, but it
  argues for the norm-4 size budget at Gate 0.
- Cannot tell a correct explanation from a confidently-worded wrong one on code
  whose behaviour depends on state it cannot see.

## TODO(jb-doc): what a failed hunk means and what to do about it

<!-- What it should cover: why the answer is not "have it explained again until
     it sounds fine", and what the three options at the end are for. -->
