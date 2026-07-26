use std::path::PathBuf;
use treesitter_class_lsp::indexer::Indexer;
use treesitter_class_lsp::languages::LanguageRegistry;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("debug"))
        .init();

    let reg = LanguageRegistry::load().unwrap();
    println!("ext 'rs' -> {:?}", reg.resolve(std::path::Path::new("foo.rs")));
    let this = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"));
    let idx = Indexer::new(reg).index_root(&this);
    for e in idx.entries() {
        println!("{} ({:?}) in {}:{}", e.name, e.kind, e.file.file_name().unwrap().to_string_lossy(), e.range.start.line);
    }
    println!("total: {}", idx.entries().len());
}
