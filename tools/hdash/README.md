# hdash

| Field        | Value                                                       |
| ------------ | ----------------------------------------------------------- |
| Type         | terminal app (ratatui)                                      |
| Binary       | `hdash`                                                     |
| Owner        | @julianbieber                                               |
| Dependencies | `hwt`/`hcdev` on PATH, alacritty, nushell, `gh` (authenticated, for PR status), `xdotool` + `zellij` (for Focus/Attach) |
| Status       | experimental                                                |

## Description

A terminal dashboard of every hobby work item: **one card per task in the notes repo**,
wherever it sits in the lifecycle. A task nobody has started, the worktree it became,
the PRs that came out of it, and the day it was marked done are all the same card.

The card's headline is the `title:` in the task's `issue.md` frontmatter (what
`/issue-create` writes), falling back to the task name.

`hdash` runs in the terminal it is started from. Started without one (from a launcher), it
re-runs itself in a fullscreen Alacritty window titled and classed `hdash`. Warnings go to
`$XDG_CACHE_HOME/hdash/hdash.log` (default `~/.cache/…`), never onto the screen.

### Mouse and keys

Every button and chip works by click, as do the project rows, the PR and subtask lines and a
card's body, which selects the card. The wheel scrolls the sidebar or the list under the
pointer; the control under the pointer is drawn reversed.

| Key | Does |
| --- | --- |
| `q` / `Ctrl-C` | quit (`Ctrl-C` also while typing in the filter field) |
| `↑` `ü` / `↓` `ä` | select the previous / next card; `PgUp` / `PgDn` by a screen |
| `←` `ö` / `→` `#` | move along the selected card's buttons, then its PR toggle, PR lines and subtasks |
| `Enter` / `Space` | press the focused control (a newly selected card starts on its primary button) |
| `/` | focus the project filter field |

While the field is focused every printable key, `q ö ä ü #` included, is typed into it;
`Backspace` deletes, pasted text lands in it, `Enter` scopes the list to the top-ranked
project, `Esc` empties it and lets go.

## The card is a lifecycle, and the primary button is the next step

| Stage | Condition | Primary button |
| --- | --- | --- |
| orphan | a worktree with no `task.json` in the notes repo | **Adopt** — writes the `task.json` |
| not started | `task.json`, no worktree, and no branch of that name in the main checkout | **Start task** — `hwt restore` then `hcdev wt` |
| active | a live worktree | **Focus** / **Attach** / **Open hcdev** |
| removed | worktree gone, branch and notes kept | **Restore** — `hwt restore` then `hcdev wt` |
| done | `done` in `task.json` | **Open hcdev** (and **Reopen**) |
| coordinator | a task under the reserved `coordinator` topic | **Focus** / **Attach** / **Open coordinator** — `hcdev coordinator` in the main checkout; the card shows the `tasks.json` counts |

Plus, as they apply: **Done**/**Reopen**, **Backlog**/**Unbacklog**, **Terminal**,
**Notes**, **Hide**.

