# julian-tree-extract

| Field        | Value                                                                                                                                     |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Type         | mcp + cli (Rust)                                                                                                                          |
| Owner        | @julianbieber                                                                                                                             |
| Dependencies | helix runtime (grammars + queries) at `$HELIX_RUNTIME` or `~/outside_repos/helix/runtime`, path-dep on `lsps/julian-treesitter-class-lsp` |
| Status       | v2 (symbol map + code graph)                                                                                                              |

## Description

Walks a repository with tree-sitter and produces two artifacts:

1. **`<repo>/.project-description.md`** — a markdown symbol map of classes,
   structs, traits, interfaces, enums, free functions, and methods, with
   file:line ranges and best-effort docstrings.
2. **`<repo>/.project-graph.json`** — a queryable **code graph** of nodes
   (symbols and files) and typed edges (`calls`, `imports`, `implements`,
   `contains`) with cross-file name resolution. Queried over MCP for call
   graphs, impact analysis, path finding, and module/cycle/hotspot detection.

Both are useful as a fast "what's in this repo / what calls what" overview —
cheaper for Claude to read than re-deriving the structure by grepping. Both files
are added to
`<repo>/.git/info/exclude` so they stay per-developer artifacts.

Two binaries:

- **`tree-extract`** — CLI; `tree-extract <repo>` walks the repo, writes the
  markdown map and the graph JSON.
- **`julian-tree-extract-mcp`** — MCP stdio server (tools listed below).

Reuses the `grammars` and `languages` modules from
`lsps/julian-treesitter-class-lsp` so the helix language registry / grammar
cache lives in one place.

## Code graph

### How edges are built

Definitions come from helix's `tags.scm` queries. References (calls,
instantiation, inheritance) and imports come from those same queries *plus*
per-language supplementary queries in `src/builtin_queries/` (helix tags don't
capture imports anywhere, nor calls/inheritance for Rust/Java/Scala/TypeScript).
Edges:

- **`contains`** — file → top-level symbol, class → method (range nesting).
- **`calls`** — caller symbol → callee, resolved by name; `new X()` counts as a
  call to `X`. Attributed to the innermost enclosing definition of the call
  site.
- **`implements`** — subclass → supertype from `extends`/`implements`/`with`.
- **`imports`** — file → in-repo file, resolved best-effort by path suffix.

Node ids are `relpath:Lstart` for symbols and bare `relpath` for files.

### Resolution is name-based (deterministic AST only)

There is no scope/type/import-aware binding (the same limitation graphify has at
the AST level):

- A call `foo()` resolves to *every* definition named `foo`. If exactly one
  candidate is in the calling file it wins unambiguously; otherwise every
  candidate gets an edge flagged `ambiguous: true`.
- Calls to external/stdlib symbols produce no edge (counted in
  `stats.unresolved_calls`).
- `obj.method()` resolves on the bare method name only (no receiver type).
- Rust trait `impl` blocks are not modeled as inheritance (Rust has none).
- Package-qualified Java/Scala imports (package ≠ path) usually fall through to
  "external" (counted in `stats.external_imports`); Python dotted paths and
  relative TS/JS imports resolve well.

Nodes and edges are sorted, so the JSON is stable across runs (clean diffs).

### MCP tools

| Tool                          | Purpose                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------- |
| `refresh_project_description` | (Re)build both the markdown map and the graph.                                        |
| `build_graph`                 | (Re)build only `.project-graph.json`; returns node/edge/resolution stats.             |
| `search_symbols`              | Fuzzy-search symbol names → matching nodes (id, kind, file:line).                     |
| `get_symbol`                  | Details for a node (by id or exact name) + in/out edge counts.                        |
| `get_neighbors`               | 1-hop neighbors, filterable by direction (out/in/both) and edge kind.                 |
| `call_path`                   | Shortest path between two symbols over chosen edge kinds (BFS, depth-capped).         |
| `impact_of`                   | What depends on X: transitive reverse-reachability over calls+imports, file-grouped.  |
| `list_modules`                | Directory modules (coupling counts) + fan-in/out hotspots + circular-dependency SCCs. |

All list-returning tools apply default + hard-cap limits and return a
`{ok, summary, total, returned, truncated, results}` envelope, so results stay
token-bounded. Queries lazily build the graph on first use if it's missing.

## Installation

```bash
cargo install --path ~/fun_repos/hobby-tooling/mcps/tree-extract
```

Installs `tree-extract` and `julian-tree-extract-mcp` into `~/.cargo/bin/`.

### Wire into Claude Code

```json
{
  "mcpServers": {
    "tree-extract": { "command": "julian-tree-extract-mcp" }
  }
}
```

## Usage

```bash
# Markdown map + code graph for the current repo
tree-extract .

# Custom paths + larger size threshold
tree-extract /path/to/repo --output /tmp/desc.md --graph-output /tmp/graph.json --size-threshold-kib 200

# Markdown only (skip the graph)
tree-extract . --no-graph

# Skip the markdown rebuild if HEAD hasn't changed
tree-extract . --skip-if-unchanged

# Don't add outputs to .git/info/exclude
tree-extract . --no-gitignore
```

From a Claude session:

```
refresh_project_description(repo_path="/path/to/repo")
search_symbols(repo_path="/path/to/repo", query="parse")
get_neighbors(repo_path="/path/to/repo", id="src/extractor.rs:L146", direction="out", edge_kind="calls")
call_path(repo_path="/path/to/repo", from="extract_repo", to="extract_docstring")
impact_of(repo_path="/path/to/repo", id="src/extractor.rs:L146")
list_modules(repo_path="/path/to/repo")
```

### Docstring extraction

Best-effort, language-specific: Rust `///`/`//!`; Scala/Java/TS/JS/Kotlin/Swift
`/** … */`; Go `//`; Python triple-quoted first statement.

### Languages with built-in graph queries

Scala, Python, Rust, TypeScript/TSX, JavaScript/JSX, Go, Java. Other languages
still get definitions (markdown map) from helix tags, and whatever references
helix tags happen to capture, but no supplementary import/inheritance queries.

## Limitations

- Helix runtime location is hard-coded with override via `HELIX_RUNTIME`. If
  helix isn't installed, download a copy of `runtime/grammars/` and
  `runtime/queries/` and point `HELIX_RUNTIME` at it.
- Edge resolution is name-based (see "Resolution is name-based" above).
- Rust `enum` variants render as separate `struct` entries because helix's Rust
  tags query captures them as `definition.struct`. Cosmetic only.
- Built-in graph queries are validated against the installed grammars by
  `tests/builtin_queries_compile.rs`; a query that breaks against a future
  grammar degrades to "no edges for that pattern" (warn + skip), never a crash.
- Large monorepos may exceed the 80 KiB soft markdown limit; bump
  `--size-threshold-kib` or accept the method-docstring drop.
