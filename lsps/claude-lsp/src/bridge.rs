//! Handing a request to the Claude Code session in a Zellij pane, by writing it
//! to a file and typing a pointer to that file into the pane.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

use crate::actions::ClaudeRequest;

const ZELLIJ_TIMEOUT: Duration = Duration::from_secs(5);

/// A request that did not reach the Claude pane.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("cannot write the prompt file {}", path.display())]
    PromptFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot run zellij")]
    Spawn(#[source] std::io::Error),
    #[error("zellij action {action} failed: {stderr}")]
    Action { action: String, stderr: String },
    #[error("zellij action {action} did not finish within {}s", ZELLIJ_TIMEOUT.as_secs())]
    Timeout { action: String },
}

/// Sends requests to a Claude Code session running in a Zellij pane.
pub struct ZellijBridge {
    /// Target Zellij session name. If None, uses the current session.
    session: Option<String>,
}

impl ZellijBridge {
    /// Targets the session named by `CLAUDE_ZELLIJ_SESSION`, or the current
    /// session when it is unset.
    pub fn from_env() -> Self {
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
    async fn write_prompt_file(prompt: &str) -> Result<PathBuf, BridgeError> {
        let dir = std::env::temp_dir().join("claude-lsp");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|source| BridgeError::PromptFile {
                path: dir.clone(),
                source,
            })?;
        let path = dir.join("request.md");
        tokio::fs::write(&path, prompt)
            .await
            .map_err(|source| BridgeError::PromptFile {
                path: path.clone(),
                source,
            })?;
        Ok(path)
    }

    async fn zellij_action(&self, args: &[&str]) -> Result<(), BridgeError> {
        let mut cmd = Command::new("zellij");
        if let Some(session) = &self.session {
            cmd.arg("--session").arg(session);
        }
        cmd.arg("action").args(args).kill_on_drop(true);

        let action = args.join(" ");
        let output = tokio::time::timeout(ZELLIJ_TIMEOUT, cmd.output())
            .await
            .map_err(|_| BridgeError::Timeout {
                action: action.clone(),
            })?
            .map_err(BridgeError::Spawn)?;
        if !output.status.success() {
            return Err(BridgeError::Action {
                action,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(())
    }

    /// Focus the Claude pane from any starting pane in the layout.
    ///
    /// Layout: editor (left) | claude (top-right) | notes (bottom-right)
    /// Strategy: go left to reach the editor column, then right+up to
    /// reliably land on the top-right (Claude) pane.
    async fn focus_claude_pane(&self) -> Result<(), BridgeError> {
        self.zellij_action(&["move-focus", "left"]).await?;
        self.zellij_action(&["move-focus", "right"]).await?;
        self.zellij_action(&["move-focus", "up"]).await?;
        Ok(())
    }

    /// Send a request to the Claude Code Zellij pane, leaving focus on the
    /// editor pane.
    pub async fn send(&self, request: &ClaudeRequest) -> Result<(), BridgeError> {
        let prompt = Self::format_prompt(request);
        let prompt_file = Self::write_prompt_file(&prompt).await?;

        let message = format!(
            "Read {} and follow the editing instructions inside.",
            prompt_file.display()
        );

        self.focus_claude_pane().await?;
        self.zellij_action(&["write-chars", &message]).await?;
        self.zellij_action(&["write", "13"]).await?;
        self.zellij_action(&["move-focus", "left"]).await?;

        tracing::info!(
            file = %request.file,
            instruction = %request.instruction,
            "sent request to Claude pane"
        );

        Ok(())
    }
}
