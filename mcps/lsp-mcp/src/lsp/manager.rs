//! Choosing the language server for a language or file, and keeping one
//! running process per project and server.

use super::client::{LspClient, LspError};
use crate::config::LanguagesConfig;
use lsp_types::SymbolInformation;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tokio::sync::RwLock;

/// Global cache for LSP clients, keyed by (canonical workspace path, server name)
/// This ensures LSP processes are reused across LspManager instances for the same project
type ClientCache = HashMap<(PathBuf, String), Arc<LspClient>>;

static GLOBAL_LSP_CACHE: LazyLock<RwLock<ClientCache>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// No language server could be provided for the request.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("language server not found in config: {0}")]
    UnknownServer(String),
    #[error("file has no extension: {}", .0.display())]
    NoExtension(PathBuf),
    #[error("no language configured for extension: {0}")]
    NoLanguageForExtension(String),
    #[error("no language configured with name: {0}")]
    UnknownLanguage(String),
    #[error("no language server configured for {0}")]
    NoServerForLanguage(String),
    #[error("language server {server} could not be started")]
    Start {
        server: String,
        #[source]
        source: LspError,
    },
    #[error("language server {server} failed")]
    Request {
        server: String,
        #[source]
        source: LspError,
    },
}

/// Manages multiple LSP server instances
pub struct LspManager {
    /// Configuration from languages.toml
    config: LanguagesConfig,

    /// Workspace root path (canonicalized for consistent cache keys)
    workspace_root: PathBuf,
}

impl LspManager {
    /// Create a new LSP manager
    pub fn new(config: LanguagesConfig, workspace_root: PathBuf) -> Self {
        // Canonicalize the workspace root for consistent cache keys
        let canonical_root = workspace_root.canonicalize().unwrap_or(workspace_root);

        Self {
            config,
            workspace_root: canonical_root,
        }
    }

    /// Get or start an LSP server by name, rooted at this manager's fallback
    /// `workspace_root`. Prefer [`get_server_for_path`] so the server is rooted
    /// at the project that actually contains the queried file.
    pub async fn get_or_start_server(
        &self,
        server_name: &str,
    ) -> Result<Arc<LspClient>, ManagerError> {
        let root = self.workspace_root.clone();
        self.get_or_start_server_in(server_name, &root).await
    }

    /// Get or start an LSP server by name for an explicit workspace root.
    /// Uses a global cache keyed by `(workspace_root, server_name)` so a
    /// separate LSP process is reused per project.
    async fn get_or_start_server_in(
        &self,
        server_name: &str,
        workspace_root: &Path,
    ) -> Result<Arc<LspClient>, ManagerError> {
        let cache_key = (workspace_root.to_path_buf(), server_name.to_string());

        // Check if server is already running in global cache
        {
            let cache = GLOBAL_LSP_CACHE.read().await;
            if let Some(client) = cache.get(&cache_key) {
                // Verify the process is still alive
                if client.is_alive() {
                    tracing::debug!(
                        server = server_name,
                        workspace = %workspace_root.display(),
                        "reusing cached language server"
                    );
                    return Ok(client.clone());
                }
                tracing::info!(
                    server = server_name,
                    "cached language server is no longer alive, restarting"
                );
            }
        }

        // Acquire write lock to start the server
        let mut cache = GLOBAL_LSP_CACHE.write().await;

        // Double-check after acquiring write lock (another task might have started it)
        if let Some(client) = cache.get(&cache_key) {
            if client.is_alive() {
                return Ok(client.clone());
            }
            // Remove dead client
            cache.remove(&cache_key);
        }

        let server_config = self
            .config
            .get_language_server(server_name)
            .ok_or_else(|| ManagerError::UnknownServer(server_name.to_string()))?;

        tracing::info!(
            server = server_name,
            workspace = %workspace_root.display(),
            "starting language server"
        );

        let client = LspClient::spawn(server_name.to_string(), server_config, workspace_root)
            .await
            .map_err(|source| ManagerError::Start {
                server: server_name.to_string(),
                source,
            })?;

        let client_arc = Arc::new(client);
        cache.insert(cache_key, client_arc.clone());

        tracing::info!(server = server_name, "language server started and cached");

        Ok(client_arc)
    }

    /// Resolve the workspace root for a given file using the language's root
    /// markers, falling back to the file's parent directory and finally this
    /// manager's launch root. The result is canonicalized for consistent cache
    /// keys.
    fn resolve_workspace_root(
        &self,
        file_path: &Path,
        language: &crate::config::LanguageConfig,
    ) -> PathBuf {
        let root = language
            .find_workspace_root(file_path)
            .or_else(|| file_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| self.workspace_root.clone());

        root.canonicalize().unwrap_or(root)
    }

