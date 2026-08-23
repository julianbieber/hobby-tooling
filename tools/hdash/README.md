# hdash

| Field        | Value                                                       |
| ------------ | ----------------------------------------------------------- |
| Type         | desktop app (Bevy)                                          |
| Binary       | `hdash`                                                     |
| Owner        | @julianbieber                                               |
| Dependencies | `hwt`/`hcdev` on PATH, alacritty, nushell, `gh` (authenticated) |
| Status       | experimental                                                |

## Description

A desktop dashboard of every hobby work item: **one card per piece of work**,
wherever it sits in the lifecycle. An open GitHub issue nobody has started, the
worktree it became, the PRs that came out of it, and the day it was marked done are
all the same card — not four rows in two windows.

Replaces `hwt-overview` and `hgh-issues`, which were the same app over two halves of
one list.

## The card is a lifecycle, and the primary button is the next step

| Stage | Condition | Primary button |
| --- | --- | --- |
| orphan | a worktree with no `task.json` in the notes repo | **Adopt** — writes the `task.json` |
| untagged | an issue with no usable `topic:` label | *(none — a line names the missing label)* |
| startable | a tagged issue with no worktree | **Start task** — `hwt new` then `hcdev wt` |
| active | a live worktree | **Open hcdev** |
| removed | worktree gone, notes kept | **Restore** — `hwt restore` |
| done | `done` in `task.json` | **Open hcdev** (and **Reopen**) |

