//! Walking a workspace and parsing its files into an index of the definitions
//! they hold.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use parking_lot::Mutex;
use streaming_iterator::StreamingIterator;
use tower_lsp::lsp_types::{Position, Range, SymbolKind, Url};
use tree_sitter::{Parser, Query, QueryCursor};

use crate::grammars::{GrammarCache, GrammarError};
use crate::languages::{LanguageInfo, LanguageRegistry, load_tags_query};

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub name: String,
    pub language: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub range: Range,
    /// Just the name within `range`; equal to `range` when the tags query
    /// captured no `@name`.
    pub name_range: Range,
    /// Name of the innermost class-like definition in the same file whose range
    /// contains this one; `None` at file scope.
    pub container: Option<String>,
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
                if let Some(dropped) = drop_mask.get_mut(idx) {
                    *dropped = true;
                }
            }
            let mut remapped: HashMap<usize, usize> = HashMap::new();
            let mut kept = Vec::with_capacity(self.entries.len());
            for ((i, entry), dropped) in self.entries.drain(..).enumerate().zip(drop_mask) {
                if !dropped {
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

/// Whether a definition of `kind` can hold methods of its own.
pub fn can_contain_methods(kind: SymbolKind) -> bool {
    kind == SymbolKind::CLASS
        || kind == SymbolKind::STRUCT
        || kind == SymbolKind::INTERFACE
        || kind == SymbolKind::ENUM
}

/// Whether `inner` lies within `outer`. A range does not contain an equal range.
pub fn range_contains(outer: &Range, inner: &Range) -> bool {
    let start = |r: &Range| (r.start.line, r.start.character);
    let end = |r: &Range| (r.end.line, r.end.character);
    start(outer) <= start(inner) && end(inner) <= end(outer) && outer != inner
}

fn attach_containers(entries: &mut [IndexEntry]) {
    let containers: Vec<(Range, String)> = entries
        .iter()
        .filter(|e| can_contain_methods(e.kind))
        .map(|e| (e.range, e.name.clone()))
        .collect();
    for entry in entries.iter_mut() {
        let innermost = containers
            .iter()
            .filter(|(range, _)| range_contains(range, &entry.range))
            .max_by_key(|(range, _)| (range.start.line, range.start.character));
        if let Some((_, name)) = innermost {
            entry.container = Some(name.clone());
            if entry.kind == SymbolKind::FUNCTION {
                entry.kind = SymbolKind::METHOD;
            }
        }
    }
}

pub(crate) fn range_from_ts_node(node: tree_sitter::Node) -> Range {
    Range {
        start: position_from_ts_point(node.start_position()),
        end: position_from_ts_point(node.end_position()),
    }
}

fn position_from_ts_point(point: tree_sitter::Point) -> Position {
    Position {
        line: u32::try_from(point.row).unwrap_or(u32::MAX),
        character: u32::try_from(point.column).unwrap_or(u32::MAX),
    }
}

/// Every regular file under `root` that the workspace holds, in walk order.
///
/// Dotfiles, symlinks and anything a repo's own `.gitignore` or
/// `.git/info/exclude` names (build outputs, `node_modules/`) are skipped. The
/// *global* gitignore (`~/.gitignore`, `~/.config/git/ignore`) is deliberately
/// not honoured: users often set a catch-all `*` there to keep ad-hoc files out
/// of `git status`, which would make this walk return zero files.
pub fn source_files(root: &Path) -> impl Iterator<Item = PathBuf> {
    WalkBuilder::new(root)
        .hidden(true)
        .follow_links(false)
        .git_global(false)
        .git_exclude(true)
        .parents(false)
        .build()
        .filter_map(|result| result.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|entry| entry.into_path())
}

pub struct Indexer {
    registry: LanguageRegistry,
    grammars: GrammarCache,
    /// Cached (parsed Query, capture indices) per language. A `Query` is tied
    /// to a `Language`, which lives for the process lifetime via `GrammarCache`.
    queries: Mutex<HashMap<String, Arc<CompiledQuery>>>,
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
            queries: Mutex::new(HashMap::new()),
        }
    }

    pub fn registry(&self) -> &LanguageRegistry {
        &self.registry
    }

    /// Parse `source` with `lang`'s grammar. `None` when the grammar will not
    /// load or the parse fails.
    pub fn parse(&self, lang: &LanguageInfo, source: &[u8]) -> Option<tree_sitter::Tree> {
        let grammar = self.grammars.get_or_load(&self.registry, lang).ok()?;
        let mut parser = Parser::new();
        parser.set_language(&grammar.language).ok()?;
        parser.parse(source, None)
    }

    /// Index the whole workspace rooted at `root`, returning a fresh `Index`.
    pub fn index_root(&self, root: &Path) -> Index {
        let mut index = Index::default();
        let mut count_files = 0usize;
        for path in source_files(root) {
            let Some(lang) = self.registry.resolve(&path) else {
                continue;
            };
            match self.index_file(&path, lang) {
                Ok(entries) => {
                    if !entries.is_empty() {
                        index.replace_file(&path, entries);
                    }
                    count_files += 1;
                }
                Err(e) => {
                    let error: &dyn std::error::Error = &e;
                    tracing::debug!(path = %path.display(), error, "indexing a file failed");
                }
            }
        }
        tracing::info!(
            files = count_files,
            symbols = index.entries.len(),
            root = %root.display(),
            "indexed workspace"
        );
        index
    }

    /// Parse a single file and return its definitions (class-like, functions, methods). Returns an
    /// empty Vec on failure modes that aren't worth surfacing (missing query,
    /// unreadable file); an error only when `lang`'s grammar does not load.
    pub fn index_file(
        &self,
        path: &Path,
        lang: &LanguageInfo,
    ) -> Result<Vec<IndexEntry>, GrammarError> {
        let source = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return Ok(Vec::new()),
        };
        // Skip obviously-binary files.
        if source.contains(&0u8) {
            return Ok(Vec::new());
        }

        let grammar = self.grammars.get_or_load(&self.registry, lang)?;

        let compiled = match self.compiled_query(lang, &grammar.language) {
            Some(q) => q,
            None => return Ok(Vec::new()),
        };

        let Some(tree) = self.parse(lang, &source) else {
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
            let mut name_range_found: Option<Range> = None;
            let mut def_kind: Option<SymbolKind> = None;
            let mut def_range: Option<Range> = None;
            let mut fallback_text: Option<String> = None;
            let mut fallback_range: Option<Range> = None;

            for cap in m.captures {
                let kind_opt = usize::try_from(cap.index)
                    .ok()
                    .and_then(|i| compiled.def_kinds.get(i))
                    .copied()
                    .flatten();
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
                    name_range_found = Some(range_from_ts_node(cap.node));
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
                name_range: name_range_found.unwrap_or(range),
                container: None,
            });
        }
        attach_containers(&mut out);
        Ok(out)
    }

    fn compiled_query(
        &self,
        lang: &LanguageInfo,
        language: &tree_sitter::Language,
    ) -> Option<Arc<CompiledQuery>> {
        if let Some(q) = self.queries.lock().get(&lang.name) {
            return Some(Arc::clone(q));
        }
        let runtime = self.registry.runtime();
        let source = load_tags_query(runtime, &lang.name)?;
        let query = match Query::new(language, &source) {
            Ok(q) => q,
            Err(e) => {
                let error: &dyn std::error::Error = &e;
                tracing::warn!(language = %lang.name, error, "tags query does not compile");
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
            .and_then(|i| u32::try_from(i).ok());

        // Skip languages whose queries don't yield any definition captures.
        if def_kinds.iter().all(|k| k.is_none()) {
            return None;
        }

        let compiled = Arc::new(CompiledQuery {
            query,
            def_kinds,
            name_idx,
        });
        Some(Arc::clone(
            self.queries
                .lock()
                .entry(lang.name.clone())
                .or_insert(compiled),
        ))
    }
}