    /// Get server for a specific file path based on extension. The LSP process
    /// is rooted at the project that contains `file_path`.
    pub async fn get_server_for_file(
        &self,
        file_path: &Path,
    ) -> Result<Arc<LspClient>, ManagerError> {
        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| ManagerError::NoExtension(file_path.to_path_buf()))?;

        let language = self
            .config
            .find_language_by_extension(extension)
            .ok_or_else(|| ManagerError::NoLanguageForExtension(extension.to_string()))?;

        let server_ref = primary_server(language)?;

        let workspace_root = self.resolve_workspace_root(file_path, language);
        self.get_or_start_server_in(server_ref.name(), &workspace_root)
            .await
    }

    /// Get server for a specific language by name (e.g., "rust", "python"),
    /// rooted at this manager's fallback `workspace_root`. Prefer
    /// [`get_server_for_path`] when a target file is known.
    pub async fn get_server_for_language(
        &self,
        language_id: &str,
    ) -> Result<Arc<LspClient>, ManagerError> {
        let server_name = self.primary_server_for_language(language_id)?;
        self.get_or_start_server(&server_name).await
    }

    /// Get server for a language, rooted at the project that contains
    /// `file_path` (resolved via the language's root markers).
    pub async fn get_server_for_path(
        &self,
        file_path: &Path,
        language_id: &str,
    ) -> Result<Arc<LspClient>, ManagerError> {
        let language = self.language(language_id)?;
        let server_ref = primary_server(language)?;

        let workspace_root = self.resolve_workspace_root(file_path, language);
        self.get_or_start_server_in(server_ref.name(), &workspace_root)
            .await
    }

    /// Find the primary language server name configured for a language.
    fn primary_server_for_language(&self, language_id: &str) -> Result<String, ManagerError> {
        let language = self.language(language_id)?;
        Ok(primary_server(language)?.name().to_string())
    }

    fn language(&self, language_id: &str) -> Result<&crate::config::LanguageConfig, ManagerError> {
        self.config
            .find_language_by_name(language_id)
            .ok_or_else(|| ManagerError::UnknownLanguage(language_id.to_string()))
    }

    /// Search symbols across all configured language servers
    /// Servers that cannot be started or queried are logged and skipped.
    pub async fn workspace_symbol_all(&self, query: &str) -> Vec<SymbolInformation> {
        let server_names = self.config.all_language_server_names();

        let mut all_symbols = Vec::new();

        // Query all servers
        for server_name in server_names {
            match self.get_or_start_server(&server_name).await {
                Ok(client) => match client.workspace_symbol(query).await {
                    Ok(symbols) => {
                        all_symbols.extend(symbols);
                    }
                    Err(e) => {
                        let error: &dyn std::error::Error = &e;
                        tracing::warn!(
                            server = %server_name,
                            error,
                            error_class = std::any::type_name_of_val(&e),
                            "workspace symbol query failed"
                        );
                    }
                },
                Err(e) => {
                    let error: &dyn std::error::Error = &e;
                    tracing::warn!(
                        server = %server_name,
                        error,
                        error_class = std::any::type_name_of_val(&e),
                        "language server could not be started"
                    );
                }
            }
        }

        all_symbols
    }

    /// Query a specific language server
    pub async fn workspace_symbol(
        &self,
        server_name: &str,
        query: &str,
    ) -> Result<Vec<SymbolInformation>, ManagerError> {
        let client = self.get_or_start_server(server_name).await?;
        client
            .workspace_symbol(query)
            .await
            .map_err(|source| ManagerError::Request {
                server: server_name.to_string(),
                source,
            })
    }

    /// Get the workspace root path
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get the configuration
    pub fn config(&self) -> &LanguagesConfig {
        &self.config
    }
}

/// Clear the global LSP cache. Useful for testing.
/// This will cause all cached LSP processes to be dropped.
pub async fn clear_global_cache() {
    let mut cache = GLOBAL_LSP_CACHE.write().await;
    cache.clear();
    tracing::info!("global LSP cache cleared");
}

fn primary_server(
    language: &crate::config::LanguageConfig,
) -> Result<&crate::config::LanguageServerRef, ManagerError> {
    language
        .language_servers
        .first()
        .ok_or_else(|| ManagerError::NoServerForLanguage(language.name.clone()))
}
