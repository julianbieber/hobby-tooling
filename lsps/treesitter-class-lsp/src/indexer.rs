use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ignore::WalkBuilder;
use tower_lsp::lsp_types::{Position, Range, SymbolKind, Url};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use crate::grammars::GrammarCache;
use crate::languages::{LanguageInfo, LanguageRegistry, load_tags_query};

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub name: String,
    pub language: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub range: Range,
}

impl IndexEntry {
    pub fn uri(&self) -> Option<Url> {
        Url::from_file_path(&self.file).ok()
    }
}

#[derive(Default)]
pub struct Index {
    entries: Vec<IndexEntry>,
    /// `file path -> indices into `entries`` for incremental re-indexing.
    by_file: HashMap<PathBuf, Vec<usize>>,
}

impl Index {
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    pub fn replace_file(&mut self, file: &Path, mut new_entries: Vec<IndexEntry>) {
        if let Some(old_idxs) = self.by_file.remove(file) {
            // Remove entries by collecting retained ones; cheaper than repeated removes
            // for large indices.
            let mut drop_mask = vec![false; self.entries.len()];
            for idx in old_idxs {
                if idx < drop_mask.len() {
                    drop_mask[idx] = true;
                }
            }
            let mut remapped: HashMap<usize, usize> = HashMap::new();
            let mut kept = Vec::with_capacity(self.entries.len());
            for (i, entry) in self.entries.drain(..).enumerate() {
                if !drop_mask[i] {
                    remapped.insert(i, kept.len());
                    kept.push(entry);
                }
            }
            self.entries = kept;
            // Rewrite remaining by_file indices.
            for idxs in self.by_file.values_mut() {
                idxs.retain_mut(|i| match remapped.get(i) {
                    Some(new_i) => {
                        *i = *new_i;
                        true
                    }
                    None => false,
                });
            }
        }

        let start = self.entries.len();
        self.entries.append(&mut new_entries);
        let added: Vec<usize> = (start..self.entries.len()).collect();
        if !added.is_empty() {
            self.by_file.insert(file.to_path_buf(), added);
        }
    }
}

/// Which captures we consider "class-like". We also accept the typo
/// `defintion.class` present in the Java query.
fn definition_kind_from_capture(name: &str) -> Option<SymbolKind> {
    let name = name.strip_prefix('@').unwrap_or(name);
    match name {
        "definition.class" | "defintion.class" => Some(SymbolKind::CLASS),
        "definition.struct" => Some(SymbolKind::STRUCT),
        "definition.interface" | "definition.trait" => Some(SymbolKind::INTERFACE),
        "definition.enum" => Some(SymbolKind::ENUM),
        "definition.type" => Some(SymbolKind::TYPE_PARAMETER),
        "definition.function" => Some(SymbolKind::FUNCTION),
        "definition.method" => Some(SymbolKind::METHOD),
        _ => None,
    }
}

fn can_contain_methods(kind: SymbolKind) -> bool {
    kind == SymbolKind::CLASS
        || kind == SymbolKind::STRUCT
        || kind == SymbolKind::INTERFACE
        || kind == SymbolKind::ENUM
}

fn range_contains(outer: &Range, inner: &Range) -> bool {
    let start = |r: &Range| (r.start.line, r.start.character);
    let end = |r: &Range| (r.end.line, r.end.character);
    start(outer) <= start(inner) && end(inner) <= end(outer) && outer != inner
}

fn promote_methods(entries: &mut [IndexEntry]) {
    let containers: Vec<Range> = entries
        .iter()
        .filter(|e| can_contain_methods(e.kind))
        .map(|e| e.range)
        .collect();
    for entry in entries.iter_mut() {
        if entry.kind == SymbolKind::FUNCTION
            && containers.iter().any(|c| range_contains(c, &entry.range))
        {
            entry.kind = SymbolKind::METHOD;
        }
    }
}

fn range_from_ts_node(node: tree_sitter::Node) -> Range {
    let s = node.start_position();
    let e = node.end_position();
    Range {
        start: Position {
            line: s.row as u32,
            character: s.column as u32,
        },
        end: Position {
            line: e.row as u32,
            character: e.column as u32,
        },
    }
}

pub struct Indexer {
    registry: LanguageRegistry,
    grammars: GrammarCache,
    /// Cached (parsed Query, capture indices) per language. A `Query` is tied
    /// to a `Language`, which lives for the process lifetime via `GrammarCache`.
    queries: std::sync::Mutex<HashMap<String, std::sync::Arc<CompiledQuery>>>,
}

struct CompiledQuery {
    query: Query,
    /// For each capture index in the query, the `SymbolKind` if it's a
    /// definition, otherwise `None` (skipped).
    def_kinds: Vec<Option<SymbolKind>>,
    /// Capture index of `@name` if present.
    name_idx: Option<u32>,
}

