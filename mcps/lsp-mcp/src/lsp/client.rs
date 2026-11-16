//! A JSON-RPC client for one language server process: spawning and
//! initializing it, sending requests and notifications, and keeping the
//! diagnostics it publishes.

use crate::config::LanguageServerConfig;
use lsp_types::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

/// Longest a single request may take before the server is considered hung.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Talking to the language server failed.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("failed to spawn {command}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("workspace root is not an absolute path: {}", .0.display())]
    InvalidRoot(PathBuf),
    #[error("cannot write to the language server")]
    Write(#[source] std::io::Error),
    #[error("cannot read from the language server")]
    Read(#[source] std::io::Error),
    #[error("message from the language server has no Content-Length header")]
    MissingContentLength,
    #[error("invalid Content-Length header: {0}")]
    InvalidContentLength(String),
    #[error("message from the language server is not UTF-8")]
    NotUtf8(#[source] std::string::FromUtf8Error),
    #[error("message from the language server is not JSON")]
    MalformedMessage(#[source] serde_json::Error),
    #[error("cannot encode {method} for the language server")]
    Encode {
        method: String,
        #[source]
        source: serde_json::Error,
    },
    /// The server answered the request with a JSON-RPC error.
    #[error("{method} failed: {error}")]
    Rejected {
        method: String,
        error: serde_json::Value,
    },
    #[error("{method} response has no result")]
    MissingResult { method: String },
    #[error("unexpected {method} result")]
    UnexpectedResult {
        method: String,
        #[source]
        source: serde_json::Error,
    },
    /// The server did not answer in time. The process is killed, so the next
    /// lookup through the manager starts a fresh one.
    #[error("{method} got no response within {}s; the server was stopped", REQUEST_TIMEOUT.as_secs())]
    TimedOut { method: String },
}

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
    process: parking_lot::Mutex<tokio::process::Child>,
    stdin: Mutex<tokio::process::ChildStdin>,
    stdout_reader: Mutex<BufReader<tokio::process::ChildStdout>>,
    next_id: AtomicI32,
    server_name: String,
    diagnostics: parking_lot::Mutex<HashMap<Url, Vec<Diagnostic>>>,
    /// Serializes whole request/response cycles. The read loop consumes
    /// messages until it sees its own response id, so two requests in flight at
    /// once would steal and discard each other's responses and then block
    /// forever. Holding this for the duration of a request makes a single
    /// client safe to share across concurrent callers (overlapping async MCP
    /// tool calls hit the same cached client).
    request_lock: Mutex<()>,
}

impl LspClient {
    /// Spawn a new LSP server process and create a client
    pub async fn spawn(
        server_name: String,
        config: &LanguageServerConfig,
        workspace_root: &std::path::Path,
    ) -> Result<Self, LspError> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| LspError::Spawn {
                command: config.command.clone(),
                source,
            })?;

        let (stdin, stdout) = take_pipes(&mut child);

        let mut client = Self {
            process: parking_lot::Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout_reader: Mutex::new(BufReader::new(stdout)),
            next_id: AtomicI32::new(1),
            server_name,
            diagnostics: parking_lot::Mutex::new(HashMap::new()),
            request_lock: Mutex::new(()),
        };

        client.initialize(workspace_root).await?;

        Ok(client)
    }

    /// Initialize the LSP server
    async fn initialize(&mut self, workspace_root: &std::path::Path) -> Result<(), LspError> {
        let root_uri = Url::from_file_path(workspace_root)
            .map_err(|()| LspError::InvalidRoot(workspace_root.to_path_buf()))?;

        #[expect(
            deprecated,
            reason = "servers that predate workspace folders still root themselves at root_uri"
        )]
        let init_params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(root_uri.clone()),
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
        self.send_notification("initialized", InitializedParams {})
            .await?;

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
                tracing::warn!(
                    server = %self.server_name,
                    cap_secs = MAX_INDEXING_WAIT.as_secs(),
                    "indexing wait hit the cap; proceeding"
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
                            tracing::info!(server = %self.server_name, "indexing complete");
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
    async fn handle_server_message(&self, value: &serde_json::Value) -> Result<(), LspError> {
        let method = value.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // Capture pushed diagnostics regardless of request/notification.
        if method == "textDocument/publishDiagnostics"
            && let Some(params) = value.get("params")
            && let Ok(diag_params) =
                serde_json::from_value::<PublishDiagnosticsParams>(params.clone())
        {
            self.diagnostics
                .lock()
                .insert(diag_params.uri, diag_params.diagnostics);
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
    async fn send_response(
        &self,
        id: &serde_json::Value,
        result: serde_json::Value,
    ) -> Result<(), LspError> {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        self.write_message(&response.to_string()).await
    }

    async fn write_message(&self, body: &str) -> Result<(), LspError> {
        let message = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(message.as_bytes())
            .await
            .map_err(LspError::Write)?;
        stdin.flush().await.map_err(LspError::Write)
    }

    /// Send textDocument/didOpen notification
    pub async fn did_open(&self, uri: &Url, language_id: &str, text: &str) -> Result<(), LspError> {
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
    pub async fn workspace_symbol(&self, query: &str) -> Result<Vec<SymbolInformation>, LspError> {
        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            partial_result_params: PartialResultParams::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        // WorkspaceSymbolResponse can be either Flat(Vec<SymbolInformation>) or Nested(Vec<WorkspaceSymbol>)
        let response: Option<WorkspaceSymbolResponse> =
            self.send_request("workspace/symbol", params).await?;

        let symbols = match response {
            Some(WorkspaceSymbolResponse::Flat(symbols)) => symbols,
            Some(WorkspaceSymbolResponse::Nested(workspace_symbols)) => workspace_symbols
                .into_iter()
                .map(flatten_workspace_symbol)
                .collect(),
            None => vec![],
        };

        Ok(symbols)
    }

    /// Send textDocument/documentSymbol request
    pub async fn document_symbol(
        &self,
        uri: &Url,
    ) -> Result<Option<DocumentSymbolResponse>, LspError> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response: Option<DocumentSymbolResponse> = self
            .send_request("textDocument/documentSymbol", params)
            .await?;

        Ok(response)
    }

    /// Send textDocument/definition request
    pub async fn goto_definition(
        &self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>, LspError> {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let response: Option<GotoDefinitionResponse> =
            self.send_request("textDocument/definition", params).await?;

        Ok(response)
    }

    /// Send textDocument/references request
    pub async fn find_references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<Location>>, LspError> {
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

        let response: Option<Vec<Location>> =
            self.send_request("textDocument/references", params).await?;

        Ok(response)
    }

    /// Send textDocument/hover request
    pub async fn hover(&self, uri: &Url, position: Position) -> Result<Option<Hover>, LspError> {
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
    ) -> Result<Option<CompletionResponse>, LspError> {
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
    pub async fn pull_diagnostics(&self, uri: &Url) -> Result<Vec<Diagnostic>, LspError> {
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
                DocumentDiagnosticReport::Full(full) => full.full_document_diagnostic_report.items,
                DocumentDiagnosticReport::Unchanged(_) => vec![],
            },
            DocumentDiagnosticReportResult::Partial(_) => vec![],
        };

        Ok(diagnostics)
    }

    /// Get stored diagnostics for a URI (from publishDiagnostics notifications)
    pub fn get_stored_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        self.diagnostics
            .lock()
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    /// Send textDocument/codeAction request
    pub async fn code_action(
        &self,
        uri: &Url,
        range: Range,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Option<CodeActionResponse>, LspError> {
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
    ) -> Result<R, LspError> {
        let _req_guard = self.request_lock.lock().await;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let exchange = async {
            self.write_message(&request.to_string()).await?;
            self.read_response(id, method).await
        };
        let result = self.within_timeout(method, exchange).await?;
        serde_json::from_value(result).map_err(|source| LspError::UnexpectedResult {
            method: method.to_string(),
            source,
        })
    }

    /// Read messages until the response to request `id` arrives, handling the
    /// notifications and server→client requests the server interleaves.
    async fn read_response(&self, id: i32, method: &str) -> Result<serde_json::Value, LspError> {
        loop {
            let raw = self.read_message().await?;
            let mut response: serde_json::Value =
                serde_json::from_str(&raw).map_err(LspError::MalformedMessage)?;

            if response.get("method").is_some() {
                self.handle_server_message(&response).await?;
                continue;
            }

            if response.get("id") != Some(&serde_json::json!(id)) {
                tracing::warn!(
                    server = %self.server_name,
                    expected = id,
                    got = ?response.get("id"),
                    "skipping response with mismatched id"
                );
                continue;
            }

            if let Some(error) = response.get_mut("error") {
                return Err(LspError::Rejected {
                    method: method.to_string(),
                    error: error.take(),
                });
            }

            return response
                .get_mut("result")
                .map(serde_json::Value::take)
                .ok_or_else(|| LspError::MissingResult {
                    method: method.to_string(),
                });
        }
    }

    /// Run one exchange with the server under [`REQUEST_TIMEOUT`]. A timed-out
    /// exchange may have stopped mid-message, so the stream can no longer be
    /// trusted and the process is killed.
    async fn within_timeout<T>(
        &self,
        method: &str,
        exchange: impl Future<Output = Result<T, LspError>>,
    ) -> Result<T, LspError> {
        match tokio::time::timeout(REQUEST_TIMEOUT, exchange).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(server = %self.server_name, method, "request timed out; stopping the server");
                if let Err(e) = self.process.lock().start_kill() {
                    let error: &dyn std::error::Error = &e;
                    tracing::warn!(server = %self.server_name, error, "could not kill the server");
                }
                Err(LspError::TimedOut {
                    method: method.to_string(),
                })
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected)
    async fn send_notification<P: serde::Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<(), LspError> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.within_timeout(method, self.write_message(&notification.to_string()))
            .await
    }

    async fn read_message(&self) -> Result<String, LspError> {
        let mut reader = self.stdout_reader.lock().await;

        let mut content_length: Option<usize> = None;
        let mut line = String::new();

        loop {
            line.clear();
            let read = reader.read_line(&mut line).await.map_err(LspError::Read)?;
            if read == 0 {
                return Err(LspError::Read(std::io::ErrorKind::UnexpectedEof.into()));
            }

            if line == "\r\n" {
                break;
            }

            if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                let len_str = len_str.trim();
                content_length = Some(
                    len_str
                        .parse()
                        .map_err(|_| LspError::InvalidContentLength(len_str.to_string()))?,
                );
            }
        }

        let content_length = content_length.ok_or(LspError::MissingContentLength)?;

        let mut buffer = vec![0u8; content_length];
        reader
            .read_exact(&mut buffer)
            .await
            .map_err(LspError::Read)?;

        String::from_utf8(buffer).map_err(LspError::NotUtf8)
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Whether the server process is still running. A process whose status
    /// cannot be read counts as dead.
    pub fn is_alive(&self) -> bool {
        matches!(self.process.lock().try_wait(), Ok(None))
    }
}

#[expect(
    deprecated,
    reason = "SymbolInformation has no constructor that leaves its deprecated field out"
)]
fn flatten_workspace_symbol(ws: WorkspaceSymbol) -> SymbolInformation {
    let location = match ws.location {
        OneOf::Left(loc) => loc,
        OneOf::Right(ws_loc) => Location {
            uri: ws_loc.uri,
            range: lsp_types::Range::default(),
        },
    };
    SymbolInformation {
        name: ws.name,
        kind: ws.kind,
        tags: ws.tags,
        deprecated: None,
        location,
        container_name: ws.container_name,
    }
}

#[expect(
    clippy::expect_used,
    reason = "stdin and stdout are set to piped on the command before spawning"
)]
fn take_pipes(
    child: &mut tokio::process::Child,
) -> (tokio::process::ChildStdin, tokio::process::ChildStdout) {
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    (stdin, stdout)
}
