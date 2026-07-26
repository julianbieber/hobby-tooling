use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    #[allow(dead_code)]
    client: Client,
    /// The first workspace root the editor announced at `initialize`, used as a
    /// fallback base for relative paths that don't resolve against the current
    /// file's directory.
    workspace_root: OnceLock<PathBuf>,
    documents: Mutex<HashMap<Url, String>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            workspace_root: OnceLock::new(),
            documents: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve the path token under the cursor in `uri` to an existing file or
    /// directory, jumping to the `:line:col` suffix if one is present.
    fn definition_at(&self, uri: &Url, pos: Position) -> Option<Location> {
        let text = {
            let docs = self.documents.lock().unwrap();
            docs.get(uri).cloned()
        };
        // Fall back to disk if the editor never sent the document (e.g. a stale
        // request, or a server that didn't `didOpen`).
        let text = text.or_else(|| {
            uri.to_file_path()
                .ok()
                .and_then(|p| fs::read_to_string(p).ok())
        })?;

        let token = path_token_at(&text, pos)?;
        let (raw, line, col) = split_suffix(&token);

        let current_dir = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let target = resolve(
            raw,
            current_dir.as_deref(),
            self.workspace_root.get().map(PathBuf::as_path),
        )?;

        let target_uri = Url::from_file_path(&target).ok()?;
        // Editor positions are 0-based; path suffixes are 1-based.
        let position = Position {
            line: line.map(|l| l.saturating_sub(1)).unwrap_or(0),
            character: col.map(|c| c.saturating_sub(1)).unwrap_or(0),
        };
        Some(Location {
            uri: target_uri,
            range: Range {
                start: position,
                end: position,
            },
        })
    }
}

/// Walk left and right from the cursor over path-like characters, returning the
/// whole token. Stops at whitespace, quotes, and common delimiters so a path
/// embedded in prose or a markdown link stays whole. Paths containing spaces are
/// not supported.
fn path_token_at(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    // Clamp the cursor; when it sits just past the last char, step back onto it.
    let mut idx = (pos.character as usize).min(len);
    if idx == len && idx > 0 {
        idx -= 1;
    }
    if idx >= len || !is_path_char(chars[idx]) {
        return None;
    }
    let mut start = idx;
    while start > 0 && is_path_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end < len && is_path_char(chars[end]) {
        end += 1;
    }
    let token: String = chars[start..end].iter().collect();
    if token.is_empty() { None } else { Some(token) }
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '/' | '.' | '-' | '_' | '~' | '@' | '+' | ':')
}

/// Split a `path:line:col` token into its path and optional 1-based line/column.
/// Trailing all-digit `:`-segments (at most two) are treated as a position;
/// everything else stays part of the path. A bare trailing `:` is trimmed.
fn split_suffix(token: &str) -> (&str, Option<u32>, Option<u32>) {
    let parts: Vec<&str> = token.split(':').collect();
    let mut nums: Vec<u32> = Vec::new();
    let mut end = parts.len();
    while end > 1 && nums.len() < 2 {
        match parts[end - 1].parse::<u32>() {
            Ok(n) => {
                nums.push(n);
                end -= 1;
            }
            Err(_) => break,
        }
    }
    // `parts[..end].join(":")` would reallocate; the path is a contiguous prefix
    // of the original token, so slice it directly.
    let consumed: usize = parts[end..].iter().map(|s| s.len() + 1).sum();
    let path = token[..token.len() - consumed].trim_end_matches(':');

    nums.reverse(); // collected right-to-left → [line, col?]
    (path, nums.first().copied(), nums.get(1).copied())
}

fn expand_tilde_with(s: &str, home: &str) -> String {
    if s == "~" {
        home.to_string()
    } else if let Some(rest) = s.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        s.to_string()
    }
}

fn expand_tilde(s: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) => expand_tilde_with(s, &home),
        Err(_) => s.to_string(),
    }
}

