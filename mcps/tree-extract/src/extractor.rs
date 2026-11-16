//! Walking a repository with tree-sitter and pulling out its definitions,
//! references and imports.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use parking_lot::Mutex;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use treesitter_class_lsp::grammars::{GrammarCache, GrammarError};
use treesitter_class_lsp::languages::{
    LanguageInfo, LanguageRegistry, RegistryError, load_tags_query,
};

use crate::builtin_queries::builtin_graph_query;
use crate::docstring::extract_docstring;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Class,
    Struct,
    Interface,
    Enum,
    Trait,
    Type,
    Function,
    Method,
}

impl SymbolKind {
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Interface => "interface",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Type => "type",
            SymbolKind::Function => "fn",
            SymbolKind::Method => "method",
        }
    }

    pub fn is_container(self) -> bool {
        matches!(
            self,
            SymbolKind::Class
                | SymbolKind::Struct
                | SymbolKind::Interface
                | SymbolKind::Enum
                | SymbolKind::Trait
        )
    }
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub language: String,
    pub file: PathBuf,
    /// 1-based start line (inclusive).
    pub line_start: u32,
    /// 1-based end line (inclusive).
    pub line_end: u32,
    pub docstring: Option<String>,
}

/// A use-site of some named symbol: a function/method call, an instantiation
/// (`new X()`), or an inheritance clause (`extends`/`implements`). Resolved to
/// a definition by name during graph building. `line` is the 1-based line of
/// the reference node (the call site / heritage clause), which is what
/// attributes the reference to its enclosing definition.
#[derive(Debug, Clone)]
pub struct Reference {
    pub name: String,
    pub ref_kind: RefKind,
    pub language: String,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    /// A function or method call.
    Call,
    /// An instantiation (`new X()`) — treated as a call to the type.
    Class,
    /// An inheritance clause (`extends`/`implements`/`with`).
    Implementation,
}

/// A module-level import. `module` is the raw module path/string; `symbol` is
/// the specific imported item when the grammar exposes it. Resolved to an
/// in-repo file (best-effort, by path suffix) during graph building.
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub symbol: Option<String>,
    pub language: String,
    pub file: PathBuf,
    pub line: u32,
}

/// Everything extracted from one repo walk.
#[derive(Debug, Clone, Default)]
pub struct ExtractResult {
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub imports: Vec<Import>,
}

