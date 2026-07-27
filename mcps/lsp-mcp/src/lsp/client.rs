use crate::config::LanguageServerConfig;
use crate::error::{LspMcpError, Result};
use lsp_types::*;
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use std::sync::Arc;

/// Maximum time to wait for an LSP server to finish its initial indexing.
const MAX_INDEXING_WAIT: Duration = Duration::from_secs(60);

/// Once the server has started reporting progress, this much silence means it
/// has settled and we stop waiting.
const INDEXING_IDLE_TIMEOUT: Duration = Duration::from_secs(2);

/// How long to wait for the *first* sign of progress activity before assuming
/// the server does not report indexing progress at all (e.g. a lightweight LSP
/// with nothing to index). Prevents exiting during the initial silent gap a
/// server like rust-analyzer spends on `cargo metadata` before its first
/// `$/progress` message.
const NO_PROGRESS_GRACE: Duration = Duration::from_secs(10);

/// True if `value` is any `$/progress` notification — i.e. the server has
/// started reporting work-done progress and is therefore actively analyzing.
fn is_progress(value: &serde_json::Value) -> bool {
    value.get("method").and_then(|m| m.as_str()) == Some("$/progress")
}

/// True if `value` is a `$/progress` notification reporting the *end* of an
/// indexing / cache-priming work-done token (rust-analyzer style). Used as the
/// fast-path signal that a server has finished its initial analysis.
fn is_indexing_end(value: &serde_json::Value) -> bool {
    if value.get("method").and_then(|m| m.as_str()) != Some("$/progress") {
        return false;
    }
    let params = match value.get("params") {
        Some(p) => p,
        None => return false,
    };
    let kind = params
        .get("value")
        .and_then(|v| v.get("kind"))
        .and_then(|k| k.as_str());
    if kind != Some("end") {
        return false;
    }
    let token = params
        .get("token")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_lowercase();
    token.contains("index") || token.contains("cacheprim")
}

/// A client for communicating with an LSP server process
pub struct LspClient {
    _process: Arc<Mutex<tokio::process::Child>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    stdout_reader: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    next_id: Arc<Mutex<i32>>,
    server_name: String,
    diagnostics: Arc<Mutex<HashMap<Url, Vec<Diagnostic>>>>,
    /// Serializes whole request/response cycles. The read loop consumes
    /// messages until it sees its own response id, so two requests in flight at
    /// once would steal and discard each other's responses and then block
    /// forever. Holding this for the duration of a request makes a single
    /// client safe to share across concurrent callers (overlapping async MCP
    /// tool calls hit the same cached client).
    request_lock: Arc<Mutex<()>>,
}

