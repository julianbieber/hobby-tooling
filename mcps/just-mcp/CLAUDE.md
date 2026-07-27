# CLAUDE.md

Guidance for Claude Code when working in this repository
(`mcps/julian-just-mcp`).

## Build and Test Commands

```bash
just build      # cargo build
just install    # cargo install --path . -> ~/.cargo/bin/just_mcp
just test       # cargo test (integration tests spin up an in-memory client)
just run        # run the MCP server (stdio)
just check      # cargo check
just fmt        # cargo fmt
just lint       # cargo clippy
```

## Overview

An MCP server that discovers and runs [`just`](https://github.com/casey/just)
recipes across a tree of projects. It shells out to the `just` binary for both
introspection (`just --dump --dump-format json`) and execution, so it always
agrees with the installed `just` about recipe semantics and parameter kinds.

## Module Structure

```
src/
├── main.rs      # entry point (stdio transport)
├── lib.rs       # module declarations + re-exports
├── config.rs    # Config struct + load from ~/.config/just_mcp.toml
├── models.rs    # DumpFile/DumpRecipe (raw just JSON) + tool response DTOs
├── discover.rs  # tree walk for justfiles + path/root resolution
├── dump.rs      # run `just --dump`, parse, flatten recipes across submodules
├── run.rs       # argv assembly, mise wrapping, process exec with timeout
└── server.rs    # JustMcpServer — the four #[tool] definitions
tests/
└── integration_test.rs  # in-memory client/server over fixture justfiles
```

## Tools

- **`just_projects`** (`ProjectsParams { root? }`) — walks `root` (default: the
  configured root or the server's cwd) and returns a `ProjectEntry` per
  justfile: path, working dir, recipe count (incl. submodules), module count,
  default recipe. A justfile that fails to parse is still listed, with `error`
  populated.
- **`just_recipes`** (`RecipesParams { justfile, include_private? }`) — dumps
  one justfile and returns a `RecipeInfo` per recipe: `namepath` (what
  `just_run` wants), doc, group, private flag, parameters and dependencies, plus
  aliases. `justfile` may be a file path or a directory containing one. Private
  recipes (`[private]` or leading `_`) are hidden unless `include_private`.
- **`just_show`** (`ShowParams { justfile, recipe }`) — `just --show <recipe>`.
- **`just_run`**
  (`RunParams { justfile, recipe, args?, positional?, dry_run?, timeout_secs? }`)
  — runs a recipe, capturing stdout/stderr/exit/duration.

## Key Mechanics

### `just --dump --dump-format json`

The introspection contract. Relevant shape (fields not consumed are ignored):

- Top level: `recipes` (map), `aliases` (map of `{name, target}`), `modules`
  (map, each a nested dump), `first` (default recipe name).
- Recipe: `name`, `namepath` (`parent::child` for submodules), `doc`,
  `parameters[]`, `dependencies[]`, `private` (bool), `attributes[]` (group is
  `{"group": "..."}`).
- Parameter: `name`, `default` (`null` ⇒ required), `kind` (`singular` | `star`
  (0+) | `plus` (1+)), `help`.

`models.rs` deserializes this with every field `#[serde(default)]` so parsing
survives `just` version drift.

### Argument assembly (`run::positional_from_map`)

`just` takes arguments **positionally**. Given a `name → value` map, parameters
are consumed in declared order:

- A variadic parameter (`star`/`plus`) accepts a JSON array (each element
  becomes one argument) or a single scalar.
- Once an omitted optional/default parameter is reached, consumption stops;
  setting a later parameter after that is an error (positional limitation).
- A missing required parameter is an error.

Alternatively pass `positional` (an ordered string list) to bypass name
resolution entirely — needed when the recipe can't be found in the dump (e.g.
invoking an alias). `args` and `positional` are mutually exclusive.

### Discovery (`discover::discover`)

`ignore::WalkBuilder` with `hidden(false)` (to see `.justfile`), honouring
in-tree `.gitignore` only (`parents(false)`, `git_global(false)`,
`git_exclude(false)`) — a broad `*` in `~/.gitignore` must not hide the tree.
Directory names in `config.ignore` (`.git`, `target`, `node_modules`, …) are
pruned via `filter_entry`. Recognises `justfile` / `.justfile`
(case-insensitive).

### mise wrapping (`run::find_mise_dir`)

When `use_mise` is set (default) and a mise marker (`mise.toml`, `.mise.toml`,
`mise.local.toml`, `.mise.local.toml`, `.tool-versions`, or
`.config/mise/config.toml`) is found at or above the justfile's directory, the
command becomes `mise exec --cd <marker_dir> -- just …`. This activates project
tool versions (Java/Node/Python) that a plain shell-out would miss — mise
otherwise only activates on an interactive shell `cd`.

### Execution (`run::run_recipe`)

Spawns with the justfile's directory as cwd, stdin closed, stdout/stderr piped
and drained concurrently (so large output can't deadlock the child). A
`tokio::time::timeout` around `child.wait()` kills the process on timeout and
sets `timed_out`. `dry_run` adds `--dry-run`.

## How to Add / Change a Tool

1. Add/adjust a params struct in `src/params.rs`
   (`#[derive(Deserialize, JsonSchema)]`, doc comments become the MCP schema
   descriptions).
2. Add response fields in `src/models.rs` if needed.
3. Implement logic in `discover.rs` / `dump.rs` / `run.rs`.
4. Register the `#[tool]` in `src/server.rs` inside the `#[tool_router]` block;
   delegate to a private `async fn` returning `Result<T, String>` and wrap with
   `respond(...)`.
5. Update this file, the README, and the integration tests (fixture justfiles
   are written to a temp dir per test).
6. `cargo build && cargo install --path .`, then restart the MCP server.
