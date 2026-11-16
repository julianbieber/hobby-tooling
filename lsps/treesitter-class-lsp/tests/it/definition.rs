//! `textDocument/definition` end to end over stdio, against a fixture workspace
//! written to a temporary directory. Skipped silently when the helix runtime
//! isn't available or resolves no Python language.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use treesitter_class_lsp::languages::LanguageRegistry;

const LIB_PY: &str = "class Widget:\n    def draw(self):\n        pass\n\n\ndef make():\n    return Widget()\n\n\nclass Shared:\n    pass\n";
const DEFS_PROTO: &str = "syntax = \"proto3\";\nservice Shared {\n}\n";
const USE_PY: &str = "Shared()\n";
const APP_PY: &str = "import json\n\nw = Widget()\nmake()\njson.dumps(w)\n";

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
        let root = std::env::temp_dir().join(format!("tslsp-def-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("lib.py"), LIB_PY).unwrap();
        std::fs::write(root.join("app.py"), APP_PY).unwrap();
        std::fs::write(root.join("defs.proto"), DEFS_PROTO).unwrap();
        std::fs::write(root.join("use.py"), USE_PY).unwrap();
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
        send(
            &mut self.stdin,
            &json!({"jsonrpc": "2.0", "method": method, "params": params}),
        );
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
            if msg["method"] == "window/logMessage"
                && text.contains("treesitter-class-lsp: indexed")
            {
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

    fn definition(&mut self, path: &Path, line: u32, character: u32) -> Value {
        self.request(
            "textDocument/definition",
            json!({"textDocument": {"uri": file_uri(path)}, "position": {"line": line, "character": character}}),
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

fn only_location(answer: &Value) -> (String, u64, u64) {
    let locations = answer.as_array().expect("an array of locations");
    assert_eq!(locations.len(), 1, "expected one location: {answer}");
    let start = &locations[0]["range"]["start"];
    let uri = locations[0]["uri"].as_str().unwrap();
    let file = uri.rsplit('/').next().unwrap().to_string();
    (
        file,
        start["line"].as_u64().unwrap(),
        start["character"].as_u64().unwrap(),
    )
}

fn every_location(answer: &Value) -> Vec<(String, u64, u64)> {
    answer
        .as_array()
        .expect("an array of locations")
        .iter()
        .map(|location| {
            let start = &location["range"]["start"];
            let file = location["uri"]
                .as_str()
                .unwrap()
                .rsplit('/')
                .next()
                .unwrap();
            (
                file.to_string(),
                start["line"].as_u64().unwrap(),
                start["character"].as_u64().unwrap(),
            )
        })
        .collect()
}

// Pins that a class use resolves to the class's name in `class Widget:`, not to the head of the block.
#[test]
fn class_use_resolves_to_the_name_of_its_definition() {
    let Some(fx) = Fixture::new("class") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.definition(&app, 2, 6);

    assert_eq!(only_location(&answer), ("lib.py".to_string(), 0, 6));
}

// Pins that a call to a free function resolves to the name in its `def`.
#[test]
fn free_function_use_resolves_to_the_name_in_its_def() {
    let Some(fx) = Fixture::new("function") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.definition(&app, 3, 1);

    assert_eq!(only_location(&answer), ("lib.py".to_string(), 5, 4));
}

// Pins that a name nothing in the workspace defines answers no location, which is what makes Helix print "No definition found.".
#[test]
fn name_nothing_defines_answers_no_location() {
    let Some(fx) = Fixture::new("undefined") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    server.open(&app, APP_PY);

    let answer = server.definition(&app, 0, 8);

    assert!(answer.is_null(), "expected no location, got {answer}");
}

// Pins that navigation follows the file on disk: an appended call and definition resolve only after the file is saved.
#[test]
fn unsaved_edits_are_invisible_until_the_file_is_saved() {
    let Some(fx) = Fixture::new("unsaved") else {
        return;
    };
    let mut server = Server::start(&fx.root);
    let app = fx.root.join("app.py");
    let edited = format!("{APP_PY}brand_new()\ndef brand_new(): pass\n");
    server.open(&app, &edited);

    let unsaved = server.definition(&app, 5, 2);
    assert!(
        unsaved.is_null(),
        "expected no location before saving, got {unsaved}"
    );

    std::fs::write(&app, &edited).unwrap();
    server.notify(
        "textDocument/didSave",
        json!({"textDocument": {"uri": file_uri(&app)}}),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saved = server.definition(&app, 5, 2);
    while saved.is_null() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        saved = server.definition(&app, 5, 2);
    }

    assert_eq!(only_location(&saved), ("app.py".to_string(), 6, 4));
}

// Pins S4: a name defined in Python and in protobuf lists the Python definition first for a Python buffer, though `defs.proto` sorts before `lib.py`.
#[test]
fn definitions_in_the_buffers_language_are_listed_first() {
    let Some(fx) = Fixture::new("ambiguous") else {
        return;
    };
    if LanguageRegistry::load()
        .unwrap()
        .resolve(Path::new("probe.proto"))
        .is_none()
    {
        eprintln!("skipping: no protobuf language configured");
        return;
    }
    let mut server = Server::start(&fx.root);
    let usage = fx.root.join("use.py");
    server.open(&usage, USE_PY);

    let answer = server.definition(&usage, 0, 1);

    assert_eq!(
        every_location(&answer),
        [
            ("lib.py".to_string(), 9, 6),
            ("defs.proto".to_string(), 1, 8)
        ]
    );
}
