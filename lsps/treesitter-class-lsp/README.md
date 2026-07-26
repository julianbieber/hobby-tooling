# treesitter-class-lsp

A small LSP that walks a directory, parses every source file with tree-sitter,
and exposes the class/struct/interface/enum/trait/type names it finds through
two LSP methods:

- `textDocument/completion` — completion items for the current identifier
  prefix.
- `workspace/symbol` — fuzzy-searchable workspace-wide symbol list (the
  canonical "jump to any class by name" endpoint, mapped to `<space>s` in Helix
  and `Ctrl+T` in VS Code).

It reuses the tree-sitter grammars and `tags.scm` queries that already live in a
local Helix checkout, so there is no grammar to build.

## Worktree awareness

On `initialize` the server inspects the client's workspace root and, if it sits
inside `~/worktrees/TOPIC/TASK/...`, uses the **TASK** directory as its indexing
root. That layout puts several repo checkouts under one task directory, so this
widening is what lets a single LSP instance surface every class across them.

Hobby tasks do not need it: `~/fun_worktrees/PROJECT/TOPIC/TASK` **is** the repo,
and the path does not match `/worktrees/` (the component is `fun_worktrees`), so
the client root is used as-is — which is already the correct scope. Any root
outside `~/worktrees/` behaves the same way.

## Languages

Any language supported by Helix whose grammar ships a `tags.scm` with class-like
captures (`@definition.class`, `@definition.struct`, `@definition.interface`,
`@definition.enum`, `@definition.trait`, `@definition.type`) is picked up
automatically — including Rust, Python, Java, Go, C/C++, TypeScript, and many
more.

Scala has no `tags.scm` in Helix, so a built-in fallback query ships in
`src/builtin_queries/scala_tags.scm`.

## Build

```bash
cargo install --path ~/fun_repos/hobby-tooling/lsps/treesitter-class-lsp
# → ~/.cargo/bin/treesitter-class-lsp
```

The grammar and query paths default to
`/home/jbieber/outside_repos/helix/runtime`. Override with
`HELIX_RUNTIME=/path/to/helix/runtime`.

## Helix config

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.treesitter-class-lsp]
command = "treesitter-class-lsp"

# Attach to languages you want class-name completions / workspace symbols in:
[[language]]
name = "rust"
language-servers = ["rust-analyzer", "treesitter-class-lsp"]

[[language]]
name = "scala"
language-servers = ["metals", "treesitter-class-lsp"]
```

## Architecture

- `main.rs` — tokio runtime + tracing setup.
- `server.rs` — `tower_lsp::LanguageServer` implementation. Background-indexes
  on `initialized`, re-indexes one file per `did_open` / `did_change` /
  `did_save`.
- `languages.rs` — parses Helix's `languages.toml` into `extension → language`
  and resolves `; inherits: …` directives in `tags.scm` query files.
- `grammars.rs` — dynamically loads Helix's pre-built `.so` grammars via
  `libloading` and caches the resulting `tree_sitter::Language`s.
- `indexer.rs` — walks the workspace with `ignore` (respects `.gitignore`),
  parses each file, runs the tags query, extracts class-like definitions.
- `fuzzy.rs` — `nucleo-matcher`-based ranking for completion and
  workspace-symbol queries.
- `worktree.rs` — extracts the TASK directory from a worktree path.

## Development

```bash
cargo test            # unit + integration tests
cargo clippy --all-targets -- -D warnings
cargo run --example debug [path]            # print every indexed symbol
cargo run --example lsp_client -- \
    target/debug/treesitter-class-lsp [path] [query]   # drive the real LSP over stdio
```