impl Indexer {
    pub fn new(registry: LanguageRegistry) -> Self {
        Self {
            registry,
            grammars: GrammarCache::new(),
            queries: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn registry(&self) -> &LanguageRegistry {
        &self.registry
    }

    /// Index the whole workspace rooted at `root`, returning a fresh `Index`.
    pub fn index_root(&self, root: &Path) -> Index {
        let mut index = Index::default();
        let mut count_files = 0usize;
        // Skip the *global* gitignore (`~/.gitignore`, `~/.config/git/ignore`)
        // because users often set a catch-all `*` there to keep ad-hoc files
        // out of `git status`; that would make this walker return zero files.
        // Per-repo `.gitignore` and `.git/info/exclude` are still respected so
        // we skip build outputs (`target/`, `node_modules/`, …).
        for result in WalkBuilder::new(root)
            .hidden(true)
            .follow_links(false)
            .git_global(false)
            .git_exclude(true)
            .parents(false)
            .build()
        {
            let Ok(entry) = result else { continue };
            if !entry
                .file_type()
                .map(|t| t.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let path = entry.path();
            let Some(lang) = self.registry.resolve(path) else {
                continue;
            };
            match self.index_file(path, lang) {
                Ok(entries) => {
                    if !entries.is_empty() {
                        index.replace_file(path, entries);
                    }
                    count_files += 1;
                }
                Err(e) => {
                    tracing::debug!("index {} failed: {}", path.display(), e);
                }
            }
        }
        tracing::info!(
            "indexed {} files, {} symbols under {}",
            count_files,
            index.entries.len(),
            root.display()
        );
        index
    }

    /// Parse a single file and return its definitions (class-like, functions, methods). Returns an
    /// empty Vec on failure modes that aren't worth surfacing (missing query,
    /// unreadable file).
    pub fn index_file(
        &self,
        path: &Path,
        lang: &LanguageInfo,
    ) -> anyhow::Result<Vec<IndexEntry>> {
        let source = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return Ok(Vec::new()),
        };
        // Skip obviously-binary files.
        if source.contains(&0u8) {
            return Ok(Vec::new());
        }

        let grammar = self
            .grammars
            .get_or_load(&self.registry, lang)
            .with_context(|| format!("loading grammar for {}", lang.name))?;

        let compiled = match self.compiled_query(lang, &grammar.language) {
            Some(q) => q,
            None => return Ok(Vec::new()),
        };

        let mut parser = Parser::new();
        parser
            .set_language(&grammar.language)
            .with_context(|| format!("setting language {}", lang.name))?;
        let Some(tree) = parser.parse(&source, None) else {
            return Ok(Vec::new());
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&compiled.query, tree.root_node(), source.as_slice());
        let mut out = Vec::new();
        while let Some(m) = matches.next() {
            // Priority: if `@name` exists in this match, use its text +
            // use the co-matched `@definition.*` kind for the symbol. Otherwise
            // fall back to the `@definition.*` capture itself (rust style).
            let mut name_text: Option<String> = None;
            let mut def_kind: Option<SymbolKind> = None;
            let mut def_range: Option<Range> = None;
            let mut fallback_text: Option<String> = None;
            let mut fallback_range: Option<Range> = None;

            for cap in m.captures {
                let kind_opt = compiled.def_kinds.get(cap.index as usize).copied().flatten();
                if let Some(kind) = kind_opt {
                    def_kind = Some(kind);
                    def_range = Some(range_from_ts_node(cap.node));
                    if let Ok(text) = cap.node.utf8_text(&source) {
                        fallback_text = Some(text.to_string());
                        fallback_range = Some(range_from_ts_node(cap.node));
                    }
                } else if Some(cap.index) == compiled.name_idx {
                    if let Ok(text) = cap.node.utf8_text(&source) {
                        name_text = Some(text.to_string());
                    }
                    if def_range.is_none() {
                        def_range = Some(range_from_ts_node(cap.node));
                    }
                }
            }

            let Some(kind) = def_kind else { continue };
            let (name, range) = match (name_text, fallback_text) {
                (Some(n), _) => (n, def_range.unwrap_or_default()),
                (None, Some(n)) => (n, fallback_range.unwrap_or_default()),
                _ => continue,
            };
            out.push(IndexEntry {
                name,
                language: lang.name.clone(),
                kind,
                file: path.to_path_buf(),
                range,
            });
        }
        promote_methods(&mut out);
        Ok(out)
    }

    fn compiled_query(
        &self,
        lang: &LanguageInfo,
        language: &tree_sitter::Language,
    ) -> Option<std::sync::Arc<CompiledQuery>> {
        {
            let guard = self.queries.lock().unwrap();
            if let Some(q) = guard.get(&lang.name) {
                return Some(std::sync::Arc::clone(q));
            }
        }
        let runtime = self.registry.runtime();
        let source = load_tags_query(runtime, &lang.name)?;
        let query = match Query::new(language, &source) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!("compile tags query for {}: {}", lang.name, e);
                return None;
            }
        };
        let capture_names = query.capture_names().to_vec();
        let def_kinds: Vec<Option<SymbolKind>> = capture_names
            .iter()
            .map(|n| definition_kind_from_capture(n))
            .collect();
        let name_idx = capture_names
            .iter()
            .position(|n| n.trim_start_matches('@') == "name")
            .map(|i| i as u32);

        // Skip languages whose queries don't yield any definition captures.
        if def_kinds.iter().all(|k| k.is_none()) {
            return None;
        }

        let compiled = std::sync::Arc::new(CompiledQuery {
            query,
            def_kinds,
            name_idx,
        });
        let mut guard = self.queries.lock().unwrap();
        Some(std::sync::Arc::clone(
            guard
                .entry(lang.name.clone())
                .or_insert_with(|| std::sync::Arc::clone(&compiled)),
        ))
    }
}
