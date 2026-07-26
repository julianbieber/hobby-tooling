use std::io::Write;
use tokio::process::Command;

use crate::actions::ClaudeRequest;

/// Sends requests to a Claude Code session running in a Zellij pane.
pub struct ZellijBridge {
    /// Target Zellij session name. If None, uses the current session.
    session: Option<String>,
}

impl ZellijBridge {
    pub fn new() -> Self {
        Self {
            session: std::env::var("CLAUDE_ZELLIJ_SESSION").ok(),
        }
    }

    /// Format the request into a prompt string for Claude Code.
    fn format_prompt(request: &ClaudeRequest) -> String {
        format!(
            "In {file}:{start}-{end}, {instruction}\n\n\
             Context:\n{context}",
            file = request.file,
            start = request.start_line,
            end = request.end_line,
            instruction = request.instruction,
            context = request.context,
        )
    }

    /// Write the prompt to a temp file and return the path.
    fn write_prompt_file(prompt: &str) -> anyhow::Result<std::path::PathBuf> {
        let dir = std::env::temp_dir().join("claude-lsp");
        std::fs::create_dir_all(&dir)?;

        let path = dir.join("request.md");
        let mut f = std::fs::File::create(&path)?;
        f.write_all(prompt.as_bytes())?;

        Ok(path)
    }

    /// Build a zellij command with session targeting if configured.
    fn zellij_cmd(&self) -> Command {
        let mut cmd = Command::new("zellij");
        if let Some(session) = &self.session {
            cmd.arg("--session").arg(session);
        }
        cmd
    }

    /// Run a zellij action (no pane targeting).
    async fn zellij_action(&self, args: &[&str]) -> anyhow::Result<()> {
        let mut cmd = self.zellij_cmd();
        cmd.arg("action");
        for arg in args {
            cmd.arg(arg);
        }
        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("zellij action {args:?} failed: {stderr}");
        }
        Ok(())
    }

    /// Write chars to the focused pane.
    async fn write_chars(&self, text: &str) -> anyhow::Result<()> {
        let mut cmd = self.zellij_cmd();
        cmd.arg("action").arg("write-chars").arg(text);

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("zellij write-chars failed: {stderr}");
        }
        Ok(())
    }

    /// Send Enter to the focused pane.
    async fn send_enter(&self) -> anyhow::Result<()> {
        // Send carriage return (13) — this is what Enter produces in a terminal
        let mut cmd = self.zellij_cmd();
        cmd.arg("action").arg("write").arg("13");

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("zellij write (enter) failed: {stderr}");
        }
        Ok(())
    }

    /// Focus the Claude pane from any starting pane in the layout.
    ///
    /// Layout: editor (left) | claude (top-right) | notes (bottom-right)
    /// Strategy: go left to reach the editor column, then right+up to
    /// reliably land on the top-right (Claude) pane.
    async fn focus_claude_pane(&self) -> anyhow::Result<()> {
        self.zellij_action(&["move-focus", "left"]).await?;
        self.zellij_action(&["move-focus", "right"]).await?;
        self.zellij_action(&["move-focus", "up"]).await?;
        Ok(())
    }

    /// Send a request to the Claude Code Zellij pane.
    pub async fn send(&self, request: &ClaudeRequest) -> anyhow::Result<()> {
        let prompt = Self::format_prompt(request);
        let prompt_file = Self::write_prompt_file(&prompt)?;

        let message = format!(
            "Read {} and follow the editing instructions inside.",
            prompt_file.display()
        );

        // Navigate to the Claude pane from any starting pane, write the
        // message, then return focus to the editor (left) pane.
        self.focus_claude_pane().await?;
        self.write_chars(&message).await?;
        self.send_enter().await?;
        self.zellij_action(&["move-focus", "left"]).await?;

        tracing::info!(
            file = %request.file,
            instruction = %request.instruction,
            "sent request to Claude pane"
        );

        Ok(())
    }
}
