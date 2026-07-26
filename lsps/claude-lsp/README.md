# Julian Claude LSP

| Field        | Value                                                                                    |
| ------------ | ---------------------------------------------------------------------------------------- |
| Type         | lsp (Rust source + binary)                                                               |
| Owner        | @julianbieber                                                                            |
| Dependencies | Rust toolchain (edition 2024); Helix (or another LSP client); Zellij; a Claude Code pane |
| Status       | experimental                                                                             |

## Description

LSP server that bridges the Helix editor with a running Claude Code CLI session
inside Zellij. It exposes **code actions** triggered by `// claude:` comments in
the source: Helix surfaces the action, `claude-lsp` gathers context (file path,
line range, surrounding code), writes a structured prompt to a temp file, and
sends it to the Claude Code pane via `zellij action write-chars`. Claude Code
then edits files directly; Helix auto-reloads the buffer.

```
Helix <--LSP--> claude-lsp --zellij write-chars--> Claude Code (zellij pane)
```

The review skills annotate code with `// claude:` comments; this LSP is the
editor-side way to act on one of those comments with a keystroke instead of a
`/skill` call.

## Installation

### Build

```bash
cd ~/fun_repos/hobby-tooling/lsps/claude-lsp
cargo install --path .
```

Produces `~/.cargo/bin/claude-lsp`.

### Wire into Helix

Add to `~/.config/helix/languages.toml`:

```toml
[language-server.claude-lsp]
command = "claude-lsp"

[[language]]
name = "rust"
language-servers = ["rust-analyzer", "claude-lsp"]

[[language]]
name = "python"
language-servers = ["pylsp", "claude-lsp"]

[[language]]
name = "scala"
language-servers = ["metals", "claude-lsp"]
```

Attach to any language you want Claude code actions in.

### Zellij layout

A reference layout lives at `layouts/dev.kdl`. It opens a Helix pane next to a
Claude Code pane; `claude-lsp` discovers the Claude pane automatically (or set
`CLAUDE_PANE_ID` to pin it).

## Usage

1. Run `claude` in a Zellij pane (must be running; LSP writes to that pane).
2. Open a file with attached `claude-lsp` in Helix.
3. Place a `// claude: {request}` comment above the code you want changed.
4. Trigger Helix code actions (`Space a`) — the LSP forwards the prompt +
   context to the Claude pane.
5. Claude edits the file; Helix reloads the buffer automatically.

## Known Limitations

- Hard depends on Zellij for pane communication; no support for tmux / native
  terminal multiplexers yet.
- No feedback loop — the LSP doesn't wait for Claude's response, it just
  dispatches the prompt.
- Pane discovery is heuristic unless `CLAUDE_PANE_ID` is set.
- `~/Notes/ai_tasks/llm_workflow/helix_integration/`, referenced in this crate's
  `CLAUDE.md`, is a stale path from before the notes moved into a dedicated
  repo; ignore it.
