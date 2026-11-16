//! The Helix language configuration: where it lives and what it says.

pub mod helix;
pub mod types;

pub use helix::{
    ConfigError, helix_config_path, load_config_from_file, load_helix_config, parse_config,
};
pub use types::{
    FileType, LanguageConfig, LanguageServerConfig, LanguageServerRef, LanguagesConfig, RootMarker,
};
