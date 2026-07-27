# Julian lsp-mcp

| Field        | Value                                                                     |
| ------------ | ------------------------------------------------------------------------- |
| Type         | mcp (Rust source + binary)                                                |
| Owner        | @julianbieber                                                             |
| Dependencies | Rust toolchain; a Helix `languages.toml`; LSP servers listed in that file |
| Status       | experimental                                                              |

## Description

MCP server that bridges Claude with Language Server Protocol servers. It reads
Helix's `languages.toml` to auto-discover LSP configuration, spawns/caches LSP
processes on demand, and exposes LSP capabilities (diagnostics, completion,
hover, goto-definition, find-references, document symbols, workspace symbols,
etc.) as MCP tools.

Because it reuses your Helix config, any language server already working in
Helix shows up automatically in Claude.

See `CLAUDE.md` in this folder for the full architecture and module breakdown
(auto-loaded when working in `mcps/julian-lsp-mcp/`).

## Installation

### Build & install

```bash
cargo install --path ~/fun_repos/hobby-tooling/mcps/lsp-mcp
```

Produces `~/.cargo/bin/lsp-mcp`.

### Wire into Claude Code

Add to `~/.claude/settings.json` (or the MCP config location your setup uses):

```json
{
  "mcpServers": {
    "lsp-mcp": {
      "command": "/home/<you>/.cargo/bin/lsp-mcp"
    }
  }
}
```

Or drop a `.mcp.json` next to the repo you're working in:

```json
{
  "mcpServers": {
    "lsp-mcp": { "command": "lsp-mcp", "args": [] }
  }
}
```

Restart Claude. The server surfaces its tools as `mcp__lsp-mcp__*` (hover,
completion, document_symbol, find_references, goto_definition, workspace_symbol,
diagnostics, …).

### Helix config

`lsp-mcp` reads `$HOME/.config/helix/languages.toml` by default. Override with
`LSP_MCP_HELIX_CONFIG=/path/to/languages.toml` if your Helix config lives
elsewhere. The language servers referenced there must be on your `PATH`.

## Usage

Transparent after installation. Representative prompts:

- "What are the diagnostics for `src/server.rs`?"
- "Use lsp-mcp to find references to `LspManager`."
- "Show me document symbols for `src/main.rs`."
- "Give me the completion list at line 42, column 10 of `foo.rs`."

## Known Limitations

- Hard-depends on a Helix `languages.toml` as the source of LSP config. Without
  Helix (or a hand-rolled equivalent file), there's nothing to discover.
- LSP server processes are cached globally per `(workspace, server_name)`; if an
  LSP crashes, the cache entry is evicted on next `is_alive()` check but there's
  no proactive restart.
- Integration tests must run with `--test-threads=1` — see `CLAUDE.md`.
- `target/` is built locally and `.gitignore`d; don't commit it.
