# claude-lsp

Vendored into hobby-tooling from the work tooling checkout so a hobby-only machine
can build it. Same binary name as that copy — see the repo `CLAUDE.md` for why, and
for what that means when both are rebuilt.

**Journal:** `~/fun_repos/hobby-mimisbrunnr/notes/hobby-tooling/`

An LSP server that bridges Helix editor with a running Claude Code CLI session
via Zellij.

## Architecture

```
Helix <--LSP--> claude-lsp --zellij write-chars--> Claude Code (zellij pane)
```

- Helix connects to this as a language server
- Provides code actions triggered by `// claude:` comments
- Gathers file context (path, line range, surrounding code)
- Writes structured prompts to temp files
- Sends them to Claude Code via `zellij action write-chars`
- Claude Code edits files; Helix auto-reloads

## Key modules

- `main.rs` — entry point, LSP transport setup
- `lib.rs` — the modules below, as the library `main.rs` runs
- `server.rs` — `LanguageServer` trait impl, code action handling
- `actions.rs` — comment detection, prompt formatting, context gathering
- `bridge.rs` — Zellij pane communication
- `symbols.rs` — claude comments as document and workspace symbols

## Build

```bash
cargo build
cargo build --release
cargo clippy
cargo fmt
```

## Helix config

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.claude-lsp]
command = "/path/to/claude-lsp"

# Attach to languages you want claude actions in:
[[language]]
name = "rust"
language-servers = ["rust-analyzer", "claude-lsp"]
```

## Zellij

Use `layouts/dev.kdl` for the development layout. Set `CLAUDE_PANE_ID` or
configure pane discovery in bridge settings.
