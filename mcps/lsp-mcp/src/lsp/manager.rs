use super::client::LspClient;
use crate::config::LanguagesConfig;
use crate::error::{LspMcpError, Result};
use lsp_types::SymbolInformation;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global cache for LSP clients, keyed by (canonical workspace path, server name)
/// This ensures LSP processes are reused across LspManager instances for the same project
static GLOBAL_LSP_CACHE: Lazy<RwLock<HashMap<(PathBuf, String), Arc<LspClient>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

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
        let canonical_root = workspace_root
            .canonicalize()
            .unwrap_or(workspace_root);

        Self {
            config,
            workspace_root: canonical_root,
        }
    }

    /// Get or start an LSP server by name, rooted at this manager's fallback
    /// `workspace_root`. Prefer [`get_server_for_path`] so the server is rooted
    /// at the project that actually contains the queried file.
    pub async fn get_or_start_server(&self, server_name: &str) -> Result<Arc<LspClient>> {
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
    ) -> Result<Arc<LspClient>> {
        let cache_key = (workspace_root.to_path_buf(), server_name.to_string());

        // Check if server is already running in global cache
        {
            let cache = GLOBAL_LSP_CACHE.read().await;
            if let Some(client) = cache.get(&cache_key) {
                // Verify the process is still alive
                if client.is_alive().await {
                    eprintln!(
                        "[lsp-mcp] Reusing cached LSP server '{}' for workspace '{}'",
                        server_name,
                        workspace_root.display()
                    );
                    return Ok(client.clone());
                }
                // Process is dead, will be replaced below
                eprintln!(
                    "[lsp-mcp] Cached LSP server '{}' is no longer alive, restarting",
                    server_name
                );
            }
        }

        // Acquire write lock to start the server
        let mut cache = GLOBAL_LSP_CACHE.write().await;

        // Double-check after acquiring write lock (another task might have started it)
        if let Some(client) = cache.get(&cache_key) {
            if client.is_alive().await {
                return Ok(client.clone());
            }
            // Remove dead client
            cache.remove(&cache_key);
        }

        let server_config = self
            .config
            .get_language_server(server_name)
            .ok_or_else(|| {
                LspMcpError::ConfigError(format!("Language server not found: {}", server_name))
            })?;

        eprintln!(
            "[lsp-mcp] Starting LSP server '{}' for workspace '{}'",
            server_name,
            workspace_root.display()
        );

        let client = LspClient::spawn(
            server_name.to_string(),
            server_config,
            workspace_root,
        )
        .await?;

        let client_arc = Arc::new(client);
        cache.insert(cache_key, client_arc.clone());

        eprintln!(
            "[lsp-mcp] LSP server '{}' started and cached successfully",
            server_name
        );

        Ok(client_arc)
    }

    /// Resolve the workspace root for a given file using the language's root
    /// markers, falling back to the file's parent directory and finally this
    /// manager's launch root. The result is canonicalized for consistent cache
    /// keys.
    fn resolve_workspace_root(&self, file_path: &Path, language: &crate::config::LanguageConfig) -> PathBuf {
        let root = language
            .find_workspace_root(file_path)
            .or_else(|| file_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| self.workspace_root.clone());

        root.canonicalize().unwrap_or(root)
    }

    /// Get server for a specific file path based on extension. The LSP process
    /// is rooted at the project that contains `file_path`.
    pub async fn get_server_for_file(&self, file_path: &Path) -> Result<Arc<LspClient>> {
        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| LspMcpError::InvalidUri("No file extension".to_string()))?;

        // Find language by extension
        let language = self
            .config
            .find_language_by_extension(extension)
            .ok_or_else(|| {
                LspMcpError::ConfigError(format!("No language configured for extension: {}", extension))
            })?;

        // Get primary language server
        let server_ref = language.language_servers.first().ok_or_else(|| {
            LspMcpError::ConfigError(format!(
                "No language server configured for {}",
                language.name
            ))
        })?;

        let workspace_root = self.resolve_workspace_root(file_path, language);
        self.get_or_start_server_in(server_ref.name(), &workspace_root).await
    }

    /// Get server for a specific language by name (e.g., "rust", "python"),
    /// rooted at this manager's fallback `workspace_root`. Prefer
    /// [`get_server_for_path`] when a target file is known.
    pub async fn get_server_for_language(&self, language_id: &str) -> Result<Arc<LspClient>> {
        let server_name = self.primary_server_for_language(language_id)?;
        self.get_or_start_server(&server_name).await
    }

    /// Get server for a language, rooted at the project that contains
    /// `file_path` (resolved via the language's root markers).
    pub async fn get_server_for_path(
        &self,
        file_path: &Path,
        language_id: &str,
    ) -> Result<Arc<LspClient>> {
        let language = self
            .config
            .find_language_by_name(language_id)
            .ok_or_else(|| {
                LspMcpError::ConfigError(format!("No language configured with name: {}", language_id))
            })?;

        let server_ref = language.language_servers.first().ok_or_else(|| {
            LspMcpError::ConfigError(format!(
                "No language server configured for {}",
                language.name
            ))
        })?;

        let workspace_root = self.resolve_workspace_root(file_path, language);
        self.get_or_start_server_in(server_ref.name(), &workspace_root).await
    }

    /// Find the primary language server name configured for a language.
    fn primary_server_for_language(&self, language_id: &str) -> Result<String> {
        let language = self
            .config
            .find_language_by_name(language_id)
            .ok_or_else(|| {
                LspMcpError::ConfigError(format!("No language configured with name: {}", language_id))
            })?;

        let server_ref = language.language_servers.first().ok_or_else(|| {
            LspMcpError::ConfigError(format!(
                "No language server configured for {}",
                language.name
            ))
        })?;

        Ok(server_ref.name().to_string())
    }

    /// Search symbols across all configured language servers
    pub async fn workspace_symbol_all(&self, query: &str) -> Result<Vec<SymbolInformation>> {
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
                        eprintln!("Error querying server {}: {}", server_name, e);
                        // Continue with other servers
                    }
                },
                Err(e) => {
                    eprintln!("Error starting server {}: {}", server_name, e);
                    // Continue with other servers
                }
            }
        }

        Ok(all_symbols)
    }

    /// Query a specific language server
    pub async fn workspace_symbol(&self, server_name: &str, query: &str) -> Result<Vec<SymbolInformation>> {
        let client = self.get_or_start_server(server_name).await?;
        client.workspace_symbol(query).await
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
    eprintln!("[lsp-mcp] Global LSP cache cleared");
}
