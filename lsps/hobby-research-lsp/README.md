# hobby-research-lsp

| Field        | Value                                       |
| ------------ | ------------------------------------------- |
| Type         | language server (Rust)                      |
| Owner        | @julianbieber                               |
| Dependencies | `tower-lsp`, `serde`, `serde_json`          |
| Status       | v2 — goto-definition on links and refs; workspace symbols over every card |

Serves `.hresearch` files (see `skills/research/README.md`). First capability:
`gd` on a URL opens that URL in the browser, and `gd` on a `[k]` ref opens
source `[k]`'s URL.

Source `[k]` is the line that starts with `[k] <url>` — after the frame and the
`source` label on a card, flush left under a matrix or figure. `[unsourced]`,
and a ref the file lists no source for, open nothing.

A link is the same token `tree-sitter-hresearch` colours — from `http://` or
`https://` to the next whitespace or card frame `│` — so the `#L42-L49` anchor
on a snippet link is kept and the frame's padding is not.

The server first asks the editor to open it (`window/showDocument` with
`external: true`); when the editor declines, it runs `$BROWSER <url>`, falling
back to `xdg-open`. The same route `hobby-mml-lsp` takes for a dependency's
docs. Goto-definition itself returns nothing, so the cursor stays put.

Second capability, `workspace/symbol` (`space S` in Helix): it runs
`hresearch symbols <query>` from the workspace root and returns the cards of
every project as three kinds of symbol — the question (`File`), each identifier
the card cites (`Struct`, `Enum`, `Interface`, `Function` or `Constant`, by the
docs.rs item URL that names it) and the `crate@version` (`Package`). The
container reads `<project>/<task> <crate>@<version> <tag>`, so the tag shows
beside a match. Helix does not filter the name column, so the filtering is
`hresearch`'s: a case-insensitive substring per query word. A card retracted with `hresearch retract` is not listed. `hresearch` must be
on `PATH`; when it is not, the answer is empty. `just wire-helix` attaches the
server to `rust` and `markdown` with `only-features = ["workspace-symbols"]`.

No diagnostics: `hresearch check` is what validates a research file.

## Install

```
just install-research   # cargo install hobby-research-lsp
just wire-helix         # registers it and routes .hresearch to it
```
