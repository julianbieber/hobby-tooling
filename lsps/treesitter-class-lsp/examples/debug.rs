//! Indexes a directory with the helix grammars and prints every definition
//! found, one per line.
//!
//! Usage: `debug [<dir>]`, default this crate's `src/`.

mod cli;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use treesitter_class_lsp::indexer::Indexer;
use treesitter_class_lsp::languages::LanguageRegistry;

use cli::{init_logging, report};

fn main() -> ExitCode {
    init_logging("debug");

    let registry = match LanguageRegistry::load() {
        Ok(registry) => registry,
        Err(e) => return report(&e),
    };
    let rs = format!("ext 'rs' -> {:?}", registry.resolve(Path::new("foo.rs")));
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"));
    let index = Indexer::new(registry).index_root(&dir);

    println!("{rs}");
    for e in index.entries() {
        let file = e.file.file_name().unwrap_or_default().to_string_lossy();
        println!(
            "{} ({:?}) in {}:{}",
            e.name, e.kind, file, e.range.start.line
        );
    }
    println!("total: {}", index.entries().len());
    ExitCode::SUCCESS
}
