use thiserror::Error;

#[derive(Debug, Error)]
pub enum LspMcpError {
    #[error("LSP server not started for language: {0}")]
    ServerNotStarted(String),

    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    #[error("LSP request failed: {0}")]
    LspRequestFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, LspMcpError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = LspMcpError::ServerNotStarted("rust".to_string());
        assert_eq!(err.to_string(), "LSP server not started for language: rust");

        let err = LspMcpError::InvalidUri("/invalid/path".to_string());
        assert_eq!(err.to_string(), "Invalid URI: /invalid/path");

        let err = LspMcpError::ConfigError("Missing field".to_string());
        assert_eq!(err.to_string(), "Configuration error: Missing field");
    }

    #[test]
    fn test_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json");
        assert!(json_err.is_err());

        let err: LspMcpError = json_err.unwrap_err().into();
        assert!(matches!(err, LspMcpError::JsonError(_)));
    }

    #[test]
    fn test_result_type() {
        let success: Result<i32> = Ok(42);
        assert_eq!(success.unwrap(), 42);

        let failure: Result<i32> = Err(LspMcpError::ConfigError("test".to_string()));
        assert!(failure.is_err());
    }
}