/// Helix's language registry, which says how to parse each file, is unavailable.
#[derive(Debug, thiserror::Error)]
#[error("cannot load the helix language registry (set HELIX_RUNTIME if it is missing)")]
pub struct RegistryUnavailable(#[from] RegistryError);

#[derive(Debug, thiserror::Error)]
enum FileError {
    #[error("cannot load the {lang} grammar")]
    Grammar {
        lang: String,
        #[source]
        source: GrammarError,
    },
    #[error("the {lang} grammar is incompatible with this tree-sitter")]
    Language {
        lang: String,
        #[source]
        source: tree_sitter::LanguageError,
    },
}

/// The 1-based line of a 0-based tree-sitter row.
fn line_of(row: usize) -> u32 {
    u32::try_from(row).unwrap_or(u32::MAX).saturating_add(1)
}

fn kind_from_capture(name: &str) -> Option<SymbolKind> {
    let name = name.strip_prefix('@').unwrap_or(name);
    match name {
        "definition.class" | "defintion.class" => Some(SymbolKind::Class),
        "definition.struct" => Some(SymbolKind::Struct),
        "definition.interface" => Some(SymbolKind::Interface),
        "definition.trait" => Some(SymbolKind::Trait),
        "definition.enum" => Some(SymbolKind::Enum),
        "definition.type" => Some(SymbolKind::Type),
        "definition.function" => Some(SymbolKind::Function),
        "definition.method" => Some(SymbolKind::Method),
        _ => None,
    }
}

fn ref_kind_from_capture(name: &str) -> Option<RefKind> {
    let name = name.strip_prefix('@').unwrap_or(name);
    match name {
        "reference.call" => Some(RefKind::Call),
        "reference.class" => Some(RefKind::Class),
        "reference.implementation" => Some(RefKind::Implementation),
        _ => None,
    }
}

/// A compiled tree-sitter query plus the per-capture-index lookup tables we
/// need to turn matches into symbols / references / imports.
struct CompiledQuery {
    query: Query,
    def_kinds: Vec<Option<SymbolKind>>,
    ref_kinds: Vec<Option<RefKind>>,
    name_idx: Option<u32>,
    import_module_idx: Option<u32>,
    import_symbol_idx: Option<u32>,
}

impl CompiledQuery {
    /// Compile `source` against `language`, building the capture tables.
    /// Returns `None` if compilation fails or the query captures nothing we
    /// care about (so we don't cache and re-run useless queries per file).
    fn compile(source: &str, language: &tree_sitter::Language, lang_name: &str) -> Option<Self> {
        let query = match Query::new(language, source) {
            Ok(q) => q,
            Err(e) => {
                let error: &dyn std::error::Error = &e;
                tracing::warn!(error, lang = lang_name, "built-in query does not compile");
                return None;
            }
        };
        let capture_names = query.capture_names().to_vec();
        let def_kinds: Vec<Option<SymbolKind>> =
            capture_names.iter().map(|n| kind_from_capture(n)).collect();
        let ref_kinds: Vec<Option<RefKind>> = capture_names
            .iter()
            .map(|n| ref_kind_from_capture(n))
            .collect();
        let idx_of = |want: &str| {
            capture_names
                .iter()
                .position(|n| n.trim_start_matches('@') == want)
                .and_then(|i| u32::try_from(i).ok())
        };
        let name_idx = idx_of("name");
        let import_module_idx = idx_of("import.module");
        let import_symbol_idx = idx_of("import.symbol");

        let useful = def_kinds.iter().any(|k| k.is_some())
            || ref_kinds.iter().any(|k| k.is_some())
            || import_module_idx.is_some();
        if !useful {
            return None;
        }

        Some(CompiledQuery {
            query,
            def_kinds,
            ref_kinds,
            name_idx,
            import_module_idx,
            import_symbol_idx,
        })
    }
}

/// The two queries we run per language: helix's `tags.scm` (definitions, plus
/// references where helix provides them) and our supplementary built-in graph
/// query (imports everywhere, plus calls/inheritance for the languages helix
/// tags omit). Either may be absent.
#[derive(Default)]
struct LangQueries {
    tags: Option<Arc<CompiledQuery>>,
    graph: Option<Arc<CompiledQuery>>,
}

pub struct Extractor {
    registry: LanguageRegistry,
    grammars: GrammarCache,
    queries: Mutex<HashMap<String, Arc<LangQueries>>>,
}

impl Extractor {
    pub fn new() -> Result<Self, RegistryUnavailable> {
        let registry = LanguageRegistry::load()?;
        Ok(Self {
            registry,
            grammars: GrammarCache::new(),
            queries: Mutex::new(HashMap::new()),
        })
    }

    pub fn extract_repo(&self, root: &Path) -> ExtractResult {
        let mut result = ExtractResult::default();
        let mut walker = WalkBuilder::new(root);
        walker
            .hidden(true)
            .follow_links(false)
            .git_global(false)
            .git_ignore(true)
            .git_exclude(true)
            .parents(true);
        // Always-skip patterns for common build/output dirs (defensive — in case
        // the repo's own .gitignore is missing them).
        walker.filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                "target"
                    | "node_modules"
                    | "dist"
                    | "build"
                    | ".git"
                    | ".venv"
                    | "venv"
                    | "__pycache__"
            )
        });
        for entry_result in walker.build() {
            let Ok(entry) = entry_result else { continue };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let Some(lang) = self.registry.resolve(path) else {
                continue;
            };
            match self.extract_file(path, lang) {
                Ok(mut r) => {
                    result.symbols.append(&mut r.symbols);
                    result.references.append(&mut r.references);
                    result.imports.append(&mut r.imports);
                }
                Err(e) => {
                    let error: &dyn std::error::Error = &e;
                    tracing::debug!(error, path = %path.display(), "extraction failed");
                }
            }
        }
        result.symbols.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line_start.cmp(&b.line_start))
                .then(a.name.cmp(&b.name))
        });
        result.references.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.name.cmp(&b.name))
        });
        result.imports.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.module.cmp(&b.module))
        });
        result
    }

    fn extract_file(&self, path: &Path, lang: &LanguageInfo) -> Result<ExtractResult, FileError> {
        let source = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => return Ok(ExtractResult::default()),
        };
        if source.contains(&0u8) {
            return Ok(ExtractResult::default());
        }

        let grammar = self
            .grammars
            .get_or_load(&self.registry, lang)
            .map_err(|source| FileError::Grammar {
                lang: lang.name.clone(),
                source,
            })?;
        let queries = self.lang_queries(lang, &grammar.language);
        if queries.tags.is_none() && queries.graph.is_none() {
            return Ok(ExtractResult::default());
        }

        let mut parser = Parser::new();
        parser
            .set_language(&grammar.language)
            .map_err(|source| FileError::Language {
                lang: lang.name.clone(),
                source,
            })?;
        let Some(tree) = parser.parse(&source, None) else {
            return Ok(ExtractResult::default());
        };

        let source_str = match std::str::from_utf8(&source) {
            Ok(s) => s,
            Err(_) => return Ok(ExtractResult::default()),
        };

        let mut result = ExtractResult::default();
        if let Some(q) = queries.tags.as_ref() {
            self.run_query(q, &tree, &source, source_str, path, lang, &mut result);
        }
        if let Some(q) = queries.graph.as_ref() {
            self.run_query(q, &tree, &source, source_str, path, lang, &mut result);
        }
        Ok(result)
    }

    /// Run one compiled query over the parsed tree, appending any symbols /
    /// references / imports it matches into `out`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the shared parse context of one file; bundling it adds no clarity"
    )]
    fn run_query(
        &self,
        compiled: &CompiledQuery,
        tree: &tree_sitter::Tree,
        source: &[u8],
        source_str: &str,
        path: &Path,
        lang: &LanguageInfo,
        out: &mut ExtractResult,
    ) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&compiled.query, tree.root_node(), source);
        while let Some(m) = matches.next() {
            let mut def_kind: Option<SymbolKind> = None;
            let mut def_node: Option<tree_sitter::Node> = None;
            let mut ref_kind: Option<RefKind> = None;
            let mut ref_node: Option<tree_sitter::Node> = None;
            let mut name_text: Option<String> = None;
            let mut import_module: Option<String> = None;
            let mut import_symbol: Option<String> = None;

            for cap in m.captures {
                let i = usize::try_from(cap.index).ok();
                if let Some(kind) = i.and_then(|i| compiled.def_kinds.get(i).copied().flatten()) {
                    def_kind = Some(kind);
                    def_node = Some(cap.node);
                } else if let Some(rk) =
                    i.and_then(|i| compiled.ref_kinds.get(i).copied().flatten())
                {
                    ref_kind = Some(rk);
                    ref_node = Some(cap.node);
                } else if Some(cap.index) == compiled.name_idx
                    && let Ok(text) = cap.node.utf8_text(source)
                {
                    name_text = Some(text.to_string());
                } else if Some(cap.index) == compiled.import_module_idx
                    && let Ok(text) = cap.node.utf8_text(source)
                {
                    import_module = Some(strip_quotes(text).to_string());
                } else if Some(cap.index) == compiled.import_symbol_idx
                    && let Ok(text) = cap.node.utf8_text(source)
                {
                    import_symbol = Some(strip_quotes(text).to_string());
                }
            }

            // Definition match.
            if let (Some(kind), Some(dnode)) = (def_kind, def_node) {
                // Two tags-query shapes produce a definition:
                //   (a) `(decl name: (id) @name) @definition.x` — `@definition`
                //       sits on the whole declaration node and a separate
                //       `@name` carries the identifier (Python, Go, TS/JS,
                //       Scala). `dnode` already spans the body.
                //   (b) `(decl name: (id) @definition.x)` — `@definition` sits
                //       directly on the name identifier with no `@name` (Rust,
                //       Java). `dnode` is then a single-line name node, so its
                //       end row would collapse the symbol to one line. Climb to
                //       the parent declaration for an accurate `line_end`;
                //       otherwise containment nesting and call-site attribution
                //       (which both rely on line ranges) silently break.
                let (name, range_node) = match name_text.clone() {
                    Some(n) => (n, dnode),
                    None => {
                        let name = match dnode.utf8_text(source) {
                            Ok(t) if !t.contains('\n') => t.to_string(),
                            _ => continue,
                        };
                        (name, dnode.parent().unwrap_or(dnode))
                    }
                };
                out.symbols.push(Symbol {
                    name,
                    kind,
                    language: lang.name.clone(),
                    file: path.to_path_buf(),
                    // Start at the name's line (stable node ids, points at the
                    // declaration); end at the declaration body's last line.
                    line_start: line_of(dnode.start_position().row),
                    line_end: line_of(range_node.end_position().row),
                    docstring: extract_docstring(source_str, &lang.name, dnode),
                });
                continue;
            }

            // Reference match (call / instantiation / inheritance).
            if let (Some(rk), Some(rnode), Some(name)) = (ref_kind, ref_node, name_text) {
                out.references.push(Reference {
                    name,
                    ref_kind: rk,
                    language: lang.name.clone(),
                    file: path.to_path_buf(),
                    line: line_of(rnode.start_position().row),
                });
                continue;
            }

            // Import match.
            if let Some(module) = import_module {
                if module.is_empty() {
                    continue;
                }
                // import line: prefer the module node's position via the match;
                // fall back to scanning the first capture's row.
                let line = m
                    .captures
                    .iter()
                    .map(|c| line_of(c.node.start_position().row))
                    .min()
                    .unwrap_or(1);
                out.imports.push(Import {
                    module,
                    symbol: import_symbol,
                    language: lang.name.clone(),
                    file: path.to_path_buf(),
                    line,
                });
            }
        }
    }

    fn lang_queries(
        &self,
        lang: &LanguageInfo,
        language: &tree_sitter::Language,
    ) -> Arc<LangQueries> {
        {
            let guard = self.queries.lock();
            if let Some(q) = guard.get(&lang.name) {
                return Arc::clone(q);
            }
        }
        let runtime = self.registry.runtime();
        let tags = load_tags_query(runtime, &lang.name)
            .and_then(|src| CompiledQuery::compile(&src, language, &lang.name))
            .map(Arc::new);
        let graph = builtin_graph_query(&lang.name)
            .and_then(|src| CompiledQuery::compile(src, language, &lang.name))
            .map(Arc::new);

        let entry = Arc::new(LangQueries { tags, graph });
        let mut guard = self.queries.lock();
        Arc::clone(
            guard
                .entry(lang.name.clone())
                .or_insert_with(|| Arc::clone(&entry)),
        )
    }
}

/// Strip a single layer of surrounding quotes (Go/TS import strings come with
/// their quotes attached) and any leading/trailing whitespace.
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .unwrap_or(s)
}
