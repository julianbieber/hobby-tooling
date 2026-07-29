# hgh-issues

| Field        | Value                                                       |
| ------------ | ----------------------------------------------------------- |
| Type         | desktop app (Bevy)                                          |
| Binary       | `hgh-issues`                                                |
| Owner        | @julianbieber                                               |
| Dependencies | `gh` (authenticated), `hwt`/`hcdev` on PATH, alacritty, nushell |
| Status       | experimental                                                |

## Description

A desktop dashboard of the **open GitHub issues** of every hobby project, with one
click from an issue to a running dev session. Pick a project in the sidebar; each
issue is a card showing its number, title, topic tag, other labels, the
task/branch name it would get, and whether a worktree for it already exists.

The primary button is the point of the app:

| Issue state | Button | What it runs |
| --- | --- | --- |
| worktree exists | **Open hcdev** | `hcdev wt <task> --project <p> --topic <t>` |
| tagged, no worktree | **Start task** | `hwt new <task> -p <p> -t <t> --no-cd` then the same `hcdev wt` |
| untagged | *none* | a line naming the missing label |

Both run in one detached alacritty through `nu --login`, which is what makes
`hwt.nu`/`hcdev.nu` reachable. Plus **Browser** (the issue on GitHub),
**Terminal** (in the worktree, when there is one), and **Hide** (this session only).

## The `topic:` tag is the whole idea

A worktree lives at `~/fun_worktrees/PROJECT/TOPIC/TASK`. An issue supplies two of
those three by itself — the project is the repo it was filed in, the task is
`gh-<number>-<slug>` from its number and title. **Topic is the missing one**, so
the issue carries it as a label:

```
labels: [topic:features, bug]   ->  ~/fun_worktrees/<proj>/features/gh-42-<slug>
labels: [bug, enhancement]      ->  untagged; nothing to launch
```

Prefixed rather than bare, for two reasons: a repo's ordinary labels (`bug`,
`enhancement`, `good first issue`) must never be read as topics, and the prefix
lets an issue name a topic whose directory does not exist yet — the normal case,
since a topic dir only appears with the first worktree under it.

A topic value has to look like a directory name (`[A-Za-z0-9._-]+`, no leading
dot). Anything else — `topic: features` with a stray space, `topic:a/b` — is
treated as *untagged* rather than escaped, because the value becomes both a path
component and a shell argument.

`/gh-issue-create` writes these labels (creating the label in the repo when it is
missing), so issues opened through the skill arrive ready to launch.

## Finding an existing worktree

By **issue number**, not by the derived task name. Tasks predating this convention
are spelled differently (`issue-9_dotfile_repo_retire`), and a slug changes
whenever the issue title is edited. So a task directory is claimed by issue 42 when
its name opens with `gh`/`issue`/`issues` followed by exactly that number:
`gh-42-x`, `gh42-x`, `issue-42_x`, bare `gh-42`. `gh-421-x` and `some-task-42` do
not match.

When such a worktree exists, **Open hcdev** takes the topic and task name from the
directory path rather than from the issue, since the path is what `hcdev wt` has to
find — the label may have been changed since.

## How it relates to hwt-overview

`hwt-overview` starts from the worktrees that exist; this starts from the issues
that have no worktree yet. They are deliberately separate windows over the same
tree, and this one **reuses** rather than copies:

```
crates/issues_app          the model (issues.rs) and the UI (ui.rs)
../hwt-overview/crates/feathers_kit   path dependency — domain-agnostic Bevy UI kit
../hwt-overview/crates/hwt_core       path dependency — paths, process spawning, dates
```

Same pattern as `mcps/tree-extract` depending into `lsps/`. The one copy of
`feathers_kit` in this repo exists so the *work* dashboard cannot regress; a second
hobby copy would only drift.

## Fetching

One `gh issue list --state open` per project, run from the project's checkout so
`gh` resolves the repo from its own remote, dispatched on the `IoTaskPool` and
streamed back over a channel. Only **visible** projects are fetched, guarded so a
project is queried once per scan — `~/fun_repos` holds repos that are not on GitHub
at all, and a failing `gh` renders as a dim per-project line rather than a hang.

There is no on-disk cache, unlike `hwt-overview`'s PR-status cache: open issues
change on GitHub's side, so a cache could only serve stale data. **Refresh**
re-scans the projects and re-queries.

## Install

```bash
just install-issues     # from ~/fun_repos/hobby-tooling
hgh-issues
```

Or during development:

```bash
just run            # dev profile (deps are optimized, so it is usable)
just run-release
just test
```

## Known Limitations

- Projects with no GitHub remote (`bevy_asset_lsp` is on codeberg) and forks with
  issues disabled always show a red `no issues from gh: …` line. There is no
  denylist yet.
- No issue *editing*: labels, comments, and closing are `gh`/`/gh-issue-*` work.
  This app reads issues and starts tasks.
- Launch actions assume alacritty + nushell with `hwt.nu`/`hcdev.nu` loaded, same
  as `hwt-overview`.
- `Start task` runs `hwt new`, which needs the project's main checkout at
  `~/fun_repos/<project>`.