**Focus** when an Alacritty window (class `Alacritty` or `hcdev-auto`) already shows the
task's zellij session: the click activates it, so leftwm switches to its tag. **Attach** when
the session runs without a window, **Open …** when it does not run or the probe cannot (no
`xdotool`/`zellij`); both open a terminal running `hcdev`, which attaches or starts it. The
label comes from a 2-second probe; the click looks the window up again, and a second launch of
the same task within 5 seconds is ignored. Task, project and topic reach the launched nu as
`HDASH_TASK`/`HDASH_PROJECT`/`HDASH_TOPIC`, never inside the script text.

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
0  open task whose Claude session is idle   5  task active
1  orphan worktree                          6  task blocked
2  task, block lifted ("ready")             7  task not started
3  task active, PR awaiting review          8  task removed-but-remembered, or a coordinator
4  task whose Claude session is busy        9  task done
```

**Show done** is off by default: done tasks outnumber open ones ~25:1 and would swamp the
list. **Show backlog** switches the list: off, backlogged open work is hidden; on, only
backlogged items that are not done are shown. The **Backlog** button writes `backlog` into
`task.json`, after `blocked`, in the same key order `hwt` writes.

No toggle can hide an orphan; a filter chip that is on can.

A **fuzzy filter** field above the project list narrows the sidebar as you type, matching a
project's name or the name or title of any task in it, best match first, smart-case. The
selected project always stays listed, and a project whose count is 0 is left out unless the
field has text. The query is transient.

A **chip bar** under the header narrows the list: **State** (ready, blocked, not blocked, not
started, removed, orphan), **PR** (awaiting review), **Repos** (dirty), **Phase** (every
`/task:build` / `/task:auto` phase some task's `build-task.json` records, plus no phase),
**Active** (today, 7 days, older) and **Session** (busy, idle, no session). Chips in one group
are alternatives; groups combine with AND. Each chip shows how many items the list would have
with it on; a chip that would match nothing is hidden unless it is on, and **clear** turns
them all off. Chips live in memory.

## Session state

A card whose task has a live Claude session shows **`idle 12m`** in amber while it waits on
you, or **`busy 3m`** while it works; the header counts the open tasks waiting on you. The
state comes from the [`session-state`](../../hooks/session-state/README.md) hook, re-read
every second. Every Claude process of the hobby account counts — a plain hcdev pane, both
`/task:auto` stages, a coordinator's pane — matched to a task by its zellij session name
(from `hcdev.nu`'s `hcdev-session-names`), else by the worktree its directory lies in; the
newest live record per task decides. A session busy for 30 seconds whose `claude` process used
under 60 CPU ticks over that window reads idle since its last hook event.

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

Every read runs in the background, on a small job pool (git, `gh`, pull) or its own thread
(the fingerprint and rescan, the window probe):

| | key | scope | refresh |
| --- | --- | --- | --- |
| git status | (task, repo path) | every present task | every 10 s for open tasks, and for repos a rescan newly found |
| PR status (`gh pr view`) | (task, PR URL) | items passing the project, done, backlog and hide filters, whatever the chips | open, draft and unknown every 5 min |
| pull (`git pull --ff-only`) | project | clicked projects only | on click |
| model | — | the notes and worktree trees | rescan when their fingerprint moves (every 2 s) |

The fingerprint is the mtimes of the project, topic and task dirs of both trees and of each
notes task's `task.json`, `prs.json`, `subtasks.json`, `build-task.json`, `issue.md` and
`tasks.json`. A rescan is merged in: git status carries over by repo path and PR state by URL,
so a background update never resets a card to loading, and nothing is redrawn when nothing
changed. **Refresh** un-hides the cards and rescans in the background, keeping the cards
until it lands.

The PR cache holds only **merged/closed** statuses — the states that can never change
— in `~/.cache/hdash/pr-status.json`, falling back once to `hwt-overview`'s old path
so the rename does not cost a re-query of every merged PR. **Refresh** re-queries but keeps
the cache; **Clear cache** wipes it so merged/closed PRs are checked again too.

Every string read from a file, `git` or `gh` loses its control and invisible characters
before it is drawn.

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
just run            # dev profile, in this terminal
just run-release
just test
just shot           # screenshot the app in an Alacritty window (needs xdotool + imagemagick)
```

## Layout

```
crates/hwt_core      — paths, detached spawning, the job pool, interval timer, file log,
                       fuzzy ranking, text cleaning, date/time helpers
crates/hdash_app     — app (terminal shell), view/{mod,draw,input} (the dashboard),
                       model/{task,project,item,launch,pull}, refresh, session, focus,
                       facets, widgets, hits
```

## Known Limitations

- Below 76 columns or 12 lines only a "terminal too small" line is drawn.
- The filter field takes typing, `Backspace` and paste only — no cursor movement.
- A terminal does not report the pointer leaving the window, so the last hovered control
  stays highlighted until the pointer moves onto something else.
- Session names come from `~/.config/nushell/hcdev.nu`; when it is missing or predates
  `hcdev-session-names`, cards fall back to the unclamped `hcdev-P-T-TASK`, which is the
  same name for every task short enough not to need clamping.

- Launch actions assume alacritty + nushell with `hwt.nu`/`hcdev.nu` loaded.
- **Start task** and **Restore** run `hwt restore`, which needs the project's main
  checkout at `~/fun_repos/<project>`.
