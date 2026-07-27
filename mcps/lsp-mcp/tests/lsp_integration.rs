//! Integration tests for LSP process caching and functionality
//!
//! These tests use the lsp-mcp codebase itself as the test workspace.
//! Requires rust-analyzer to be installed.
//!
//! **Shared runtime + shared cache:** all tests run on a single process-wide
//! tokio runtime (see [`block_on`]) and deliberately do *not* clear the global
//! LSP cache between tests. The first test that needs rust-analyzer spawns and
//! indexes it once; every later test reuses that warm process. The cached
//! `LspClient` owns a `tokio::process::Child` bound to the runtime that spawned
//! it, so sharing the cache only works because the runtime is shared too.
//!
//! **IMPORTANT**: run with `--test-threads=1`. The warm client is reused across
//! tests and its request/response stream is not safe for concurrent callers:
//! ```
//! cargo test --test lsp_integration -- --test-threads=1
//! ```

use hobby_lsp_mcp::config::{
    FileType, LanguageConfig, LanguageServerConfig, LanguageServerRef, LanguagesConfig, RootMarker,
};
use hobby_lsp_mcp::lsp::LspManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

/// Process-wide tokio runtime shared by every test. Cached LSP processes live
/// on this runtime, so reusing them across tests requires one shared runtime
/// (a per-test `#[tokio::test]` runtime would be dropped, killing the process).
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build shared tokio runtime")
    })
}

/// Run an async test body on the shared runtime.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    runtime().block_on(fut)
}

/// Get the workspace root (this project's directory)
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Create a test config with rust-analyzer
fn create_rust_analyzer_config() -> LanguagesConfig {
    let mut language_servers = HashMap::new();
    language_servers.insert(
        "rust-analyzer".to_string(),
        LanguageServerConfig {
            command: "rust-analyzer".to_string(),
            args: vec![],
            config: None,
            environment: HashMap::new(),
            timeout: Some(30000),
        },
    );

    let language = vec![LanguageConfig {
        name: "rust".to_string(),
        scope: "source.rust".to_string(),
        file_types: vec![FileType::Extension("rs".to_string())],
        language_servers: vec![LanguageServerRef::String("rust-analyzer".to_string())],
        roots: vec![RootMarker::String("Cargo.toml".to_string())],
        auto_format: false,
        language_id: None,
        shebangs: vec![],
        comment_token: None,
        indent: None,
        text_width: None,
        formatter: None,
    }];

    LanguagesConfig {
        language_servers,
        language,
    }
}

/// Check if rust-analyzer is available
fn rust_analyzer_available() -> bool {
    std::process::Command::new("rust-analyzer")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build a manager and ensure its rust-analyzer is started. `get_or_start_server`
/// now blocks until indexing settles, so the returned manager is ready to query
/// — no polling needed. The underlying process is cached globally, so repeated
/// calls with the same workspace reuse the already-warm server.
async fn create_warmed_up_manager(config: LanguagesConfig, workspace: PathBuf) -> Arc<LspManager> {
    let manager = Arc::new(LspManager::new(config, workspace));
    let _ = manager
        .get_or_start_server("rust-analyzer")
        .await
        .expect("Failed to start rust-analyzer");
    manager
}

#[test]
fn test_lsp_client_caching_same_workspace() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();

        // Create first manager
        let manager1 = Arc::new(LspManager::new(config.clone(), workspace.clone()));

        // Get the server - spawns a new process or reuses the cached one
        let client1 = manager1.get_or_start_server("rust-analyzer").await;
        assert!(
            client1.is_ok(),
            "Failed to start rust-analyzer: {:?}",
            client1.err()
        );
        let client1 = client1.unwrap();

        // Verify the client is alive
        assert!(client1.is_alive().await, "Client should be alive after spawn");

        // Create second manager for same workspace
        let manager2 = Arc::new(LspManager::new(config.clone(), workspace.clone()));

        // Get the server again - should reuse the cached process
        let client2 = manager2.get_or_start_server("rust-analyzer").await;
        assert!(client2.is_ok(), "Failed to get cached rust-analyzer");
        let client2 = client2.unwrap();

        // Both should point to the same Arc (same process)
        assert!(
            Arc::ptr_eq(&client1, &client2),
            "Should reuse the same LSP client for same workspace"
        );
    });
}