Plus, as they apply: **Done**/**Reopen**, **Terminal**, **Notes**, **Browser** (any
card with an issue half — something neither predecessor could offer), **Hide**.

## A task exists because the notes repo says so

`notes/<project>/<topic>/<task>/task.json` is what makes a task, and the only thing that
does. `~/fun_worktrees` is not enumerated: it answers `present`, and nothing else.

| | source | answers |
| --- | --- | --- |
| a task exists | `notes/P/T/TASK/task.json` | which tasks there are |
| a task is checked out | `~/fun_worktrees/P/T/TASK` | `present` — Active vs Removed |
| a task is done / blocked | `task.json` | the badges and the sort rank |
| what the work *is* | the GitHub issue | title, labels, author, `topic:` |

A worktree with no `task.json` behind it is an **orphan**: on disk, recorded nowhere. It
gets a card at the top of the list with an **Adopt** button that writes the file, byte for
byte what `hwt new` writes. `hwt orphans` lists the same set from the CLI.

`hwt` applies the identical rule (`known-tasks` reads the notes tree), so `hwt list | length`
and the dashboard's task count agree.

TODO(jb-doc): why the notes repo, and not the worktree tree, is the thing that decides

TODO(jb-doc): why an orphan is surfaced for a click rather than adopted during the scan

## The join is what makes it one list

An issue's worktree is named `gh-<number>-<slug>`, so the two halves are matched on
the **number**:

```
issue #7  +  ~/fun_worktrees/wusel/perf/issue-7_chunk_loading   ->  one card
```

Matching on the number rather than on the derived name is what catches the
pre-convention spellings (`issue-9_dotfile_repo_retire`) and survives an issue title
being edited. `gh-42-x`, `gh42-x`, `issue-42_x` and bare `gh-42` all belong to issue
42; `gh-421-x` and `some-task-42` do not.

The match never crosses projects, and if two worktrees claim one issue the first in
`(project, topic, name)` order wins while the other keeps its own card — nothing is
silently dropped. **Open hcdev** always takes topic and task from the worktree path,
never from the `topic:` label, because the path is what `hcdev wt` has to find.

Because the task half already holds `~/fun_worktrees`, the issue half touches no
filesystem at all — the per-issue directory walk `hgh-issues` did is gone.

## `topic:<name>` is the one thing an issue cannot supply

A worktree lives at `~/fun_worktrees/PROJECT/TOPIC/TASK`. An issue implies two of
those three — the project is the repo it was filed in, the task is
`gh-<number>-<slug>` — so the topic rides on a label:

```
labels: [topic:features, bug]   ->  ~/fun_worktrees/<proj>/features/gh-42-<slug>
labels: [bug, enhancement]      ->  untagged; nothing to launch
```

Prefixed rather than bare, because a repo's ordinary labels (`bug`, `enhancement`,
`good first issue`) must never be read as topics, and the prefix lets an issue name a
topic whose directory does not exist yet — the normal case. The value must look like
a directory name (`[A-Za-z0-9._-]+`, no leading dot); anything else is treated as
*untagged* rather than escaped, since it becomes both a path component and a shell
argument. `/gh-issue-create` writes the label, so issues opened through the skill
arrive launchable.

## Order and filters

The list is **flat and globally sorted** — not grouped by topic, because an unstarted
issue has a topic *label* and no topic *directory* to group under. Rank, then most
recent activity, then name:

```
0  orphan worktree                  5  issue tagged, no worktree
1  task, block lifted ("ready")     6  task removed-but-remembered
2  task active, PR awaiting review  7  issue untagged
3  task active                      8  task done
4  task blocked
```

This keeps both predecessors' orders: `hwt-overview`'s ready < active < blocked <
done and `hgh-issues`' worktree < tagged < untagged.

Two content toggles, both with load-bearing defaults:

- **Show done** — off. Done tasks outnumber open ones ~25:1 and would swamp the list.
- **Show untagged** — on. An untagged issue is the one item the app cannot act on, so
  it is the one that most needs to be seen.

Neither toggle can hide an orphan.

**"All" is a task list.** Issues are queried for the selected project only (see *Fetching*),
so with no project selected there is nothing to show but tasks — an issue-only card under
"All" would report whichever projects happened to be visited this session and no others.
Select a project to see its issues.

The sidebar filters by project, and leads with the projects that have a `notes/<project>/`
directory; the rest of `~/fun_repos` follows after a divider. Every project keeps a row —
with issues fetched per selection, a missing row would mean unreachable issues. Counts
ignore the project selection and session hides (a dismissal should not make a project's
total drop) but do honour both toggles, so a project's badge can exceed the number of its
cards visible under "All".

TODO(jb-doc): why selection, rather than a denylist or an ownership filter, is what scopes
the issue query

## Pull — the one thing here that writes to a repo

Each project's sidebar row carries a **Pull** button that runs `git pull --ff-only` in
that project's main checkout (`~/fun_repos/<project>`), in the background. Not on the
"All" row: `~/fun_repos` holds upstream clones too (see the limitations below), so one
click that pulls every checkout would fire dozens of network fetches nobody asked for.

`--ff-only` is the whole safety story. A diverged or dirty checkout is **refused**, with
git's own reason on a line above the item list, rather than silently growing a merge
commit or dropping an editor onto a terminal nobody is watching. Credential prompting is
turned off (`GIT_TERMINAL_PROMPT=0`, ssh `BatchMode`) for the same reason: a prompt would
otherwise hang a pool thread for the rest of the session.

While a pull runs the button becomes a `…`, so there is nothing to click twice. The
outcome — `pulled 3 commits`, `already up to date`, or `pull failed: <reason>` — is a
line at the top of the item list, shown whichever project is selected, because the button
that started it is visible from every project. Successes fade after ten seconds;
failures stay until the next pull or **Refresh**.

Worktrees are untouched. This pulls the checkout a `hwt new` branches *from*, not the
branch you are working on.

## Fetching

Three async reads, all through one `Fetch<K, M>` keyed at-most-once fan-out onto the
`IoTaskPool`:

| | key | scope | cache |
| --- | --- | --- | --- |
| git status | (task, repo) | every present task | none |
| PR status (`gh pr view`) | (task, PR) | visible items | terminal states, on disk |
| issues (`gh issue list`) | project | the **selected** project only | none |
| pull (`git pull --ff-only`) | project | clicked projects only | none |

Git is eager because a local `git status` is cheap and the branch is what makes the
list readable immediately. The other two reads are lazy: `gh` is a network call. The
pull is neither — it writes, so it happens on a click and never as a side effect of a
filter change, and its guard is dropped as soon as it answers so the same repo can be
pulled again.

Issues go one step further than lazy: under "All" **no** issue query is made at all.
`~/fun_repos` holds ~50 checkouts, and querying every one of them on startup is what the
Known Limitations below used to describe.

The PR cache holds only **merged/closed** statuses — the states that can never change
— in `~/.cache/hdash/pr-status.json`, falling back once to `hwt-overview`'s old path
so the rename does not cost a re-query of every merged PR. **Refresh** re-scans and
re-queries but keeps the cache; **Clear cache** wipes it so merged/closed PRs are
checked again too. Open issues are never cached: they change on GitHub's side, so a
cache could only serve stale data.

## Install

```bash
just install-dash     # from ~/fun_repos/hobby-tooling
hdash
```

If a machine still has the retired binaries on `PATH`:

```bash
cargo uninstall hwt_app issues_app
```

During development:

```bash
just run            # dev profile (deps are optimized, so it is usable)
just run-release
just test
just shot           # screenshot the window (needs xdotool + imagemagick)
```

## Layout

```
crates/feathers_kit  — a fork of the work repo's kit; domain-agnostic Bevy UI helpers
                       (cards, rows, badges, scroll lists, tokens). The fork exists so
                       the work dashboard cannot regress; this is the only copy here.
crates/hwt_core      — well-known paths, detached process spawning, date/time helpers
crates/hdash_app     — model/{task,issue,join,launch} and ui/{chrome,card,fetch}
```

## Known Limitations

- **`~/fun_repos` holds upstream clones, not just my projects.** Selecting `bevy` or
  `helix` still fetches its 200 open issues (the fetch cap), none of which is work here.
  There is no denylist or ownership filter — only the selection gate, which keeps them out
  of the default view.
- A **closed** issue whose task still exists shows as an unjoined task:
  `gh issue list --state open` never returns it. Closing an issue therefore does not mark
  its task done, and marking a task done does not close its issue — `task.json` is the only
  authority on `done`.
- **The issue ↔ task link is parsed from the directory name** (`matches_issue`), not
  recorded, so a task named for no issue can never join one.
- Projects with no GitHub remote (`bevy_asset_lsp` is on codeberg) and forks with
  issues disabled report a red line. Aggregated to one line while "All" is selected;
  select the project to see the reason.
- No issue *editing*: labels, comments, and closing are `gh`/`/gh-issue-*` work.
- Launch actions assume alacritty + nushell with `hwt.nu`/`hcdev.nu` loaded.
- `Start task` runs `hwt new`, which needs the project's main checkout at
  `~/fun_repos/<project>`.
