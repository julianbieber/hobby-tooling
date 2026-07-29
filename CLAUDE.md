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
  zellij/{config.kdl, layouts/hobby-dev.kdl}   # symlinked into ~/.config/zellij/
  claude-md/                          # symlinked into ~/.claude-hobby/ and ~/fun_worktrees/
bin/{chobby, hx-claude}               # symlinked into ~/.local/bin/
hooks/<name>/hook.sh                  # symlinked into ~/.claude-hobby/hooks/
skills/<name>/{skill.md, README.md}   # symlinked into ~/.claude-hobby/commands/
settings/hobby.settings.json          # symlinked to ~/.claude-hobby/settings.json
scripts/                              # install logic too involved for a justfile recipe
lsps/hobby-mml-lsp/                   # forked HMML language server + grammar.md
lsps/{claude,claude-skills,treesitter-class,path}-lsp/   # vendored from the work checkout
mcps/{just-mcp, lsp-mcp, tree-extract}/   # vendored from the work checkout
tools/{hobby-mml-index, hwt-overview, hgh-issues}/
```

## `topic:<name>` is how an issue names its worktree

`hgh-issues` lists open GitHub issues and turns one into a dev session with a
click. The whole thing rests on a label. A worktree lives at
`~/fun_worktrees/PROJECT/TOPIC/TASK`, and an issue already implies two of those
three — the **project** is the repo it was filed in, the **task** is
`gh-<number>-<slug>` from its number and title. **Topic is the one component
nothing else supplies**, so the issue carries it as `topic:features`.

Prefixed, with no fallback to bare labels. Every hobby repo already has `bug`,
`enhancement`, `good first issue` from GitHub's defaults, and `wusel` labels its
issues `world_gen` / `world_sim` / `persistence` — all of which read like topics
and none of which are. Requiring the prefix keeps the tag an explicit statement
rather than a guess, and lets an issue name a topic whose directory does not exist
yet, which is the normal case: a topic dir only appears with the first worktree
under it. The value must be directory-shaped (`[A-Za-z0-9._-]+`, no leading dot);
anything else is treated as untagged rather than escaped, because it becomes both
a path component and a shell argument.

An untagged issue is still listed — it is the one that most needs attention — but
it has no launch button, only a line naming the missing label. `/gh-issue-create`
writes the label (creating it in the repo when absent), so issues opened through
the skill arrive launchable.

Existing worktrees are matched to issues **by number, not by task name**:
`gh-42-*`, `gh42-*`, `issue-42_*` and bare `gh-42` all belong to issue 42. Task
names predating the convention are spelled differently (`issue-9_dotfile_repo_retire`)
and a slug changes whenever the title is edited, so the derived name is what a
*new* task gets, never what a lookup relies on. When such a worktree exists, its
path — not the label — supplies the topic for `hcdev wt`, since the path is what
`hcdev` has to find.

## The two dashboards share their crates

`tools/hwt-overview` starts from the worktrees that exist; `tools/hgh-issues`
starts from the issues that have none yet. Separate workspaces, separate windows,
but `hgh-issues` **path-depends** on `../hwt-overview/crates/{feathers_kit,hwt_core}`
instead of copying them — the same move `mcps/tree-extract` makes into `lsps/`.

The one `feathers_kit` copy in this repo exists so the *work* dashboard cannot
regress. A second hobby copy would protect nothing and drift, and sharing the kit
is also what keeps the two windows looking like one tool. Two consequences worth
knowing: `cargo fmt`/`clippy` run from `tools/hgh-issues` reach into
`tools/hwt-overview`, and a change to `hwt_core` needs both workspaces' tests
(`just test` runs them).

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

`## data` is the one section **both** dialects take, and v2.1 is what made that
true. A Bevy feature is full of types that are not components, resources, or
events — a field type, an enum a component is built from — and there was nowhere
to put them: a world section would have claimed a role the type does not have,
and omitting them made every field naming one an `undefined-type` error. So a
Bevy kind may declare `## data`, with `in:`/`out:` and `marker` still illegal
there, and a `## data` name in an access list an `access-role-mismatch`. The
section is a type namespace in this dialect, not an interface — role is what
makes an access list checkable, and that is exactly what `## data` withholds.

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

## The vendored MCP servers

`mcps/` holds copies of the work tooling repo's general-purpose MCP servers —
`just-mcp`, `lsp-mcp`, `tree-extract` — vendored so `just install-mcps` builds on a
hobby-only machine instead of skipping all three.

**They are `hobby-`-prefixed, and this is where they diverge from `lsps/`.** The two
accounts run separate builds that coexist in `~/.cargo/bin`:

| | chobby (`~/.claude-hobby`) | claude (`~/.claude`) |
|---|---|---|
| just | `hobby-just-mcp` | `just_mcp` |
| lsp | `hobby-lsp-mcp` | `lsp-mcp` |
| tree-extract | `hobby-tree-extract-mcp` | `julian-tree-extract-mcp` |

The argument for keeping upstream names in `lsps/` does not carry over: there is one
Helix and one `languages.toml`, so a second LSP name would run a second server on
every buffer — but each account has its **own MCP registry**, so a second name
isolates instead of duplicating. Nothing is shared to collide over.

