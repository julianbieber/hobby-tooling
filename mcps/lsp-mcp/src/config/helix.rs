use super::types::LanguagesConfig;
use crate::error::{LspMcpError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Load Helix languages configuration from `$HOME/.config/helix/languages.toml`.
///
/// Override with the `LSP_MCP_HELIX_CONFIG` environment variable if your Helix
/// config lives elsewhere.
pub fn load_helix_config() -> Result<LanguagesConfig> {
    let config_path = helix_config_path()?;

    if config_path.exists() {
        load_config_from_file(&config_path)
    } else {
        Err(LspMcpError::ConfigError(format!(
            "Helix config not found at: {}",
            config_path.display()
        )))
    }
}

/// Resolve the Helix `languages.toml` path, honoring `LSP_MCP_HELIX_CONFIG`.
pub fn helix_config_path() -> Result<PathBuf> {
    if let Ok(override_path) = std::env::var("LSP_MCP_HELIX_CONFIG") {
        return Ok(PathBuf::from(override_path));
    }
    let home = std::env::var("HOME").map_err(|_| {
        LspMcpError::ConfigError(
            "HOME environment variable is not set; cannot locate Helix config".to_string(),
        )
    })?;
    Ok(PathBuf::from(home).join(".config/helix/languages.toml"))
}

/// Load configuration from a specific file path
pub fn load_config_from_file<P: AsRef<Path>>(path: P) -> Result<LanguagesConfig> {
    let content = fs::read_to_string(path.as_ref()).map_err(|e| LspMcpError::IoError(e))?;

    parse_config(&content)
}

/// Parse TOML content into LanguagesConfig
pub fn parse_config(content: &str) -> Result<LanguagesConfig> {
    toml::from_str(content).map_err(|e| LspMcpError::TomlError(e))
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
