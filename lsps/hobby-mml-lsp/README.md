# hobby-mml-lsp

| Field        | Value                                              |
| ------------ | -------------------------------------------------- |
| Type         | lsp + CLI                                          |
| Binaries     | `hobby-mml-lsp`, `hobby-mml-parser`                |
| Owner        | @julianbieber                                      |
| Dependencies | tree-sitter-mml grammar (from the work tooling repo) |
| Status       | experimental                                       |

## Description

Fork of the work `mml-lsp` for hobby specs. Language server (completion, hover,
goto-definition, references, document symbols, diagnostics) plus the
`hobby-mml-parser --lint` CLI the `mml-*` skills shell out to.

## Why a fork rather than a shared binary

The only real difference is spec→source path resolution:

| | work | hobby |
|---|---|---|
| spec | `notes/{topic}/{task}/<unit>.mml` | `notes/{project}/{topic}/{task}/<unit>.mml` |
| checkout | `~/worktrees/{topic}/{task}/{repo}` | `~/fun_worktrees/{project}/{topic}/{task}` |
| fallback | `~/projects/<repo>` | `~/fun_repos/<repo>` |

The hobby layout has **no repo level** — a task is one repo, so the task
directory *is* the checkout. `mml-parser/src/paths.rs` is the only module that
changed; the lexer, parser, lint, and validate passes are untouched.

Parameterizing one binary by env var was the alternative and would avoid the
duplication, but it meant editing a work repo for a personal need. The fork keeps
work untouched at the cost of hand-cherry-picking upstream `mml-parser` fixes.

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
scope = "source.mml"
file-types = [{ glob = "**/hobby-mimisbrunnr/notes/**/*.mml" }]
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

- **Depends on the work repo for the grammar.** `grammar = "mml"` points at
  `team-constellation-tooling/lsps/tree-sitter-mml`. Moving or deleting that
  checkout breaks hobby MML highlighting.
- Upstream `mml-parser` fixes must be cherry-picked by hand.
- The MML *language spec* is not vendored here — it stays at its work path, to
  keep work-authored content out of a repo that gets pushed to GitHub.
