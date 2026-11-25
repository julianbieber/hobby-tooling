# ~/fun_worktrees — Hobby Task Worktrees

## Structure

```
~/fun_worktrees/<project>/<topic>/<task>/
```

Three levels. The **task directory is the git worktree itself** — hobby tasks are
single-repo, so there is no repo level underneath. The branch is named after the
task, and the main checkout it derives from is `~/fun_repos/<project>`.

```
~/fun_worktrees/
  bevy_survivor/
    features/
      enemy-waves/        <- a git worktree, branch `enemy-waves`
    bugs/
      spawn-crash/
  wusel/
    perf/
      chunk-loading/
```

## Where Things Live

| | |
|---|---|
| Main checkout | `~/fun_repos/<project>` |
| Notes for this task | `~/fun_repos/hobby-mimisbrunnr/notes/<project>/<topic>/<task>/` |
| Journal entries | same dir, `YYYY-MM-DD.md` |
| Done/blocked/backlog state | same dir, `task.json` (tracked — `{ done, blocked }`, plus `backlog` once hdash has set one) |
| PRs opened | same dir, `prs.json` (written by the track-pr hook) |
| HMML review delta | same dir, `delta-<branch>.hmml` (derived from the diff by `/hmml-extract`) |
| Business context | same dir, `business-context.md` (my own words, recorded verbatim) |
| Research aspects | same dir, `aspects/<name>/` (`aspect.json`, `brief.md`, `findings.md`); worktree `~/fun_aspects/<project>/<topic>/<task>/<name>` |

Derive topic and task from the path: they are the **second and third** components
under `~/fun_worktrees`, with the project first.

**`task.json` is what makes a task exist**, not the worktree directory. `hwt` and `hdash`
both enumerate the notes tree; `~/fun_worktrees` only answers whether a task is currently
checked out. A worktree with no `task.json` behind it is an *orphan* — `hwt orphans` lists
them, and `hdash` offers **Adopt**.

The notes dir outlives the worktree: `hwt rm` is routine, and the notes are committed
and shared across machines while the worktree is scratch on one disk. A record that
vanished with the checkout would lose the task every time.

## Commands

```
hwt new <task> -p <project> -t <topic>   create a task worktree + notes dir
hwt go <task>                            cd to it (fuzzy, searches all projects)
hwt tree                                 project -> topic -> task overview
hwt done | block | unblock               task state
hwt block --pr <url> [--task <t> -p <p>] wait on a PR; merging it lifts the block
hwt orphans                              worktrees with no task.json
hwt rm <task>                            remove the worktree and its aspects', keep the notes
hwt sweep [-n]                           zclean, then hwt rm every done task that is clean, pushed and not attached
hwt restore <task>                       re-create a removed worktree and its started aspects'
hcdev wt <task>                          zellij session: Helix + Claude, plus its started aspects' tabs
hcdev close <task> [-p] [-t]             kill its session, stop its processes, close its window
hcdev notes                              cd to this task's notes dir
hwt aspect new <name> [-k code|research] add an aspect of this task: notes only, started now or later
hwt aspect start <name>                  cut its worktree from the current base, open its tab
hwt aspect list | close <name>           a task's aspects / remove one's worktree, keep notes
hcdev tab <name>                         open or focus a started aspect's tab in the task's session
```

## Account

Sessions started via `hcdev` or `chobby` run on the **private** account
(`CLAUDE_CONFIG_DIR=~/.claude-hobby`). A bare `claude` here would use the **work**
account — check `echo $CLAUDE_CONFIG_DIR` if unsure.
