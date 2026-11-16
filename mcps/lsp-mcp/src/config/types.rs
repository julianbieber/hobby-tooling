//! The model of Helix's `languages.toml`: language servers, languages, and
//! how a language finds its project root.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Root configuration containing language servers and language definitions
/// Matches Helix's Configuration struct
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct LanguagesConfig {
    #[serde(rename = "language-server", default)]
    pub language_servers: HashMap<String, LanguageServerConfig>,

    #[serde(default)]
    pub language: Vec<LanguageConfig>,
}

/// File type matcher - can be a simple extension/name or a glob pattern
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileType {
    Extension(String),
    Glob(String),
}

impl Serialize for FileType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            FileType::Extension(ext) => serializer.serialize_str(ext),
            FileType::Glob(pattern) => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("glob", pattern)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for FileType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FileTypeVisitor;

        impl<'de> serde::de::Visitor<'de> for FileTypeVisitor {
            type Value = FileType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("string or map with 'glob' key")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(FileType::Extension(value.to_string()))
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut glob_value = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "glob" {
                        glob_value = Some(map.next_value::<String>()?);
                    } else {
                        // Skip unknown keys
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }

                match glob_value {
                    Some(pattern) => Ok(FileType::Glob(pattern)),
                    None => Err(serde::de::Error::custom(
                        "expected 'glob' key in file-types map",
                    )),
                }
            }
        }

        deserializer.deserialize_any(FileTypeVisitor)
    }
}

impl FileType {
    /// Get the string representation for matching
    pub fn as_str(&self) -> &str {
        match self {
            FileType::Extension(s) => s,
            FileType::Glob(s) => s,
        }
    }

    /// Check if this matches a file extension
    pub fn matches_extension(&self, extension: &str) -> bool {
        match self {
            FileType::Extension(ext) => ext == extension,
            FileType::Glob(pattern) => {
                // Simple glob matching - just check if extension appears in pattern
                pattern.contains(extension)
            }
        }
    }
}

/// Root marker - can be either a string or a glob pattern object
#[derive(Debug, Clone, Serialize)]
pub enum RootMarker {
    String(String),
    Glob { glob: String },
}

/// Language server reference - can be a simple string or an object with configuration
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum LanguageServerRef {
    String(String),
    Config {
        name: String,
        #[serde(rename = "except-features", skip_serializing_if = "Option::is_none")]
        except_features: Option<Vec<String>>,
        #[serde(rename = "only-features", skip_serializing_if = "Option::is_none")]
        only_features: Option<Vec<String>>,
    },
}

impl<'de> Deserialize<'de> for LanguageServerRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum LanguageServerRefHelper {
            String(String),
            Config {
                name: String,
                #[serde(rename = "except-features")]
                except_features: Option<Vec<String>>,
                #[serde(rename = "only-features")]
                only_features: Option<Vec<String>>,
            },
        }

        match LanguageServerRefHelper::deserialize(deserializer)? {
            LanguageServerRefHelper::String(s) => Ok(LanguageServerRef::String(s)),
            LanguageServerRefHelper::Config {
                name,
                except_features,
                only_features,
            } => Ok(LanguageServerRef::Config {
                name,
                except_features,
                only_features,
            }),
        }
    }
}

impl LanguageServerRef {
    /// Get the name of the language server
    pub fn name(&self) -> &str {
        match self {
            LanguageServerRef::String(s) => s,
            LanguageServerRef::Config { name, .. } => name,
        }
    }
}

impl<'de> Deserialize<'de> for RootMarker {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RootMarkerHelper {
            String(String),
            Glob { glob: String },
        }

        match RootMarkerHelper::deserialize(deserializer)? {
            RootMarkerHelper::String(s) => Ok(RootMarker::String(s)),
            RootMarkerHelper::Glob { glob } => Ok(RootMarker::Glob { glob }),
        }
    }
}

impl RootMarker {
    /// Get the string value, whether it's a plain string or a glob pattern
    pub fn as_str(&self) -> &str {
        match self {
            RootMarker::String(s) => s,
            RootMarker::Glob { glob } => glob,
        }
    }

    /// Check whether this marker is present directly inside `dir`.
    /// Plain markers are matched by path existence; glob markers are matched
    /// against the directory's entry names with a minimal `*`-wildcard matcher.
    pub fn matches_in_dir(&self, dir: &Path) -> bool {
        match self {
            RootMarker::String(name) => dir.join(name).exists(),
            RootMarker::Glob { glob } => match std::fs::read_dir(dir) {
                Ok(entries) => entries.flatten().any(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .map(|name| glob_matches(glob, name))
                        .unwrap_or(false)
                }),
                Err(_) => false,
            },
        }
    }
}

