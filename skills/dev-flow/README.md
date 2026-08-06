# dev-flow

| Field        | Value                                  |
| ------------ | -------------------------------------- |
| Type         | skill                                  |
| Install path | `~/.claude-hobby/commands/dev-flow.md` |
| Owner        | @julianbieber                          |
| Dependencies | `resume-journal`, HMML suite, `pr-facts`, `journal`; notes repo |
| Status       | experimental                           |

## Description

Thin orchestrator for my default development loop, run as a predictable pipeline
with two hard human gates:

```
read task → HMML plan → self-review → [GATE 1] → implement → self-review
  → test → [GATE 2] → PR → journal
```

Resumable: phase state lives in `dev-flow.json` in the task's notes dir, so a
later session picks up exactly where it stopped. Never auto-commits.

## Installation

Installed by `just install-skills` from `~/fun_repos/hobby-tooling`.

## Usage

```
/dev-flow                    # resume at the recorded/inferred phase
/dev-flow plan               # force a phase
/dev-flow --no-hmml           # skip the HMML steps, plan inline
/dev-flow --resume-deep      # use resume-deep instead of resume-journal
```

## Known Limitations

- **Step 7 reviews inline, not via `/complete-review`.** The review skills are
  not installed on the hobby account, so the self-review is a single-pass
  four-lens read of the diff rather than six parallel subagents. Install the
  review skills if you want the full treatment.
- The PR phase is optional here — hobby repos are solo, so pushing straight to
  the default branch is sometimes right. The skill asks rather than assuming.
- If the state file and the on-disk artifacts disagree, the artifacts win.
