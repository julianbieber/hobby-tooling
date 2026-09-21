//! `textDocument/references` end to end over stdio, against a fixture workspace
//! written to a temporary directory. Skipped silently when the helix runtime
//! isn't available or resolves no Python language.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};
use treesitter_class_lsp::languages::LanguageRegistry;

const LIB_PY: &str = "class Widget:\n    \"\"\"A Widget.\"\"\"\n\n\nprint(\"Widget\")\n";
const APP_PY: &str = "w = Widget()\n";
const DEFS_PROTO: &str = "syntax = \"proto3\";\nservice Widget {\n}\n";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Option<Self> {
        let registry = match LanguageRegistry::load() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("skipping: helix runtime not available: {e}");
                return None;
            }
        };
        if registry.resolve(Path::new("probe.py")).is_none() {
            eprintln!("skipping: no python language configured");
            return None;
        }
        let root = std::env::temp_dir().join(format!("tslsp-refs-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("lib.py"), LIB_PY).unwrap();
        std::fs::write(root.join("app.py"), APP_PY).unwrap();
        std::fs::write(root.join("defs.proto"), DEFS_PROTO).unwrap();
        Some(Self {
            root: std::fs::canonicalize(root).unwrap(),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Server {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    next_id: u64,
}

impl Server {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_treesitter-class-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            while let Some(msg) = recv(&mut reader) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child,
            stdin,
            messages,
            next_id: 1,
        };
        server.request(
            "initialize",
            json!({"processId": null, "rootUri": format!("file://{}", root.display()), "capabilities": {}}),
        );
        server.notify("initialized", json!({}));
        server.wait_until_indexed();
        server
    }

    fn notify(&mut self, method: &str, params: Value) {
        send(&mut self.stdin, &json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        );
        loop {
            let msg = self.messages.recv_timeout(Duration::from_secs(60)).unwrap();
            if msg.get("id") == Some(&json!(id)) {
                return msg["result"].clone();
            }
        }
    }

    fn wait_until_indexed(&self) {
        loop {
            let msg = self.messages.recv_timeout(Duration::from_secs(60)).unwrap();
            let text = msg["params"]["message"].as_str().unwrap_or("");
            if msg["method"] == "window/logMessage" && text.contains("treesitter-class-lsp: indexed") {
                return;
            }
        }
    }

    fn open(&mut self, path: &Path, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": file_uri(path), "languageId": "python", "version": 1, "text": text}}),
        );
    }

    fn references(&mut self, path: &Path, line: u32, character: u32, include_declaration: bool) -> Value {
        self.request(
            "textDocument/references",
            json!({
                "textDocument": {"uri": file_uri(path)},
                "position": {"line": line, "character": character},
                "context": {"includeDeclaration": include_declaration},
            }),
        )
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn send<W: Write>(w: &mut W, msg: &Value) {
    let body = msg.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    w.flush().unwrap();
}

fn recv<R: BufRead>(r: &mut R) -> Option<Value> {
    let mut len = 0usize;
    let mut header = String::new();
    loop {
        header.clear();
        if r.read_line(&mut header).ok()? == 0 {
            return None;
        }
        if header == "\r\n" {
            break;
        }
        if let Some(rest) = header.strip_prefix("Content-Length:") {
            len = rest.trim().parse().ok()?;
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn every_location(answer: &Value) -> Vec<(String, u64, u64)> {
    answer
        .as_array()
        .expect("an array of locations")
        .iter()
        .map(|location| {
            let start = &location["range"]["start"];
            let file = location["uri"].as_str().unwrap().rsplit('/').next().unwrap();
            (file.to_string(), start["line"].as_u64().unwrap(), start["character"].as_u64().unwrap())
        })
        .collect()
}

fn python_locations(answer: &Value) -> Vec<(String, u64, u64)> {
    every_location(answer)
        .into_iter()
        .filter(|(file, _, _)| file.ends_with(".py"))
        .collect()
}

// Pins S8: references to `Widget` from its definition hold `lib.py`'s definition and `app.py`'s use.
#[test]
fn references_hold_the_definition_and_every_use() {
    let Some(fx) = Fixture::new("uses") else { return };
    let mut server = Server::start(&fx.root);
    let lib = fx.root.join("lib.py");
    server.open(&lib, LIB_PY);

    let answer = server.references(&lib, 0, 6, true);

    assert_eq!(
        python_locations(&answer),
        [("app.py".to_string(), 0, 4), ("lib.py".to_string(), 0, 6)]
    );
}

// Pins that a mention is not a reference: the docstring and the `print("Widget")` string in `lib.py` are absent.
#[test]
fn a_mention_in_a_docstring_or_string_is_not_a_reference() {
    let Some(fx) = Fixture::new("mentions") else { return };
    let mut server = Server::start(&fx.root);
    let lib = fx.root.join("lib.py");
    server.open(&lib, LIB_PY);

    let answer = server.references(&lib, 0, 6, true);

    let lib_lines: Vec<u64> = python_locations(&answer)
        .into_iter()
        .filter(|(file, _, _)| file == "lib.py")
        .map(|(_, line, _)| line)
        .collect();
    assert_eq!(lib_lines, [0]);
}

// Pins that references in the buffer's language come first for a Python buffer, though `defs.proto` sorts before `lib.py`.
#[test]
fn references_in_the_buffers_language_are_listed_first() {
    let Some(fx) = Fixture::new("order") else { return };
    if LanguageRegistry::load().unwrap().resolve(Path::new("probe.proto")).is_none() {
        eprintln!("skipping: no protobuf language configured");
        return;
    }
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.references(&app, 0, 4, true);

    assert_eq!(
        every_location(&answer),
        [
            ("app.py".to_string(), 0, 4),
            ("lib.py".to_string(), 0, 6),
            ("defs.proto".to_string(), 1, 8)
        ]
    );
}

// Pins that `includeDeclaration: false` drops the definition row and keeps the use.
#[test]
fn excluding_the_declaration_drops_the_definition_and_keeps_the_use() {
    let Some(fx) = Fixture::new("no-declaration") else { return };
    let mut server = Server::start(&fx.root);
    let lib = fx.root.join("lib.py");
    server.open(&lib, LIB_PY);

    let answer = server.references(&lib, 0, 6, false);

    assert_eq!(python_locations(&answer), [("app.py".to_string(), 0, 4)]);
}