/// Minimal glob matcher supporting only the `*` wildcard (matches any run of
/// characters, including empty). Sufficient for typical root markers like
/// `*.sln` or `*.csproj`. Brace expansion and `?` are not supported.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let segments: Vec<&str> = pattern.split('*').collect();
    if segments.len() == 1 {
        // No wildcard: exact match.
        return pattern == name;
    }

    let mut rest = name;
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            continue;
        }
        if i == 0 {
            // Leading segment must be a prefix.
            if let Some(stripped) = rest.strip_prefix(seg) {
                rest = stripped;
            } else {
                return false;
            }
        } else if i == segments.len() - 1 {
            // Trailing segment must be a suffix of what remains.
            return rest.ends_with(seg);
        } else if let Some((_, after)) = rest.split_once(seg) {
            rest = after;
        } else {
            return false;
        }
    }
    true
}

/// Configuration for a single language server
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LanguageServerConfig {
    /// Command to execute the language server
    pub command: String,

    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,

    /// Optional configuration passed to the language server
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<toml::Value>,

    /// Environment variables to set
    #[serde(default)]
    pub environment: HashMap<String, String>,

    /// Request timeout in milliseconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Configuration for a programming language
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LanguageConfig {
    /// Language identifier (e.g., "rust", "python")
    pub name: String,

    /// Tree-sitter scope
    pub scope: String,

    /// File types for this language (extensions or glob patterns)
    #[serde(rename = "file-types", default)]
    pub file_types: Vec<FileType>,

    /// Names of language servers to use
    #[serde(rename = "language-servers", default)]
    pub language_servers: Vec<LanguageServerRef>,

    /// Project root markers (e.g., "Cargo.toml", "package.json", or glob patterns)
    #[serde(default)]
    pub roots: Vec<RootMarker>,

    /// Enable auto-format on save
    #[serde(rename = "auto-format", default)]
    pub auto_format: bool,

    /// Language ID for LSP (if different from name)
    #[serde(
        rename = "language-id",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub language_id: Option<String>,

    /// Shebangs for script detection
    #[serde(default)]
    pub shebangs: Vec<String>,

    /// Comment tokens (can be a string or array of strings)
    #[serde(
        rename = "comment-token",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub comment_token: Option<toml::Value>,

    /// Indent configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<toml::Value>,

    /// Text width
    #[serde(
        rename = "text-width",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub text_width: Option<u32>,

    /// Formatter configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatter: Option<toml::Value>,
}

impl LanguageConfig {
    /// Walk up from `file_path` toward the filesystem root and return the
    /// nearest ancestor directory that contains one of this language's
    /// configured root markers (e.g. `Cargo.toml`). Returns `None` when no
    /// marker is found or the language declares no roots.
    ///
    /// `file_path` may point at a file or a directory; if it is a file, the
    /// search starts from its parent directory.
    pub fn find_workspace_root(&self, file_path: &Path) -> Option<PathBuf> {
        if self.roots.is_empty() {
            return None;
        }

        let start = if file_path.is_dir() {
            file_path
        } else {
            file_path.parent()?
        };

        for dir in start.ancestors() {
            if self.roots.iter().any(|marker| marker.matches_in_dir(dir)) {
                return Some(dir.to_path_buf());
            }
        }

        None
    }
}

impl LanguagesConfig {
    /// Get language server configuration by name
    pub fn get_language_server(&self, name: &str) -> Option<&LanguageServerConfig> {
        self.language_servers.get(name)
    }

    /// Find language configuration by file extension
    pub fn find_language_by_extension(&self, extension: &str) -> Option<&LanguageConfig> {
        self.language.iter().find(|lang| {
            lang.file_types
                .iter()
                .any(|ft| ft.matches_extension(extension))
        })
    }

    /// Find language configuration by language name (e.g., "rust", "python")
    pub fn find_language_by_name(&self, name: &str) -> Option<&LanguageConfig> {
        self.language.iter().find(|lang| lang.name == name)
    }

    /// Get all language server names configured across all languages
    pub fn all_language_server_names(&self) -> Vec<String> {
        self.language
            .iter()
            .flat_map(|lang| &lang.language_servers)
            .map(|ls_ref| ls_ref.name().to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_extension_matching() {
        let ft = FileType::Extension("rs".to_string());
        assert!(ft.matches_extension("rs"));
        assert!(!ft.matches_extension("toml"));
    }

    #[test]
    fn test_file_type_glob_matching() {
        let ft = FileType::Glob("*.{rs,toml}".to_string());
        assert!(ft.matches_extension("rs"));
        assert!(ft.matches_extension("toml"));
        assert!(!ft.matches_extension("py"));
    }

    #[test]
    fn test_language_server_ref_name() {
        let ref_string = LanguageServerRef::String("rust-analyzer".to_string());
        assert_eq!(ref_string.name(), "rust-analyzer");

        let ref_config = LanguageServerRef::Config {
            name: "rust-analyzer".to_string(),
            except_features: None,
            only_features: Some(vec!["diagnostics".to_string()]),
        };
        assert_eq!(ref_config.name(), "rust-analyzer");
    }

    #[test]
    fn test_root_marker_as_str() {
        let marker_string = RootMarker::String("Cargo.toml".to_string());
        assert_eq!(marker_string.as_str(), "Cargo.toml");

        let marker_glob = RootMarker::Glob {
            glob: "*.toml".to_string(),
        };
        assert_eq!(marker_glob.as_str(), "*.toml");
    }

    #[test]
    fn test_glob_matches() {
        assert!(glob_matches("*.sln", "MyApp.sln"));
        assert!(glob_matches("*.csproj", "Lib.csproj"));
        assert!(!glob_matches("*.sln", "MyApp.csproj"));
        assert!(glob_matches("Cargo.toml", "Cargo.toml"));
        assert!(!glob_matches("Cargo.toml", "Cargo.lock"));
        assert!(glob_matches("build.*", "build.gradle"));
        assert!(glob_matches("*build*", "prebuild.txt"));
        assert!(!glob_matches("*build*", "compile.txt"));
    }

    fn rust_lang_with_roots(roots: Vec<RootMarker>) -> LanguageConfig {
        LanguageConfig {
            name: "rust".to_string(),
            scope: "source.rust".to_string(),
            file_types: vec![FileType::Extension("rs".to_string())],
            language_servers: vec![LanguageServerRef::String("rust-analyzer".to_string())],
            roots,
            auto_format: false,
            language_id: None,
            shebangs: vec![],
            comment_token: None,
            indent: None,
            text_width: None,
            formatter: None,
        }
    }

    #[test]
    fn test_find_workspace_root_walks_up_to_cargo_toml() {
        // A file deep inside this crate resolves to the crate's Cargo.toml dir.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let deep_file = manifest.join("src/lsp/manager.rs");
        let lang = rust_lang_with_roots(vec![RootMarker::String("Cargo.toml".to_string())]);

        let root = lang.find_workspace_root(&deep_file);
        assert_eq!(root.as_deref(), Some(manifest.as_path()));
    }

    #[test]
    fn test_find_workspace_root_none_without_markers() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let deep_file = manifest.join("src/lsp/manager.rs");
        let lang = rust_lang_with_roots(vec![]);
        assert!(lang.find_workspace_root(&deep_file).is_none());
    }

    #[test]
    fn test_find_workspace_root_none_when_marker_absent() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let deep_file = manifest.join("src/lsp/manager.rs");
        let lang = rust_lang_with_roots(vec![RootMarker::String(
            "this-marker-does-not-exist.xyz".to_string(),
        )]);
        assert!(lang.find_workspace_root(&deep_file).is_none());
    }

    #[test]
    fn test_languages_config_get_language_server() {
        let mut config = LanguagesConfig::default();
        config.language_servers.insert(
            "rust-analyzer".to_string(),
            LanguageServerConfig {
                command: "rust-analyzer".to_string(),
                args: vec![],
                config: None,
                environment: HashMap::new(),
                timeout: None,
            },
        );

        assert!(config.get_language_server("rust-analyzer").is_some());
        assert!(config.get_language_server("nonexistent").is_none());
    }

    #[test]
    fn test_languages_config_find_language_by_extension() {
        let mut config = LanguagesConfig::default();
        config.language.push(LanguageConfig {
            name: "rust".to_string(),
            scope: "source.rust".to_string(),
            file_types: vec![FileType::Extension("rs".to_string())],
            language_servers: vec![LanguageServerRef::String("rust-analyzer".to_string())],
            roots: vec![],
            auto_format: false,
            language_id: None,
            shebangs: vec![],
            comment_token: None,
            indent: None,
            text_width: None,
            formatter: None,
        });

        let lang = config.find_language_by_extension("rs");
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().name, "rust");

        assert!(config.find_language_by_extension("py").is_none());
    }

    #[test]
    fn test_languages_config_all_language_server_names() {
        let mut config = LanguagesConfig::default();
        config.language.push(LanguageConfig {
            name: "rust".to_string(),
            scope: "source.rust".to_string(),
            file_types: vec![],
            language_servers: vec![
                LanguageServerRef::String("rust-analyzer".to_string()),
                LanguageServerRef::String("taplo".to_string()),
            ],
            roots: vec![],
            auto_format: false,
            language_id: None,
            shebangs: vec![],
            comment_token: None,
            indent: None,
            text_width: None,
            formatter: None,
        });
        config.language.push(LanguageConfig {
            name: "toml".to_string(),
            scope: "source.toml".to_string(),
            file_types: vec![],
            language_servers: vec![LanguageServerRef::String("taplo".to_string())],
            roots: vec![],
            auto_format: false,
            language_id: None,
            shebangs: vec![],
            comment_token: None,
            indent: None,
            text_width: None,
            formatter: None,
        });

        let names = config.all_language_server_names();
        assert!(names.contains(&"rust-analyzer".to_string()));
        assert!(names.contains(&"taplo".to_string()));
        // Should deduplicate - taplo appears twice
        assert_eq!(names.len(), 2);
    }
}