#[test]
fn test_lsp_client_isolation_different_workspaces() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();

        // Use two different paths (even if one doesn't exist, canonicalization will differ)
        let workspace1 = workspace_root();
        let workspace2 = workspace_root().join("src"); // Different canonical path

        let manager1 = Arc::new(LspManager::new(config.clone(), workspace1));
        let manager2 = Arc::new(LspManager::new(config.clone(), workspace2));

        let client1 = manager1.get_or_start_server("rust-analyzer").await;
        assert!(
            client1.is_ok(),
            "Failed to start rust-analyzer for workspace1"
        );
        let client1 = client1.unwrap();

        let client2 = manager2.get_or_start_server("rust-analyzer").await;
        assert!(
            client2.is_ok(),
            "Failed to start rust-analyzer for workspace2"
        );
        let client2 = client2.unwrap();

        // Should be different processes for different workspaces
        assert!(
            !Arc::ptr_eq(&client1, &client2),
            "Should have different LSP clients for different workspaces"
        );

        // Both should be alive
        assert!(client1.is_alive().await, "Client1 should be alive");
        assert!(client2.is_alive().await, "Client2 should be alive");
    });
}

#[test]
fn test_workspace_symbol_search() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = create_warmed_up_manager(config, workspace).await;

        // Search for "LspManager" - should find our struct
        let symbols = manager
            .workspace_symbol("rust-analyzer", "LspManager")
            .await;
        assert!(symbols.is_ok(), "workspace_symbol failed: {:?}", symbols.err());

        let symbols = symbols.unwrap();
        assert!(
            !symbols.is_empty(),
            "Should find LspManager symbol in workspace"
        );

        // Verify we found the right symbol
        let found = symbols.iter().any(|s| s.name.contains("LspManager"));
        assert!(found, "Should find a symbol containing 'LspManager'");
    });
}

#[test]
fn test_workspace_symbol_search_function() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = create_warmed_up_manager(config, workspace).await;

        // Search for "get_or_start_server" function
        let symbols = manager
            .workspace_symbol("rust-analyzer", "get_or_start_server")
            .await;
        assert!(symbols.is_ok(), "workspace_symbol failed: {:?}", symbols.err());

        let symbols = symbols.unwrap();
        assert!(
            !symbols.is_empty(),
            "Should find get_or_start_server function"
        );
    });
}

#[test]
fn test_document_symbol() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = create_warmed_up_manager(config, workspace.clone()).await;

        // Get symbols from a specific file
        let file_path = workspace.join("src/lsp/manager.rs");
        let client = manager.get_server_for_file(&file_path).await;
        assert!(
            client.is_ok(),
            "Failed to get server for file: {:?}",
            client.err()
        );

        let client = client.unwrap();
        let uri = lsp_types::Url::from_file_path(&file_path).unwrap();
        let symbols = client.document_symbol(&uri).await;

        assert!(symbols.is_ok(), "document_symbol failed: {:?}", symbols.err());

        let symbols = symbols.unwrap();
        assert!(symbols.is_some(), "Should return symbols for manager.rs");
    });
}

#[test]
fn test_get_server_for_file_by_extension() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = Arc::new(LspManager::new(config, workspace.clone()));

        // Get server for a .rs file
        let file_path = workspace.join("src/lib.rs");
        let client = manager.get_server_for_file(&file_path).await;
        assert!(client.is_ok(), "Should find rust-analyzer for .rs file");

        let client = client.unwrap();
        assert_eq!(client.server_name(), "rust-analyzer");
    });
}

#[test]
fn test_workspace_symbol_all() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = create_warmed_up_manager(config, workspace).await;

        // Search across all language servers
        let symbols = manager.workspace_symbol_all("LspClient").await;
        assert!(
            symbols.is_ok(),
            "workspace_symbol_all failed: {:?}",
            symbols.err()
        );

        let symbols = symbols.unwrap();
        assert!(
            !symbols.is_empty(),
            "Should find LspClient symbol across all servers"
        );
    });
}

#[test]
fn test_lsp_client_is_alive() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = Arc::new(LspManager::new(config, workspace));

        let client = manager.get_or_start_server("rust-analyzer").await;
        assert!(client.is_ok(), "Failed to start rust-analyzer");

        let client = client.unwrap();

        // Client should be alive
        assert!(
            client.is_alive().await,
            "Freshly spawned client should be alive"
        );

        // Make a request to verify it's working
        let symbols = client.workspace_symbol("test").await;
        assert!(
            symbols.is_ok(),
            "Should be able to make requests to live client"
        );
    });
}