The prefix goes all the way down — package, lib and binary — and the **package** name
is the load-bearing part. Cargo tracks installs by package name in
`~/.cargo/.crates2.json`, so a hobby build sharing the work package name would evict
the work binaries on every `cargo install`, which is exactly what this is meant to
prevent. Renaming only the binary would not be enough.

Consequences: the two copies drift independently (neither overwrites the other), a
re-sync has to be re-scrubbed *and* re-prefixed, and `hobby-tree-extract` writes its
own name into the `generator:` field of `.project-description.md` and the
`.git/info/exclude` comment. That last one is safe because the exclude dedup keys on
the path, not the comment.

Only the *general-purpose* three are here, and the omissions are deliberate rather
than pending work:

- `db-mcp` and `jira-mcp` speak to work systems. Hobby projects have no databases,
  and issue tracking is GitHub via `gh`.
- `experts-mcp` assumes a repo level *below* the task, which this layout does not
  have — the same reason `CLAUDE.md` gives for there being no experts at all.
- `constellation-mcp` is not mine and is work-specific besides.

`tree-extract` is the one crate with a path dependency: it reuses the Helix language
registry and grammar cache from `treesitter-class-lsp`, repointed at `lsps/` here.
Vendoring one without the other does not build.

The *server* names stay unprefixed (`just-mcp`, `lsp-mcp`, `tree-extract`) even
though the binaries do not, so a skill or prompt calling `mcp__tree-extract__*` works
on either account. Only the command behind the name differs.

Registration is `-s user` scope, not the default `local` — `local` binds a server to
the single directory it was added from, so every hobby project outside that one
directory would see no MCPs. `install-mcps` removes before adding, so re-running it
repoints an entry that still names a work binary instead of skipping it as already
registered.

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

The rule bites hardest in formats with no variable expansion. `dotfiles/zellij/config.kdl`
binds `Alt a` to the tool-audit log, and KDL cannot expand anything — so the binding
runs `sh -c` and lets the shell do it:
`tail -f ${CLAUDE_CONFIG_DIR:-$HOME/.claude}/tool-audit.log`. Since `hcdev` exports
`CLAUDE_CONFIG_DIR` before launching zellij, a hobby pane follows the hobby log and
every other session follows the work one — one committed line, correct on both
accounts. Reach for a shell before reaching for a hardcoded path.

## Dotfiles are installed, never merely present

Every symlink in `install-*` goes through `scripts/link-dotfile.sh`, not `ln -sf`.
The difference is the one case `ln -sf` cannot handle: it replaces a stale symlink
but **refuses a regular file**, and a regular file is what the retired home dotfiles
repo (`github.com:julianbieber/config.git`, see #9) left at these paths. The helper
moves it to `<target>.pre-hobby-tooling.<timestamp>` and prints where it went —
nothing is overwritten in place, and no backup is silent.

`doctor` tests these with `-h` before `-e` for the same reason. `-e` is true for a
file some *other* repo installed, which is exactly how `~/.config/zellij/config.kdl`
passed as this repo's for as long as it did: the layout was linked, the config was
merely there, and `default_shell "nu"` — what makes `hwt`/`hcdev` reachable in a pane
— came from somewhere `just bootstrap` had never heard of. A real file at a path this
repo owns is a gap, not a pass.

`link-dotfile.sh` is also where per-machine variants will land (#9): prefer
`<source>.$HOBBY_MACHINE` over `<source>` there, and every recipe inherits it.

## The conversation under ctrl-g

**ctrl-g** in Claude Code opens `$EDITOR` on the prompt buffer. Here that shows
the conversation so far beside it, so the context is readable while writing the
next prompt. Two pieces, and they only work as a pair:

- `hooks/claude-conversation/hook.sh` renders the transcript to
  `/tmp/claude-conversation-<key>.md` on every `PostToolUse` and `Stop`
- `bin/hx-claude` is `$EDITOR`, and vsplits that file next to the prompt buffer
  ctrl-g hands it

`<key>` is the sanitized `$ZELLIJ_SESSION_NAME`, so ctrl-g in one pane opens that
pane's conversation and not a neighbour's. **The key derivation is duplicated in
both scripts and must stay identical** — diverge and each silently uses a
different file.

This is not about the zellij layout. The `editor` pane stays a plain shell; the
conversation view is a ctrl-g thing, transient, and gone when you close it.

`settings.json` calls the hook **by absolute path**
(`$HOME/.claude-hobby/hooks/claude-conversation.sh`), like every other hook, not
by a bare `claude-conversation-dump` on `PATH`. That bare name was the bug: it
resolved only on a machine that also had the work checkout, and there only
through a symlink into a work *worktree* that any `wt rm` deletes. On a
hobby-only machine it failed after every single tool call, silently, because
`PostToolUse` registers it with an empty matcher. Only `hx-claude` needs
`~/.local/bin`, because `$EDITOR` names it.

`hx-claude` **keeps its upstream name**, for the reason `lsps/` keeps theirs and
`mcps/` does not: `$EDITOR` is one variable shared by both accounts, so a
`hobby-`-prefixed second name would isolate nothing — it would just be a name
nothing points at. Installing from here replaces any link from the work checkout,
which is harmless because the script is account-agnostic. The dump path is safe
to share for the same reason: `hcdev` names sessions
`hcdev-<project>-<topic>-<task>` and the work `cdev` names its own `cdev-…`, so
the two accounts already have disjoint keys inside
`/tmp/claude-conversation-*.md`.

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