/// Resolve `raw` to the first existing path: absolute paths as-is, relative
/// paths against the current file's directory then the workspace root.
fn resolve(raw: &str, current_dir: Option<&Path>, workspace: Option<&Path>) -> Option<PathBuf> {
    let expanded = expand_tilde(raw);
    let p = Path::new(&expanded);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if p.is_absolute() {
        candidates.push(p.to_path_buf());
    } else {
        if let Some(dir) = current_dir {
            candidates.push(dir.join(p));
        }
        if let Some(ws) = workspace {
            candidates.push(ws.join(p));
        }
    }
    candidates
        .into_iter()
        .find(|c| c.exists())
        .and_then(|c| c.canonicalize().ok())
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Prefer the first workspace folder, then the deprecated root_uri, then root_path.
        let root = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|f| f.uri.to_file_path().ok())
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(|u| u.to_file_path().ok())
            })
            .or_else(|| {
                #[allow(deprecated)]
                params.root_path.clone().map(PathBuf::from)
            });
        if let Some(root) = root {
            let _ = self.workspace_root.set(root);
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "path_lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.documents
                .lock()
                .unwrap()
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(self
            .definition_at(&uri, pos)
            .map(GotoDefinitionResponse::Scalar))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn token_at_cursor_in_prose() {
        let text = "see ./src/main.rs for details";
        // cursor on the "m" of main
        let tok = path_token_at(text, pos(0, 10));
        assert_eq!(tok.as_deref(), Some("./src/main.rs"));
    }

    #[test]
    fn token_stops_at_quotes_and_parens() {
        let text = "[link](./docs/readme.md) and \"~/notes.txt\"";
        assert_eq!(
            path_token_at(text, pos(0, 9)).as_deref(),
            Some("./docs/readme.md")
        );
        assert_eq!(
            path_token_at(text, pos(0, 32)).as_deref(),
            Some("~/notes.txt")
        );
    }

    #[test]
    fn token_includes_line_col_suffix() {
        let text = "at src/main.rs:42:10 boom";
        assert_eq!(
            path_token_at(text, pos(0, 5)).as_deref(),
            Some("src/main.rs:42:10")
        );
    }

    #[test]
    fn token_none_on_whitespace() {
        let text = "a   b";
        assert_eq!(path_token_at(text, pos(0, 2)), None);
    }

    #[test]
    fn token_at_end_of_word_steps_back() {
        let text = "file.rs";
        // cursor one past the last char
        assert_eq!(path_token_at(text, pos(0, 7)).as_deref(), Some("file.rs"));
    }

    #[test]
    fn suffix_line_and_col() {
        assert_eq!(
            split_suffix("src/main.rs:42:10"),
            ("src/main.rs", Some(42), Some(10))
        );
    }

    #[test]
    fn suffix_line_only() {
        assert_eq!(
            split_suffix("src/main.rs:42"),
            ("src/main.rs", Some(42), None)
        );
    }

    #[test]
    fn suffix_none() {
        assert_eq!(split_suffix("src/main.rs"), ("src/main.rs", None, None));
    }

    #[test]
    fn suffix_trailing_colon_trimmed() {
        assert_eq!(split_suffix("src/main.rs:"), ("src/main.rs", None, None));
    }

    #[test]
    fn suffix_non_numeric_kept_in_path() {
        // A scheme-like prefix must not be mistaken for a position.
        assert_eq!(split_suffix("http://x"), ("http://x", None, None));
    }

    #[test]
    fn suffix_caps_at_two_numbers() {
        // Only the last two numeric segments are a position; the rest stays path.
        assert_eq!(split_suffix("a:1:2:3"), ("a:1", Some(2), Some(3)));
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(expand_tilde_with("~", "/home/me"), "/home/me");
        assert_eq!(
            expand_tilde_with("~/notes.txt", "/home/me"),
            "/home/me/notes.txt"
        );
        assert_eq!(expand_tilde_with("./rel", "/home/me"), "./rel");
    }

    #[test]
    fn resolve_absolute() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "x").unwrap();
        let got = resolve(file.to_str().unwrap(), None, None).unwrap();
        assert_eq!(got, file.canonicalize().unwrap());
    }

    #[test]
    fn resolve_relative_to_current_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        let got = resolve("a.txt", Some(dir.path()), None).unwrap();
        assert_eq!(got, dir.path().join("a.txt").canonicalize().unwrap());
    }

    #[test]
    fn resolve_falls_back_to_workspace() {
        let cur = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        fs::write(ws.path().join("a.txt"), "x").unwrap();
        // Not present under cur, found under workspace.
        let got = resolve("a.txt", Some(cur.path()), Some(ws.path())).unwrap();
        assert_eq!(got, ws.path().join("a.txt").canonicalize().unwrap());
    }

    #[test]
    fn resolve_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve("nope.txt", Some(dir.path()), None), None);
    }
}
