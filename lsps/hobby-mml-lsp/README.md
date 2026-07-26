# hobby-mml-lsp

| Field        | Value                                              |
| ------------ | -------------------------------------------------- |
| Type         | lsp + CLI                                          |
| Binaries     | `hobby-mml-lsp`, `hobby-mml-parser`                |
| Owner        | @julianbieber                                      |
| Dependencies | tree-sitter-mml grammar (from the work tooling repo) |
| Status       | experimental                                       |

## Description

Fork of the work `mml-lsp` for hobby specs, now carrying its **own grammar**
([`grammar.md`](grammar.md), HMML v2). Language server (completion, hover,
goto-definition, references, document symbols, diagnostics) plus the
`hobby-mml-parser --lint` CLI the `hmml-*` skills shell out to.

## Why a fork rather than a shared binary

Originally: spec→source path resolution.

| | work | hobby |
|---|---|---|
| spec | `notes/{topic}/{task}/<unit>.hmml` | `notes/{project}/{topic}/{task}/<unit>.hmml` |
| checkout | `~/worktrees/{topic}/{task}/{repo}` | `~/fun_worktrees/{project}/{topic}/{task}` |
| fallback | `~/projects/<repo>` | `~/fun_repos/<repo>` |

The hobby layout has **no repo level** — a task is one repo, so the task
directory *is* the checkout.

Since **v2 the fork is a genuine dialect, not a path tweak.** Hobby projects are
overwhelmingly Bevy games, and MML v1's founding assumption — a unit is a
function with `in: T` / `out: T` — does not hold in an ECS, where data flows
through the World. v2 adds `kind: plugin` / `kind: system` and the sections
`## components`, `## resources`, `## events`, `## systems`, `## access`, and
makes the legal section set a function of `kind:`. The classic kinds are
untouched, so every v1 spec still parses.

That divergence ends any prospect of re-merging with the work binary, and makes
upstream `mml-parser` cherry-picks a manual job — a cost accepted deliberately,
since the Bevy dialect has no business in a work repo.

## Grammar

[`grammar.md`](grammar.md) in this directory is the source of truth, and the
`hmml-*` skills point at it. The work `01-language-spec/grammar.md` describes v1
and no longer governs anything here.

The always-parsing examples are the parser's own fixtures under
`mml-parser/tests/fixtures/examples/` — they are compiled into the test suite, so
docs and parser cannot drift:

| Fixture | Shows |
|---|---|
| `enemy-waves.hmml` | `kind: plugin` — world sections, systems, ordering, `marker`, `where:` |
| `spawn-enemies.hmml` | `kind: system` — `## access`, cross-spec imports, query exclusion |
| `get-map-tile.hmml` | `kind: endpoint` — `method`, param sources |
| `mesh-terrain-chunks.hmml` | `kind: workflow` — `Queue`/`Stream` |
| `shared-data-imports.hmml` / `cross-spec-refs.hmml` | `use` imports, `@ spec` refs |

## Helix routing

Two binaries on `PATH` would normally fight, since Helix maps a language to one
server. A second language entry routed by path glob solves it — in
`~/.config/helix/languages.toml`:

```toml
[language-server]
hobby-mml-lsp = { command = "hobby-mml-lsp" }

# BEFORE the plain `mml` entry — a glob file-type is more specific than the bare
# "mml" extension.
[[language]]
name = "mml-hobby"
scope = "source.hmml"
file-types = [{ glob = "**/hobby-mimisbrunnr/notes/**/*.hmml" }]
grammar = "mml"                       # reuse the existing tree-sitter grammar
language-servers = ["hobby-mml-lsp"]
```

Helix resolves highlight queries by **language name**, not grammar, so
`~/.config/helix/runtime/queries/mml-hobby/highlights.scm` exists and contains
just `; inherits: mml`. Without it hobby specs parse but render unhighlighted.

Verify with `hx --health mml-hobby` and `hx --health mml` — each should bind its
own server, and both should show a tree-sitter parser and highlight queries.

## Installation

```bash
just install-mml     # installs hobby-mml-lsp, hobby-mml-parser, hobby-mml-index
```

## Known Limitations

- **Highlighting depends on a grammar this repo does not own.** `grammar = "mml"`
  borrows the `tree-sitter-mml` build, whose checkout lives outside the hobby tree.
  Its path is no longer recorded here — `wire-helix` reads `TREE_SITTER_MML` from
  `~/.config/hobby-tooling/paths.env` and generates the `[[grammar]]` block into
  `~/.config/helix/languages.local.toml`. A machine without that checkout keeps the
  language server, diagnostics and goto-definition, and loses highlighting alone.
  Where the grammar *is* present it is a v1 one, so v2 sections (`## components`,
  `## systems`, …) parse and lint fine but may render plainly. Cosmetic either way —
  every diagnostic comes from `hobby-mml-parser`, not the grammar.
- Upstream `mml-parser` fixes must be cherry-picked by hand, and v2 has moved
  the two apart far enough that this is real work rather than a patch apply.
- The built-in Bevy name lists (`BUILTIN_COMPONENTS`, `BUILTIN_RESOURCES`,
  `BUILTIN_EVENTS` in `mml-parser/src/validate.rs`) are hand-maintained and
  cover the common engine types, not all of Bevy. An unlisted engine type reads
  as `undefined-access-item`; the fix is to add it to the list, not to declare
  it locally.
