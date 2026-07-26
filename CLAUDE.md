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
lsps/hobby-mml-lsp/                   # forked HMML language server + grammar.md
lsps/{claude,claude-skills,treesitter-class,path}-lsp/   # vendored from the work checkout
tools/{hobby-mml-index, hwt-overview}/
```

## HMML is a Bevy dialect here

`lsps/hobby-mml-lsp/grammar.md` is the **source of truth** for HMML (v2) and
what the `hmml-*` skills read. The work `01-language-spec/grammar.md` describes v1
and governs nothing in this repo.

Hobby specs are **`.hmml`**, never `.mml`. The extension is what keeps the two
dialects apart: Helix routes `.hmml` to `hobby-mml-lsp` by file type, and work
`.mml` to `mml-lsp`, with no path globs or block-ordering tricks involved.
`mml_parser::SPEC_EXT` is the single definition — every place that builds or
filters a spec filename goes through it, so the extension stays a one-line
change.

The **binaries keep their `hobby-mml-*` names** (`hobby-mml-lsp`,
`hobby-mml-parser`, `hobby-mml-index`), as do the crate directories. That is
deliberate, not an oversight: they never collided with the work `mml-lsp` /
`mml-parser` / `mml-index` on `PATH` in the first place, so renaming them would
buy nothing and cost a `cargo uninstall` dance plus stale `~/.cargo/bin`
entries. The language is HMML; the programs that read it are `hobby-mml-*`.

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

## The vendored language servers

`lsps/` holds five servers. `hobby-mml-lsp` is a fork with its own dialect (above);
the other four — `claude-lsp`, `claude-skills-lsp`, `treesitter-class-lsp`,
`path-lsp` — are **copies** of crates that also exist in the work tooling checkout,
vendored so a hobby-only machine can build them at all.

Three consequences worth knowing before touching them:

- **The binary names are unchanged** (`claude-lsp`, `claude-skills-lsp`,
  `treesitter-class-lsp`, `path_lsp`), unlike the deliberately-renamed `hobby-mml-*`.
  There is one Helix and one `languages.toml` shared by both accounts, so a
  `hobby-`-prefixed second name would not isolate anything — it would just register a
  second server that runs on every buffer alongside the first. One binary is correct.
- **Building from here replaces the work build** in `~/.cargo/bin`, and vice versa.
  The two copies will drift; this one is the copy that has the hobby fix.
- **The hobby fix is `CLAUDE_CONFIG_DIR`.** `claude-skills-lsp` used to anchor its
  user tier on `$HOME/.claude`, which in a hobby session offered the work skills and
  none of the 18 hobby ones. It now resolves `$CLAUDE_CONFIG_DIR` first, falling back
  to `$HOME/.claude`, so one binary serves both accounts — `hcdev` exports the
  variable before launching zellij, so the Helix pane inherits it. Keep that property
  if you re-sync from the work copy.

`treesitter-class-lsp` needs no hobby change: its `~/worktrees/TOPIC/TASK` widening
does not match `fun_worktrees`, and falling back to the client root is already the
right scope when the task directory *is* the repo.

Nothing in `lsps/` may name a work repo, worktree, or ticket — the copies were
scrubbed, and a re-sync has to be scrubbed again.

## Installing

`just bootstrap` is the fresh-machine entry point; `just doctor` diagnoses an
existing install and names the recipe that fixes each gap. Every recipe is
idempotent.

Anything more involved than a symlink lives in `scripts/` rather than inline in the
justfile: `preflight.sh` (dependency check), `wire-nushell.sh` (append the `use`
lines once), `helix-paths.sh` (find this machine's Helix bits), `wire-helix.sh`
(generate the Helix language config), `helix-servers.sh` (declared-vs-installed
language servers), `local-paths.sh` (the machine-local overrides file), `doctor.sh`.

## No machine-specific paths in this repo

This repo is checked out on more than one machine and they do not agree on where
anything outside `~/fun_repos` lives — the Helix checkout is `~/fun_repos/helix` on
one and `~/outside_repos/helix` on another. A hardcoded default is therefore a
guaranteed breakage somewhere, so **paths are discovered, never assumed**, and
`scripts/helix-paths.sh` is the single resolver every script and recipe uses.

The escape hatch is `~/.config/hobby-tooling/paths.env` (`just local-paths`),
outside the repo and never committed. It is also the *only* correct home for a path
under the work tree: `TREE_SITTER_MML` belongs there, not in any file here.

## Helix: one dev flow, three files

Helix merges `~/.config/helix/languages.toml` over the `languages.toml` compiled
into its binary, so:

```
<checkout>/languages.toml               machine-NEUTRAL, committed, builds the binary
~/.config/helix/languages.local.toml    machine-SPECIFIC, generated, never committed
~/.config/helix/languages.toml          = the two concatenated, an OUTPUT
```

Edit the first for anything shared; let `just wire-helix` regenerate the third.
Editing the installed file directly is the one thing that does not work.

What forced the split is `[[grammar]] name = "mml"`. Its `source.path` must be
absolute, differs per machine, and does not exist at all on a hobby-only one — and
`helix-term/build.rs` builds grammars with `STRICT = true`, so a single unresolvable
path fails the whole `hx` build on every other machine. Committing that line was
what previously required commenting it out per machine. Now it is generated.

That reasoning applied to `.hmml` too, until HMML got its own grammar —
`lsps/tree-sitter-hmml`, forked from the work `tree-sitter-mml`. Because it ships
in this repo, `wire-helix` derives its path from the repo root and declares it
unconditionally, so **every machine highlights `.hmml`**. `TREE_SITTER_MML` now
only affects the work `.mml` dialect.

Two invariants for that fork: the grammar name, the exported symbol in
`src/parser.c`, and `name:` in `grammar.js` must all say `hmml` (Helix `dlsym`s
`tree_sitter_<name>`, and a mismatch silently yields no parser); and
`src/parser.c` stays committed, so a machine needs only a C compiler — requiring
the tree-sitter CLI everywhere would reintroduce the portability problem. The
queries are a real copy, not `; inherits: mml`, which was a second hidden
dependency on the work grammar.

`just helix-build` rebuilds `hx` with `HELIX_DISABLE_AUTO_GRAMMAR_BUILD=1` (the
automatic path runs `--grammar fetch` first, which dies on any grammar whose remote
has gone away and takes the binary build with it), then builds grammars separately
where a failure is survivable.

## Paths

```
~/fun_repos/<project>                          main checkouts
~/fun_worktrees/<project>/<topic>/<task>       the worktree itself
~/fun_repos/hobby-mimisbrunnr/notes/
    <project>/<topic>/<task>                   task notes
~/.claude-hobby                                CLAUDE_CONFIG_DIR
~/.config/hobby-tooling/paths.env              machine-local overrides, uncommitted
```

The Helix checkout is deliberately absent from that list — it is discovered, and it
is on branch **`custom`** wherever it is.

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
