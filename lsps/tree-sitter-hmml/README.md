# tree-sitter-hmml

| Field        | Value                                             |
| ------------ | ------------------------------------------------- |
| Type         | tree-sitter grammar (generated C parser, checked in) |
| Owner        | @julianbieber                                     |
| Dependencies | a C compiler — Helix builds it, no tree-sitter CLI |
| Status       | v1 shape, forked so v2 can be added               |

Syntax highlighting for **HMML** (`.hmml`), the hobby Bevy dialect of the Mental
Model Language. Semantics — diagnostics, completion, goto, references — come from
`hobby-mml-lsp` and `hobby-mml-parser`, never from here. The grammar is
deliberately loose: it recognises the shape of each line well enough to assign
highlight groups, not to validate the language.

## Why this is a fork

HMML used to borrow the work grammar via `grammar = "mml"`. That grammar's
`source.path` pointed into the work checkout, so `.hmml` highlighting worked on
exactly one machine — and an absolute path to a directory that exists nowhere
else fails the *entire* `hx` build, because `helix-term/build.rs` compiles
grammars with `STRICT = true`.

Vendoring here fixes both. The path is `<hobby-tooling>/lsps/tree-sitter-hmml`,
which `wire-helix` derives from the repo root it already resolves — the same on
every machine, and never recorded in `paths.env`. `src/parser.c` is committed and
there is no external scanner, so Helix builds it with a C compiler alone.

It also unblocks the real fix: this is still the **v1** grammar, so v2 sections
(`## components`, `## resources`, `## events`, `## systems`, `## access`) parse as
generic lines and render plainly. Being ours, it can now grow those rules —
before the fork that meant editing a grammar shared with work `.mml` files.

## Regenerating

`src/parser.c` is generated from `grammar.js` and committed on purpose: Helix
needs it, and requiring the tree-sitter CLI on every machine would reintroduce
the portability problem this fork removes. After editing `grammar.js`:

```bash
cargo install tree-sitter-cli   # only on the machine making the change
tree-sitter generate            # regenerates src/parser.c
tree-sitter test                # runs test/corpus
just helix-build                # or: hx --grammar build
```

`tree-sitter generate` runs `grammar.js` with Node; if `node` is not on `PATH`,
use `mise exec node@latest -- tree-sitter generate`.

**Keep `name: "hmml"` in `grammar.js`.** Helix `dlsym`s `tree_sitter_<name>`, so
if the grammar name and the exported symbol disagree the library loads nothing
and the language silently falls back to having no parser.

## Wiring

`just wire-helix` does all of it — generates the `[[grammar]] name = "hmml"`
block into `~/.config/helix/languages.local.toml` and installs
`queries/highlights.scm` into the Helix checkout's `runtime/queries/hmml/`.
Helix resolves queries by *language name*, which is why they live under `hmml/`.