#[test]
fn test_multiple_sequential_requests() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = Arc::new(LspManager::new(config, workspace));

        // Make multiple requests sequentially
        for query in &["LspManager", "LspClient", "LanguagesConfig", "Result"] {
            let symbols = manager.workspace_symbol("rust-analyzer", query).await;
            assert!(
                symbols.is_ok(),
                "workspace_symbol failed for '{}': {:?}",
                query,
                symbols.err()
            );
        }
    });
}

#[test]
fn test_concurrent_requests() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = Arc::new(LspManager::new(config, workspace));

        // First ensure the server is started
        let _ = manager.get_or_start_server("rust-analyzer").await;

        // Spawn multiple concurrent requests
        let manager1 = manager.clone();
        let manager2 = manager.clone();
        let manager3 = manager.clone();

        let (r1, r2, r3) = tokio::join!(
            manager1.workspace_symbol("rust-analyzer", "LspManager"),
            manager2.workspace_symbol("rust-analyzer", "LspClient"),
            manager3.workspace_symbol("rust-analyzer", "Config"),
        );

        assert!(r1.is_ok(), "Concurrent request 1 failed: {:?}", r1.err());
        assert!(r2.is_ok(), "Concurrent request 2 failed: {:?}", r2.err());
        assert!(r3.is_ok(), "Concurrent request 3 failed: {:?}", r3.err());
    });
}

#[test]
fn test_nonexistent_server() {
    block_on(async {
        let mut config = create_rust_analyzer_config();
        // Don't add any language servers
        config.language_servers.clear();

        let workspace = workspace_root();
        let manager = Arc::new(LspManager::new(config, workspace));

        let result = manager.get_or_start_server("nonexistent-server").await;
        assert!(result.is_err(), "Should fail for nonexistent server");
    });
}

#[test]
fn test_file_without_configured_language() {
    block_on(async {
        let config = create_rust_analyzer_config();
        let workspace = workspace_root();
        let manager = Arc::new(LspManager::new(config, workspace.clone()));

        // Try to get server for a file type that's not configured
        let file_path = workspace.join("README.md");
        let result = manager.get_server_for_file(&file_path).await;
        assert!(result.is_err(), "Should fail for unconfigured file type");
    });
}

/// Regression test for the production failure: the MCP is launched from a
/// directory that is NOT a cargo project, so a server rooted there indexes
/// nothing. With per-file root resolution, querying a file inside a real
/// project must still root the server at that project and return results —
/// without the manual warm-up polling the other tests rely on (the spawn now
/// waits for indexing internally).
#[test]
fn test_wrong_launch_root_resolves_per_file() {
    block_on(async {
        if !rust_analyzer_available() {
            eprintln!("Skipping test: rust-analyzer not available");
            return;
        }

        // Launch root deliberately has no Cargo.toml (reproduces the bug).
        let bogus_root = std::env::temp_dir();
        let config = create_rust_analyzer_config();
        let manager = Arc::new(LspManager::new(config, bogus_root));

        // Query a real file inside THIS crate. The server must be rooted at the
        // crate's Cargo.toml directory via root markers, not the launch directory.
        let file_path = workspace_root().join("src/lsp/manager.rs");
        let client = manager
            .get_server_for_path(&file_path, "rust")
            .await
            .expect("should resolve a server rooted at the file's project");

        // document_symbol must work after did_open (mirrors the fixed tool flow).
        let uri = lsp_types::Url::from_file_path(&file_path).unwrap();
        let contents = std::fs::read_to_string(&file_path).unwrap();
        client.did_open(&uri, "rust", &contents).await.unwrap();

        let symbols = client.document_symbol(&uri).await;
        assert!(symbols.is_ok(), "document_symbol failed: {:?}", symbols.err());
        assert!(
            symbols.unwrap().is_some(),
            "expected document symbols for manager.rs"
        );

        // workspace_symbol must find symbols from the correct project, proving the
        // server indexed the right cargo workspace.
        let ws = client.workspace_symbol("LspManager").await;
        assert!(ws.is_ok(), "workspace_symbol failed: {:?}", ws.err());
        assert!(
            !ws.unwrap().is_empty(),
            "expected non-empty workspace symbols for LspManager"
        );
    });
}
