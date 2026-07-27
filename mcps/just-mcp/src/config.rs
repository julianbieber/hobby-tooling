use serde::Deserialize;
use std::path::PathBuf;

/// Directory names never descended into during discovery, regardless of
/// `.gitignore`. Keeps the tree walk fast and avoids build artefacts.
fn default_ignore() -> Vec<String> {
    [
        ".git",
        "target",
        "node_modules",
        ".venv",
        "venv",
        "dist",
        "build",
        ".gradle",
        ".idea",
        ".mypy_cache",
        "__pycache__",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_max_depth() -> usize {
    10
}

fn default_true() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    300
}

/// Runtime configuration, loaded from `~/.config/just_mcp.toml` (all fields
/// optional). When absent, discovery roots default to the server's current
/// working directory — the parent directory Claude was started in.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Explicit discovery roots. When empty, the server's cwd is used.
    #[serde(default)]
    pub roots: Vec<PathBuf>,
    /// Directory names to skip while walking the tree.
    #[serde(default = "default_ignore")]
    pub ignore: Vec<String>,
    /// Maximum directory depth to descend during discovery.
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// Wrap recipe execution in `mise exec --cd <dir>` when a mise config is
    /// found near the justfile, so project tool versions activate.
    #[serde(default = "default_true")]
    pub use_mise: bool,
    /// Default kill timeout for `just_run`, in seconds.
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            roots: Vec::new(),
            ignore: default_ignore(),
            max_depth: default_max_depth(),
            use_mise: true,
            default_timeout_secs: default_timeout_secs(),
        }
    }
}

impl Config {
    /// Load config from `~/.config/just_mcp.toml`, falling back to defaults if
    /// the file is missing or unreadable.
    pub fn load() -> Self {
        let Some(path) = dirs::config_dir().map(|d| d.join("just_mcp.toml")) else {
            return Config::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }
}