impl LspClient {
    /// Spawn a new LSP server process and create a client
    pub async fn spawn(
        server_name: String,
        config: &LanguageServerConfig,
        workspace_root: &std::path::Path,
    ) -> Result<Self> {
        // Spawn the LSP server process
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| LspMcpError::LspRequestFailed(format!("Failed to spawn {}: {}", config.command, e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspMcpError::LspRequestFailed("Failed to get stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspMcpError::LspRequestFailed("Failed to get stdout".to_string()))?;

        let stdout_reader = BufReader::new(stdout);

        let mut client = Self {
            _process: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout_reader: Arc::new(Mutex::new(stdout_reader)),
            next_id: Arc::new(Mutex::new(1)),
            server_name,
            diagnostics: Arc::new(Mutex::new(HashMap::new())),
            request_lock: Arc::new(Mutex::new(())),
        };

        // Initialize the server
        client.initialize(workspace_root).await?;

        Ok(client)
    }

    /// Initialize the LSP server
    async fn initialize(&mut self, workspace_root: &std::path::Path) -> Result<()> {
        let root_uri = Url::from_file_path(workspace_root)
            .map_err(|_| LspMcpError::InvalidUri(workspace_root.display().to_string()))?;

        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(root_uri.clone()),
            root_path: None, // Deprecated
            initialization_options: None,
            capabilities: ClientCapabilities {
                workspace: Some(WorkspaceClientCapabilities {
                    workspace_folders: Some(true),
                    symbol: Some(WorkspaceSymbolClientCapabilities {
                        dynamic_registration: Some(false),
                        symbol_kind: Some(SymbolKindCapability {
                            value_set: Some(vec![
                                SymbolKind::FILE,
                                SymbolKind::MODULE,
                                SymbolKind::NAMESPACE,
                                SymbolKind::PACKAGE,
                                SymbolKind::CLASS,
                                SymbolKind::METHOD,
                                SymbolKind::PROPERTY,
                                SymbolKind::FIELD,
                                SymbolKind::CONSTRUCTOR,
                                SymbolKind::ENUM,
                                SymbolKind::INTERFACE,
                                SymbolKind::FUNCTION,
                                SymbolKind::VARIABLE,
                                SymbolKind::CONSTANT,
                                SymbolKind::STRING,
                                SymbolKind::NUMBER,
                                SymbolKind::BOOLEAN,
                                SymbolKind::ARRAY,
                                SymbolKind::OBJECT,
                                SymbolKind::KEY,
                                SymbolKind::NULL,
                                SymbolKind::ENUM_MEMBER,
                                SymbolKind::STRUCT,
                                SymbolKind::EVENT,
                                SymbolKind::OPERATOR,
                                SymbolKind::TYPE_PARAMETER,
                            ]),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: "workspace".to_string(),
            }]),
            ..Default::default()
        };

        // Send initialize request
        let _init_result: InitializeResult = self.send_request("initialize", init_params).await?;

        // Send initialized notification
        self.send_notification("initialized", InitializedParams {}).await?;

        // Wait for the server to finish its initial indexing/cache priming.
        // Symbol search and diagnostics return empty until this completes, so
        // blocking here (once, at spawn) prevents the common "0 results"
        // failure on the first query against a freshly started server.
        self.wait_until_indexed().await;

        Ok(())
    }

    /// Drain server-initiated messages until the server finishes indexing or
    /// goes idle. Replies to server→client requests (e.g. `workspace/configuration`,
    /// `window/workDoneProgress/create`) so the server is not left blocked, and
    /// watches `$/progress` to detect when indexing/cache-priming ends.
    ///
    /// Best-effort: never errors. Bounded by [`MAX_INDEXING_WAIT`]; also returns
    /// early once the server has been quiet for [`INDEXING_IDLE_TIMEOUT`].
    async fn wait_until_indexed(&self) {
        let start = tokio::time::Instant::now();
        let mut saw_progress = false;

        loop {
            if start.elapsed() >= MAX_INDEXING_WAIT {
                eprintln!(
                    "[lsp-mcp] '{}' indexing wait hit the {}s cap; proceeding",
                    self.server_name,
                    MAX_INDEXING_WAIT.as_secs()
                );
                break;
            }

            match tokio::time::timeout(INDEXING_IDLE_TIMEOUT, self.read_message()).await {
                Ok(Ok(raw)) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                        // Reply to requests / store diagnostics as needed.
                        let _ = self.handle_server_message(&value).await;

                        if is_progress(&value) {
                            saw_progress = true;
                        }

                        // Stop as soon as a rust-analyzer-style indexing token ends.
                        if is_indexing_end(&value) {
                            eprintln!(
                                "[lsp-mcp] '{}' reported indexing complete",
                                self.server_name
                            );
                            break;
                        }
                    }
                }
                Ok(Err(_)) => break, // read error (e.g. server closed stdout)
                Err(_) => {
                    // Idle window elapsed with no message.
                    if saw_progress {
                        // The server was actively reporting progress and has now
                        // gone quiet → indexing has settled.
                        break;
                    }
                    if start.elapsed() >= NO_PROGRESS_GRACE {
                        // No progress was ever reported within the grace window →
                        // assume this server does not report indexing progress.
                        break;
                    }
                    // Otherwise keep waiting: the server may still be in its
                    // initial silent phase (e.g. `cargo metadata`) before it
                    // starts reporting progress.
                }
            }
        }
    }

    /// Handle a server-initiated message (one carrying a `method`): a
    /// notification (no `id`) or a server→client request (has `id`).
    /// Stores `publishDiagnostics` and replies to requests so the server does
    /// not stall waiting on us. Returns without error for unrecognized methods.
    async fn handle_server_message(&self, value: &serde_json::Value) -> Result<()> {
        let method = value.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // Capture pushed diagnostics regardless of request/notification.
        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = value.get("params") {
                if let Ok(diag_params) =
                    serde_json::from_value::<PublishDiagnosticsParams>(params.clone())
                {
                    let mut diagnostics = self.diagnostics.lock().await;
                    diagnostics.insert(diag_params.uri, diag_params.diagnostics);
                }
            }
        }

        // A server→client request carries an `id` and must be answered.
        if let Some(id) = value.get("id") {
            let result = match method {
                // One config value per requested item; we have no overrides, so null.
                "workspace/configuration" => {
                    let n = value
                        .get("params")
                        .and_then(|p| p.get("items"))
                        .and_then(|i| i.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    serde_json::Value::Array(vec![serde_json::Value::Null; n])
                }
                // Acknowledge capability registration, progress creation, refreshes, etc.
                _ => serde_json::Value::Null,
            };
            self.send_response(id, result).await?;
        }

        Ok(())
    }

    /// Send a JSON-RPC response to a server-initiated request.
    async fn send_response(&self, id: &serde_json::Value, result: serde_json::Value) -> Result<()> {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });

        let response_str = serde_json::to_string(&response).map_err(LspMcpError::JsonError)?;
        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            response_str.len(),
            response_str
        );

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(message.as_bytes())
            .await
            .map_err(LspMcpError::IoError)?;
        stdin.flush().await.map_err(LspMcpError::IoError)?;

        Ok(())
    }

    /// Send textDocument/didOpen notification
    pub async fn did_open(&self, uri: &Url, language_id: &str, text: &str) -> Result<()> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: language_id.to_string(),
                version: 1,
                text: text.to_string(),
            },
        };

        self.send_notification("textDocument/didOpen", params).await
    }

    /// Send workspace/symbol request
    pub async fn workspace_symbol(&self, query: &str) -> Result<Vec<SymbolInformation>> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        // WorkspaceSymbolResponse can be either Flat(Vec<SymbolInformation>) or Nested(Vec<WorkspaceSymbol>)
        let response: Option<WorkspaceSymbolResponse> = self.send_request("workspace/symbol", params).await?;

        let symbols = match response {
            Some(WorkspaceSymbolResponse::Flat(symbols)) => symbols,
            Some(WorkspaceSymbolResponse::Nested(workspace_symbols)) => {
                // Convert WorkspaceSymbol to SymbolInformation
                workspace_symbols
                    .into_iter()
                    .filter_map(|ws| {
                        // WorkspaceSymbol.location can be either Location or just { uri }
                        let location = match ws.location {
                            OneOf::Left(loc) => loc,
                            OneOf::Right(ws_loc) => Location {
                                uri: ws_loc.uri,
                                range: lsp_types::Range::default(), // No range available
                            },
                        };
                        Some(SymbolInformation {
                            name: ws.name,
                            kind: ws.kind,
                            tags: ws.tags,
                            deprecated: None,
                            location,
                            container_name: ws.container_name,
                        })
                    })
                    .collect()
            }
            None => vec![],
        };

        Ok(symbols)
    }

    /// Send textDocument/documentSymbol request
    pub async fn document_symbol(&self, uri: &Url) -> Result<Option<DocumentSymbolResponse>> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response: Option<DocumentSymbolResponse> = self.send_request("textDocument/documentSymbol", params).await?;

        Ok(response)
    }

    /// Send textDocument/definition request
    pub async fn goto_definition(&self, uri: &Url, position: Position) -> Result<Option<GotoDefinitionResponse>> {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response: Option<GotoDefinitionResponse> = self.send_request("textDocument/definition", params).await?;

        Ok(response)
    }

    /// Send textDocument/references request
    pub async fn find_references(&self, uri: &Url, position: Position, include_declaration: bool) -> Result<Option<Vec<Location>>> {
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration,
            },
        };

        let response: Option<Vec<Location>> = self.send_request("textDocument/references", params).await?;

        Ok(response)
    }

    /// Send textDocument/hover request
    pub async fn hover(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<Hover>> {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        self.send_request("textDocument/hover", params).await
    }

    /// Send textDocument/completion request
    pub async fn completion(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<CompletionResponse>> {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: Some(CompletionContext {
                trigger_kind: CompletionTriggerKind::INVOKED,
                trigger_character: None,
            }),
        };

        self.send_request("textDocument/completion", params).await
    }

    /// Pull diagnostics for a document using textDocument/diagnostic (LSP 3.17+)
    pub async fn pull_diagnostics(&self, uri: &Url) -> Result<Vec<Diagnostic>> {
        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response: DocumentDiagnosticReportResult =
            self.send_request("textDocument/diagnostic", params).await?;

        let diagnostics = match response {
            DocumentDiagnosticReportResult::Report(report) => match report {
                DocumentDiagnosticReport::Full(full) => {
                    full.full_document_diagnostic_report.items
                }
                DocumentDiagnosticReport::Unchanged(_) => vec![],
            },
            DocumentDiagnosticReportResult::Partial(_) => vec![],
        };

        Ok(diagnostics)
    }

    /// Get stored diagnostics for a URI (from publishDiagnostics notifications)
    pub async fn get_stored_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let diagnostics = self.diagnostics.lock().await;
        diagnostics.get(uri).cloned().unwrap_or_default()
    }

    /// Send textDocument/codeAction request
    pub async fn code_action(
        &self,
        uri: &Url,
        range: Range,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Option<CodeActionResponse>> {
        let params = CodeActionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range,
            context: CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        self.send_request("textDocument/codeAction", params).await
    }

    /// Send a JSON-RPC request and wait for response
    async fn send_request<P: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R> {
        // Serialize the whole request/response cycle: the read loop below
        // consumes messages until it finds this request's id, so two concurrent
        // requests on the same client would discard each other's responses.
        let _req_guard = self.request_lock.lock().await;

        let id = {
            let mut next_id = self.next_id.lock().await;
            let id = *next_id;
            *next_id += 1;
            id
        };

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let request_str = serde_json::to_string(&request)
            .map_err(|e| LspMcpError::JsonError(e))?;

        let message = format!("Content-Length: {}\r\n\r\n{}", request_str.len(), request_str);

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(message.as_bytes())
                .await
                .map_err(|e| LspMcpError::IoError(e))?;
            stdin.flush().await.map_err(|e| LspMcpError::IoError(e))?;
        }

        // Read messages until we get the response with matching ID. The server
        // can interleave notifications (progress, diagnostics) and its own
        // requests (workspace/configuration, etc.) at any time.
        loop {
            let response_str = self.read_message().await?;
            let response: serde_json::Value = serde_json::from_str(&response_str)
                .map_err(|e| LspMcpError::JsonError(e))?;

            // Any message carrying a `method` is server-initiated: either a
            // notification (no `id`) or a server→client request (with an `id`
            // in the *server's* id namespace, which can collide with ours).
            // Handle it (store diagnostics / reply to requests) and keep reading.
            if response.get("method").is_some() {
                self.handle_server_message(&response).await?;
                continue;
            }

            // Otherwise this is a response. It must match our request id.
            if response.get("id") != Some(&serde_json::json!(id)) {
                eprintln!(
                    "[lsp-mcp] Skipping response with mismatched id: expected {}, got {:?}",
                    id,
                    response.get("id")
                );
                continue;
            }

            // Check for error
            if let Some(error) = response.get("error") {
                return Err(LspMcpError::LspRequestFailed(format!(
                    "LSP error: {}",
                    error
                )));
            }

            // Extract result
            let result = response
                .get("result")
                .ok_or_else(|| LspMcpError::LspRequestFailed("No result in response".to_string()))?;

            return serde_json::from_value(result.clone()).map_err(|e| LspMcpError::JsonError(e));
        }
    }

    /// Send a JSON-RPC notification (no response expected)
    async fn send_notification<P: serde::Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let notification_str = serde_json::to_string(&notification)
            .map_err(|e| LspMcpError::JsonError(e))?;

        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification_str.len(),
            notification_str
        );

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(message.as_bytes())
            .await
            .map_err(|e| LspMcpError::IoError(e))?;
        stdin.flush().await.map_err(|e| LspMcpError::IoError(e))?;

        Ok(())
    }

    /// Read a single LSP message from stdout
    async fn read_message(&self) -> Result<String> {
        let mut reader = self.stdout_reader.lock().await;

        // Read headers
        let mut content_length: Option<usize> = None;
        let mut line = String::new();

        loop {
            line.clear();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| LspMcpError::IoError(e))?;

            if line == "\r\n" {
                // End of headers
                break;
            }

            if line.starts_with("Content-Length: ") {
                let len_str = line.trim_start_matches("Content-Length: ").trim();
                content_length = Some(
                    len_str
                        .parse()
                        .map_err(|_| LspMcpError::LspRequestFailed("Invalid Content-Length".to_string()))?,
                );
            }
        }

        let content_length = content_length
            .ok_or_else(|| LspMcpError::LspRequestFailed("Missing Content-Length header".to_string()))?;

        // Read content
        let mut buffer = vec![0u8; content_length];
        use tokio::io::AsyncReadExt;
        reader
            .read_exact(&mut buffer)
            .await
            .map_err(|e| LspMcpError::IoError(e))?;

        String::from_utf8(buffer)
            .map_err(|e| LspMcpError::LspRequestFailed(format!("Invalid UTF-8: {}", e)))
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Check if the LSP server process is still alive
    pub async fn is_alive(&self) -> bool {
        let mut process = self._process.lock().await;
        // try_wait returns Ok(Some(status)) if exited, Ok(None) if still running, Err on error
        match process.try_wait() {
            Ok(None) => true,  // Process is still running
            Ok(Some(_)) => false,  // Process has exited
            Err(_) => false,  // Error checking status, assume dead
        }
    }
}
