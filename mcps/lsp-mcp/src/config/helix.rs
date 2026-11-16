//! Locating and reading Helix's `languages.toml`.

use super::types::LanguagesConfig;
use std::fs;
use std::path::{Path, PathBuf};

/// The Helix language configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("HOME environment variable is not set; cannot locate Helix config")]
    NoHome,
    #[error("Helix config not found at: {}", .path.display())]
    NotFound { path: PathBuf },
    #[error("cannot read Helix config {}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid Helix config {}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

/// Load Helix languages configuration from `$HOME/.config/helix/languages.toml`.
///
/// Override with the `LSP_MCP_HELIX_CONFIG` environment variable if your Helix
/// config lives elsewhere.
pub fn load_helix_config() -> Result<LanguagesConfig, ConfigError> {
    load_config_from_file(helix_config_path()?)
}

/// Resolve the Helix `languages.toml` path, honoring `LSP_MCP_HELIX_CONFIG`.
pub fn helix_config_path() -> Result<PathBuf, ConfigError> {
    if let Some(override_path) = std::env::var_os("LSP_MCP_HELIX_CONFIG") {
        return Ok(PathBuf::from(override_path));
    }
    let home = std::env::var_os("HOME").ok_or(ConfigError::NoHome)?;
    Ok(PathBuf::from(home).join(".config/helix/languages.toml"))
}

/// Load configuration from a specific file path.
pub fn load_config_from_file<P: AsRef<Path>>(path: P) -> Result<LanguagesConfig, ConfigError> {
    let path = path.as_ref().to_path_buf();
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConfigError::NotFound { path });
        }
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    parse_config(&content).map_err(|source| ConfigError::Parse { path, source })
}

pub fn parse_config(content: &str) -> Result<LanguagesConfig, toml::de::Error> {
    toml::from_str(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_config() {
        let toml_content = r#"
[language-server.rust-analyzer]
command = "rust-analyzer"
args = []

[[language]]
name = "rust"
scope = "source.rust"
file-types = ["rs"]
language-servers = ["rust-analyzer"]
roots = ["Cargo.toml"]
"#;

        let config = parse_config(toml_content).unwrap();
        assert_eq!(config.language_servers.len(), 1);
        assert_eq!(config.language.len(), 1);

        let rust_analyzer = config.get_language_server("rust-analyzer").unwrap();
        assert_eq!(rust_analyzer.command, "rust-analyzer");

        let rust_lang = &config.language[0];
        assert_eq!(rust_lang.name, "rust");
        assert_eq!(rust_lang.file_types.len(), 1);
        assert!(rust_lang.file_types[0].matches_extension("rs"));
    }

    #[test]
    fn test_find_language_by_extension() {
        let toml_content = r#"
[[language]]
name = "rust"
scope = "source.rust"
file-types = ["rs"]
language-servers = ["rust-analyzer"]

[[language]]
name = "python"
scope = "source.python"
file-types = ["py", "pyi"]
language-servers = ["pylsp"]
"#;

        let config = parse_config(toml_content).unwrap();

        let rust = config.find_language_by_extension("rs").unwrap();
        assert_eq!(rust.name, "rust");

        let python = config.find_language_by_extension("py").unwrap();
        assert_eq!(python.name, "python");

        assert!(config.find_language_by_extension("unknown").is_none());
    }
}
