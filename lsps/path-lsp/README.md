# Julian Path LSP

| Field        | Value                                                                      |
| ------------ | -------------------------------------------------------------------------- |
| Type         | lsp (Rust source + binary)                                                 |
| Owner        | @julianbieber                                                              |
| Dependencies | Rust toolchain (stable edition 2024); editor with LSP client (Helix, etc.) |
| Status       | experimental                                                               |
| Upstream     | local (no remote configured at import time)                                |

## Description

Tiny LSP server that implements **go-to-definition on filesystem paths**. Put
the cursor on a path token in any served file and trigger `goto_definition`
(`gd` in Helix) to open the referenced file. Works for absolute paths,
`~`-relative paths, and relative paths — and jumps to a `:line:col` suffix when
present (handy for stack traces, grep output, and log lines).

Relative paths are resolved against the directory of the current file first,
then against the editor's workspace root as a fallback.

Examples that resolve when the cursor is on them:

- `./src/main.rs`, `../README.md` (relative to the current file)
- `src/main.rs` (relative to the current file, else the workspace root)
- `/etc/hosts` (absolute)
- `~/projects/foo` (tilde-expanded)
- `src/main.rs:42:10` (opens the file at line 42, column 10)

Capabilities:

- `textDocument/definition` — resolves the path under the cursor to a `Location`
- `textDocument/sync` = full (used to recover the path token at the cursor;
  falls back to reading the file from disk if the document was never synced)

It only contributes definitions — no completion, hover, or diagnostics — so it
composes cleanly as a fallback alongside real language servers.

## Installation

### Build

```bash
cd ~/fun_repos/hobby-tooling/lsps/path-lsp
cargo install --path .
```

Produces `~/.cargo/bin/path_lsp`.

### Wire into Helix (example)

Add the server and attach it to the languages where you want path navigation.
List it **last** in each `language-servers` array so it acts as a fallback —
Helix uses the first server that returns a definition, so real code symbols
still resolve via the primary language server, and `path_lsp` only answers when
nothing else does.

```toml
[language-server.path-relative]
command = "path_lsp"

[[language]]
name = "markdown"
language-servers = ["marksman", "markdown-oxide", "path-relative"]

[[language]]
name = "rust"
language-servers = ["rust-analyzer", "path-relative"]
```

Restart Helix, place the cursor on a path, and press `gd`.

## Usage

Cursor on a path token → `gd` (Helix `goto_definition`). If the path resolves to
an existing file or directory, the editor navigates to it; with a `:line` or
`:line:col` suffix the cursor lands at that position. If nothing resolves, the
server returns no result and the editor falls through to other language servers.

## Known Limitations

- **No spaces in paths.** The token scanner stops at whitespace and quotes, so a
  path containing spaces won't be captured whole.
- **Directories are best-effort.** A resolved directory is returned as a
  `Location`; what the editor does with it (open a buffer, do nothing) is
  editor-dependent.
- Token detection treats alphanumerics and `/ . - _ ~ @ + :` as part of the
  path. Other punctuation ends the token.
- The path must exist on disk at request time — there is no creation or fuzzy
  matching.
- UTF-16 column math is approximated as `char` counts — fine for ASCII paths,
  off for multi-byte filenames.
