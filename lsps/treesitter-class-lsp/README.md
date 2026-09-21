# treesitter-class-lsp

A small LSP that walks a directory, parses every source file with tree-sitter,
and exposes the class/struct/interface/enum/trait/type, function and method names
it finds through three LSP methods:

- `textDocument/completion` — completion items for the current identifier
  prefix. Class-like names only; functions and methods are not offered.
- `workspace/symbol` — fuzzy-searchable workspace-wide symbol list (the
  canonical "jump to any symbol by name" endpoint, mapped to `<space>s` in Helix
  and `Ctrl+T` in VS Code). Every result carries its kind.
- `textDocument/definition` — the definitions named exactly like the identifier
  under the cursor, wherever in the workspace they are. A method call is resolved
  through its receiver where the file says its type — `self`/`this` inside a
  class body, an annotated variable, a variable assigned from a constructor call
  (`bot = pkg.Client(...)`), or a receiver that is itself a class name — and
  answers that class's method alone. Otherwise, including when the type resolves
  to a class with no such method, resolution is by name alone: a name defined
  more than once answers every location, which Helix shows as its picker. Definitions in the buffer's own language come first, then
  the rest, each group ordered by file then position; the cap of 100 applies
  after that order. Each location
  is the definition's **name**, not the head of the block. A name nothing defines
  answers no location, so Helix prints `No definition found.` and the cursor
  stays. The answer comes from the index, which is built from the files on disk:
  an edit in a buffer is not seen until the file is saved.

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

Any language supported by Helix whose grammar ships a `tags.scm` with definition
captures (`@definition.class`, `@definition.struct`, `@definition.interface`,
`@definition.enum`, `@definition.trait`, `@definition.type`,
`@definition.function`, `@definition.method`) is picked up automatically —
including Rust, Python, Java, Go, C/C++, TypeScript, and many more.

A `@definition.function` whose definition sits inside a class, struct, interface
or enum definition of the same file is reported as a method, because several
Helix queries (Python's, for one) capture methods as functions.

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
- `definition.rs` — finds the identifier under a cursor, works out the type of
  the receiver it is called on from `self`/`this`, an annotation, a constructor
  assignment or a class name, and looks the name up in the index.
- `indexer.rs` — walks the workspace with `ignore` (respects `.gitignore`),
  parses each file, runs the tags query, extracts class-like, function and
  method definitions, and records each definition's containing class.
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
cargo run -q --example goto -- <root> <file> <line> <col> [--append <text>]
    # ask for one definition (1-based line and column) and print where it lands
```
