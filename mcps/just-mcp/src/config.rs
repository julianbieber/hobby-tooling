//! Runtime configuration of the server: where to look for justfiles, what to
//! skip, and how recipes are run.

use serde::Deserialize;
use std::path::PathBuf;

/// The configuration file exists but could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid config {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

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
    /// Load config from `~/.config/just_mcp.toml`, falling back to defaults
    /// when there is no such file. A file that exists but cannot be read or
    /// parsed is an error, so a typo does not silently run with defaults.
    pub fn load() -> Result<Self, ConfigError> {
        let Some(path) = dirs::config_dir().map(|d| d.join("just_mcp.toml")) else {
            return Ok(Config::default());
        };
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        toml::from_str(&contents).map_err(|source| ConfigError::Parse { path, source })
    }
}
