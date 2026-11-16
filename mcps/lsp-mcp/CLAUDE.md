# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository.

## What This Is

An MCP server (Rust) that bridges Claude with Language Server Protocol servers.
It reads Helix editor's `languages.toml` to auto-discover LSP configuration,
spawns/caches LSP processes, and exposes LSP capabilities as MCP tools.

## Build & Test Commands

```bash
cargo build                                                    # Build
cargo test --lib                                               # Unit tests only
cargo test --test lsp_integration -- --test-threads=1          # Integration tests (MUST be single-threaded)
cargo test -- --test-threads=1                                 # All tests
cargo clippy                                                   # Lint
cargo fmt                                                      # Format
```

Integration tests require `rust-analyzer` in PATH and use the global LSP cache,
so `--test-threads=1` is mandatory to avoid cache conflicts. Tests call
`clear_global_cache()` before running.

## Architecture

```
MCP Client (Claude) → stdio JSON-RPC → LspMcpServer (server.rs)
                                            ↓ tool dispatch via #[tool] macros
                                       LspManager (lsp/manager.rs)
                                            ↓ get_or_start_server / get_server_for_language
                                       LspClient (lsp/client.rs)
                                            ↓ JSON-RPC over stdin/stdout
                                       LSP Server Process (e.g., rust-analyzer)
```

**Key design decisions:**

- **Global LSP process cache**: `static LazyLock<tokio::sync::RwLock<HashMap>>` in `manager.rs`
  keyed by `(canonical_workspace_path, server_name)`. Prevents duplicate LSP
  processes across manager instances. Checks `is_alive()` before returning
  cached clients.
- **Notification capture**: `send_request` in `client.rs` processes
  server-initiated notifications inline while waiting for responses.
  `publishDiagnostics` notifications are stored in a per-client
  `HashMap<Url, Vec<Diagnostic>>`.
- **Pull + push diagnostics**: `get_diagnostics` tries the LSP 3.17 pull model
  (`textDocument/diagnostic`) first, falls back to stored notification
  diagnostics.
- **Document opening**: Most LSP requests require `did_open` first. The MCP tool
  handlers read the file from disk and send `did_open` before each request.

## Module Responsibilities

- **`server.rs`**: MCP tool definitions and response conversion. All MCP tools
  live here as `#[tool]` methods on `LspMcpServer`.
- **`lsp/client.rs`**: Raw JSON-RPC communication with a single LSP process.
  Owns stdin/stdout pipes behind `tokio::sync::Mutex` (held across the
  write/read `.await`s); diagnostics and the child handle are behind
  `parking_lot::Mutex`. Every request and notification is bounded by
  `REQUEST_TIMEOUT`; a timeout kills the process so the manager respawns it.
- **`lsp/manager.rs`**: Maps language IDs / file extensions → server configs →
  cached `LspClient` instances.
- **`config/`**: Parses Helix `languages.toml`. `types.rs` has the full config
  model with custom serde deserializers for `FileType`, `RootMarker`,
  `LanguageServerRef`.
- **`tools/`**: MCP parameter and response types (serde + schemars for JSON
  Schema). `diagnostics.rs` also has conversion helpers
  (`convert_lsp_diagnostic`, `extract_edits_from_code_action`).
- **`fuzzy.rs`**: Wraps `nucleo-matcher` for workspace symbol ranking.
  Smart-case matching.

## Adding a New MCP Tool

1. Define param/result types in `tools/` with `Deserialize`, `JsonSchema`,
   `Serialize` derives and `#[schemars(description)]` annotations.
2. Add the LSP request method to `LspClient` in `lsp/client.rs`.
3. Add a `#[tool(description = "...")]` method to the
   `#[tool_router] impl LspMcpServer` block in `server.rs`. Follow the existing
   pattern: `open_document` (parse URI → read file → get client → did_open),
   then the LSP request with `.map_err(doc.failed())?`, then convert the
   response and return it with `json_text`.

## Conventions

- Errors are `thiserror` enums per layer: `ConfigError` (config), `LspError`
  (client), `ManagerError` (manager), a private `ToolError` in `server.rs`
  mapped to MCP `invalid_params` / `internal_error`, and `LspMcpError` for
  startup. `unwrap`/`expect`/indexing are denied by the crate's lints.
- All LSP I/O is async via tokio. Logging goes through `tracing` to stderr;
  stdout carries the MCP protocol.
- LSP positions are 0-indexed `(line, character)` throughout.
- URIs are `lsp_types::Url`, converted to/from file paths at tool boundaries.
- LSP processes spawned with `.kill_on_drop(true)`.
