# Hobby Tooling

## What This Repo Is

Personal Claude Code tooling for hobby projects, running under a **separate Claude
account** from work. The private-account counterpart to
`team-constellation-tooling`. Adapted from that repo's `julian-*` tools, but
restructured for single-repo tasks.

## The Structural Difference That Explains Everything

At work, `~/worktrees/TOPIC/TASK/` is a **container** holding several repo
worktrees, because a work task often spans Patagona-API + request-worker + infra.

Here, `~/fun_worktrees/PROJECT/TOPIC/TASK/` **is** the worktree. Hobby tasks are
single-repo, so the project moves to the front of the path and the repo level
disappears.

Consequences, all of which make this tooling smaller than its work counterpart:

- No `wt add` — a task can't gain a second repo
- `task.json` is just `{ done, blocked }`; no `repos[]`, since the project is in the path
- `hwt go <task>` takes no project argument
- A task directory is itself a git repo, so Claude gets branch + git status for free
- No experts (one repo means one expert, and a third path level strains the
  108-byte Unix socket limit)

## Layout

```
dotfiles/
  nushell/{hwt.nu, hcdev.nu}          # symlinked into ~/.config/nushell/
  zellij/layouts/hobby-dev.kdl        # symlinked into ~/.config/zellij/layouts/
  claude-md/                          # symlinked into ~/.claude-hobby/ and ~/fun_worktrees/
hooks/<name>/hook.sh                  # symlinked into ~/.claude-hobby/hooks/
skills/<name>/{skill.md, README.md}   # symlinked into ~/.claude-hobby/commands/
settings/hobby.settings.json          # symlinked to ~/.claude-hobby/settings.json
scripts/                              # install logic too involved for a justfile recipe
lsps/hobby-mml-lsp/                   # forked MML language server + grammar.md
tools/{hobby-mml-index, hwt-overview}/
```

## MML is a Bevy dialect here

`lsps/hobby-mml-lsp/grammar.md` is the **source of truth** for hobby MML (v2) and
what the `mml-*` skills read. The work `01-language-spec/grammar.md` describes v1
and governs nothing in this repo.

The fork exists because hobby projects are Bevy games and MML v1 assumes a unit
is a function (`in: T` / `out: T`). An ECS has no such contract — data flows
through the World — so v2 adds `kind: plugin` / `kind: system` and the sections
`## components`, `## resources`, `## events`, `## systems`, `## access`, with the
legal section set a function of `kind:`. A system's interface is its **access
block** (`schedule:`, `query:`, `reads:`, `writes:`, `spawns:`, `emits:`, …), and
every name in it is role-checked against the declared components / resources /
events. That check is what replaces the `in:`/`out:` type check and what gives
the LSP goto-definition, "who writes this component?", and the index's schedule
map.

`endpoint` / `workflow` / `job` keep the v1 grammar unchanged, so the handful of
non-Bevy hobby repos (`mosaik_map`, `securitd`, `glsllsp`) still have a usable
dialect. The two cannot be mixed in one file.

## Installing

`just bootstrap` is the fresh-machine entry point; `just doctor` diagnoses an
existing install and names the recipe that fixes each gap. Every recipe is
idempotent.

Anything more involved than a symlink lives in `scripts/` rather than inline in the
justfile: `preflight.sh` (dependency check), `wire-nushell.sh` (append the `use`
lines once), `wire-helix.sh` (register the forked MML server), `doctor.sh`.

**Helix config is edited in the Helix checkout, not in `~/.config/helix`.**
`languages.toml` is copied from the repo into place, so editing the installed copy
loses the change on the next refresh. `wire-helix.sh` handles this; keep any future
Helix change going through it.

## Paths

```
~/fun_repos/<project>                          main checkouts
~/fun_worktrees/<project>/<topic>/<task>       the worktree itself
~/fun_repos/hobby-mimisbrunnr/notes/
    <project>/<topic>/<task>                   task notes
~/.claude-hobby                                CLAUDE_CONFIG_DIR
```

## Account Isolation

`CLAUDE_CONFIG_DIR=~/.claude-hobby` gives a fully separate login, MCP set,
settings, skills, and history. Two ways in:

- `chobby` — wrapper on `PATH` that sets the var and execs `claude`
- `hcdev` — exports the var before launching zellij, so every pane inherits it

A bare `claude` always means the **work** account. If you're in a hobby worktree
and unsure, check `echo $CLAUDE_CONFIG_DIR`.

## Conventions

- Every tool is a folder with a `README.md`, never a loose file.
- `skill.md` is the canonical skill filename.
- No `julian-` prefix — this repo has one author, so provenance prefixes are noise.
- Hobby projects use **GitHub issues**, not Jira. Anything reaching for a tracker
  goes through `gh`.
- No secrets, no work identifiers, no company hostnames. This repo is private but
  it is not a work repo.

## Installing

`just` lists every recipe. `just install` runs them all. Each `install-*` recipe is
independently re-runnable.
