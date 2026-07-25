# hwt-overview

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| Type         | desktop app (Bevy)                          |
| Binary       | `hwt-overview`                              |
| Owner        | @julianbieber                               |
| Dependencies | `hwt`/`hcdev` on PATH, alacritty, nushell, `gh` |
| Status       | experimental                                |

## Description

A desktop dashboard over every hobby task: live worktrees under
`~/fun_worktrees/PROJECT/TOPIC/TASK` unioned with notes dirs carrying a
`task.json`, so removed-but-remembered tasks stay visible and can be restored.

Per task: git status (branch, HEAD, dirty count), done/blocked/ready state, open
PRs with live status from `gh`, and subtasks spun off via `/spawn`. Cards act:
toggle done, open a terminal, open the notes, launch an `hcdev` session, or
`hwt restore` a removed task.

## How it differs from the work dashboard

Adapted from `julian-wt-overview`. Two model differences drive everything:

**Three levels, and the task dir is the worktree.** `TaskId` carries
`(project, topic, name)`. A hobby task is a single repo, so `scan()` no longer
enumerates repo subdirectories — each task gets exactly one `Repo` entry pointing
at itself, named after its project. Keeping that a vector rather than a scalar is
deliberate: the async git-status plumbing and the card rendering work unchanged.

**One author, so no owner filter.** The work notes repo is shared, so a scan turns
up teammates' tasks and the UI needed an owner bar plus per-task owner badges and
attribution via `git log`. Here there is one author. The whole owner-filter bar,
its buttons, `passes_owner`, and the owner badge are removed — roughly 130 lines
gone rather than left as dead configuration.

Consequently **the sidebar filters by project**, where the work version filtered by
topic. Project is the coarsest useful axis for hobby work. Because topic is no
longer in the sidebar, each card now shows its topic beside the task name — without
that, a filtered view gave no clue where a task sat within its project.

`pr-overview` was not ported; at hobby PR volume `gh pr list` and `/pr-comments`
cover it.

## Layout

```
crates/feathers_kit  — copied verbatim from the work repo; domain-agnostic Bevy
                       UI helpers (cards, rows, badges, scroll lists, tokens)
crates/hwt_core      — well-known paths (~/fun_worktrees, ~/fun_repos, the hobby
                       notes repo), detached process spawning, time formatting
crates/hwt_app       — the model (worktree.rs) and the UI (ui.rs)
```

## Install

```bash
just install-overview     # from ~/fun_repos/hobby-tooling
hwt-overview
```

Or during development:

```bash
cargo run -p hwt_app          # dev profile (deps are optimized, so it is usable)
cargo run -p hwt_app --release
```

## Known Limitations

- `feathers_kit` (521 lines) is duplicated from the work repo rather than shared.
  A fork was chosen over a shared crate so the work dashboard cannot regress.
- PR status needs `gh` authenticated; results are cached under
  `~/.cache/hwt-overview/pr-status.json` (terminal states only).
- Launch actions assume alacritty + nushell with `hwt.nu`/`hcdev.nu` loaded.
