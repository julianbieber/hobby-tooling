# hdash

| Field        | Value                                                       |
| ------------ | ----------------------------------------------------------- |
| Type         | desktop app (Bevy)                                          |
| Binary       | `hdash`                                                     |
| Owner        | @julianbieber                                               |
| Dependencies | `hwt`/`hcdev` on PATH, alacritty, nushell, `gh` (authenticated, for PR status) |
| Status       | experimental                                                |

## Description

A desktop dashboard of every hobby work item: **one card per task in the notes repo**,
wherever it sits in the lifecycle. A task nobody has started, the worktree it became,
the PRs that came out of it, and the day it was marked done are all the same card.

The card's headline is the `title:` in the task's `issue.md` frontmatter (what
`/issue-create` writes), falling back to the task name.

## The card is a lifecycle, and the primary button is the next step

| Stage | Condition | Primary button |
| --- | --- | --- |
| orphan | a worktree with no `task.json` in the notes repo | **Adopt** — writes the `task.json` |
| not started | `task.json`, no worktree, and no branch of that name in the main checkout | **Start task** — `hwt restore` then `hcdev wt` |
| active | a live worktree | **Open hcdev** |
| removed | worktree gone, branch and notes kept | **Restore** — `hwt restore` then `hcdev wt` |
| done | `done` in `task.json` | **Open hcdev** (and **Reopen**) |
| coordinator | a task under the reserved `coordinator` topic | **Open coordinator** — `hcdev coordinator` in the main checkout; the card shows the `tasks.json` counts |

Plus, as they apply: **Done**/**Reopen**, **Backlog**/**Unbacklog**, **Terminal**,
**Notes**, **Hide**.

*Not started* and *removed* run the same command: `hwt restore` re-creates a worktree on
its existing branch, and creates the branch from the main checkout's `HEAD` when there is
none. The branch check is one `git for-each-ref` per project with a worktree-less task,
during the scan; a checkout that cannot be read leaves its tasks at *removed*.

## A task exists because the notes repo says so

`notes/<project>/<topic>/<task>/task.json` is what makes a task, and the only thing that
does. `~/fun_worktrees` is not enumerated: it answers `present`, and nothing else.

| | source | answers |
| --- | --- | --- |
| a task exists | `notes/P/T/TASK/task.json` | which tasks there are |
| a task is checked out | `~/fun_worktrees/P/T/TASK` | `present` — Active vs Removed / Not started |
| a task was ever started | a branch named after it in `~/fun_repos/P` | Removed vs Not started |
| a task is done / blocked | `task.json` | the badges and the sort rank |
| an item is backlogged | `task.json` | which list mode shows it |
| what the work *is* | `notes/P/T/TASK/issue.md` | the card's title |

A worktree with no `task.json` behind it is an **orphan**: on disk, recorded nowhere. It
gets a card at the top of the list with an **Adopt** button that writes the file, byte for
byte what `hwt new` writes. `hwt orphans` lists the same set from the CLI.

`hwt` applies the identical rule (`known-tasks` reads the notes tree), so `hwt list | length`
and the dashboard's task count agree.

TODO(jb-doc): why the notes repo, and not the worktree tree, is the thing that decides

TODO(jb-doc): why an orphan is surfaced for a click rather than adopted during the scan

## Order and filters

The list is **flat and globally sorted**, with project and topic printed on the card.
Rank, then most recent activity, then name:

```
0  orphan worktree                  5  task not started
1  task, block lifted ("ready")     6  task removed-but-remembered, or a coordinator
2  task active, PR awaiting review  8  task done
3  task active
4  task blocked
```

**Show done** is off by default: done tasks outnumber open ones ~25:1 and would swamp the
list. **Show backlog** switches the list: off, backlogged open work is hidden; on, only
backlogged items that are not done are shown. The **Backlog** button writes `backlog` into
`task.json`, after `blocked`, in the same key order `hwt` writes.

No toggle can hide an orphan.

The sidebar lists the projects with a `notes/<project>/` directory — an upstream clone
under `~/fun_repos` with no notes gets no row — plus any project an orphan worktree sits
in. Counts ignore the project selection and session hides (a dismissal should not make a
project's total drop) but do honour the toggles, so a project's badge can exceed the
number of its cards visible under "All".

## Pull — the one thing here that writes to a repo

Each project's sidebar row carries a **Pull** button that runs `git pull --ff-only` in
that project's main checkout (`~/fun_repos/<project>`), in the background. Not on the
"All" row, and not on a project with no checkout at `~/fun_repos/<project>`.

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

Two async reads and one write, all through one `Fetch<K, M>` keyed at-most-once fan-out onto the
`IoTaskPool`:

| | key | scope | cache |
| --- | --- | --- | --- |
| git status | (task, repo) | every present task | none |
| PR status (`gh pr view`) | (task, PR) | visible items | terminal states, on disk |
| pull (`git pull --ff-only`) | project | clicked projects only | none |

Git is eager because a local `git status` is cheap and the branch is what makes the
list readable immediately. The PR read is lazy: `gh` is a network call. The
pull is neither — it writes, so it happens on a click and never as a side effect of a
filter change, and its guard is dropped as soon as it answers so the same repo can be
pulled again.

The PR cache holds only **merged/closed** statuses — the states that can never change
— in `~/.cache/hdash/pr-status.json`, falling back once to `hwt-overview`'s old path
so the rename does not cost a re-query of every merged PR. **Refresh** re-scans and
re-queries but keeps the cache; **Clear cache** wipes it so merged/closed PRs are
checked again too.

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
crates/hdash_app     — model/{task,project,item,launch,pull} and ui/{chrome,card,fetch}
```

## Known Limitations

- Launch actions assume alacritty + nushell with `hwt.nu`/`hcdev.nu` loaded.
- **Start task** and **Restore** run `hwt restore`, which needs the project's main
  checkout at `~/fun_repos/<project>`.
