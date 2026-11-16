//! What can stop the server from starting or keep it from serving.

use crate::config::ConfigError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LspMcpError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("cannot determine the workspace root")]
    WorkspaceRoot(#[source] std::io::Error),

    #[error("MCP handshake failed")]
    Initialize(#[source] Box<rmcp::service::ServerInitializeError>),

    #[error("MCP service task failed")]
    Service(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, LspMcpError>;

/// Render an error with every source below it, outermost first.
pub fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_error_display() {
        let err = LspMcpError::Config(ConfigError::NotFound {
            path: PathBuf::from("/nowhere/languages.toml"),
        });
        assert_eq!(
            err.to_string(),
            "Helix config not found at: /nowhere/languages.toml"
        );

        let err = LspMcpError::Config(ConfigError::NoHome);
        assert_eq!(
            err.to_string(),
            "HOME environment variable is not set; cannot locate Helix config"
        );

        let err = LspMcpError::WorkspaceRoot(std::io::ErrorKind::NotFound.into());
        assert_eq!(
            error_chain(&err),
            "cannot determine the workspace root: entity not found"
        );
    }

    #[test]
    fn test_config_parse_error_conversion() {
        let toml_err = crate::config::parse_config("not = [valid");
        assert!(toml_err.is_err());

        let err: LspMcpError = ConfigError::Parse {
            path: PathBuf::from("languages.toml"),
            source: toml_err.unwrap_err(),
        }
        .into();
        assert!(matches!(
            err,
            LspMcpError::Config(ConfigError::Parse { .. })
        ));
        assert!(error_chain(&err).starts_with("invalid Helix config languages.toml: "));
    }

    #[test]
    fn test_result_type() {
        let success: Result<i32> = Ok(42);
        assert!(matches!(success, Ok(42)));

        let failure: Result<i32> = Err(LspMcpError::Config(ConfigError::NoHome));
        assert!(failure.is_err());
    }
}
