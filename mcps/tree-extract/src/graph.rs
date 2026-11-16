//! The persisted code graph: symbol/file nodes plus `Calls` / `Imports` /
//! `Implements` / `Contains` edges, built from one [`ExtractResult`].
//!
//! # Resolution and its limitations
//!
//! Edges are resolved by **name only** — there is no scope, type, or
//! import-aware binding (the same deterministic-AST limitation graphify has):
//!
//! - A call `foo()` resolves to *every* definition named `foo`. If exactly one
//!   candidate lives in the calling file it wins unambiguously; otherwise every
//!   candidate gets an edge flagged [`Edge::ambiguous`].
//! - Calls to external/stdlib symbols (no in-repo definition) produce no edge;
//!   they are counted in [`GraphStats::unresolved_calls`].
//! - Method calls `obj.method()` resolve on the bare method name only — the
//!   receiver's type is not considered.
//! - Inheritance (`extends`/`implements`) is attributed to the enclosing class
//!   because the heritage clause sits inside the class's line range; Rust trait
//!   `impl` blocks are *not* modeled (Rust has no class inheritance).
//! - Import resolution is best-effort path-suffix matching: it resolves Python
//!   dotted paths and relative TS/JS imports well, but package-qualified
//!   Java/Scala imports (where package ≠ path) usually fall through to
//!   "external" and are only counted in [`GraphStats::external_imports`].
//! - Containment is range-based (same heuristic the renderer uses), so an
//!   unusual/macro-generated line range can mis-nest a symbol.
//!
//! Nodes and edges are sorted deterministically so the serialized JSON is
//! stable across runs (clean git diffs).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::containment;
use crate::extractor::{ExtractResult, Import, RefKind, Symbol, SymbolKind};

/// A persisted graph file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum GraphFileError {
    #[error("cannot read code graph {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("code graph {} is not valid graph JSON", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot serialize the code graph")]
    Serialize(#[source] serde_json::Error),
    #[error("cannot write code graph {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// A node or edge kind named by a caller that is not one of the known kinds.
#[derive(Debug, thiserror::Error)]
#[error("unknown {what} {value:?}; expected one of {expected}")]
pub struct UnknownKind {
    pub what: &'static str,
    pub value: String,
    pub expected: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    Class,
    Struct,
    Interface,
    Enum,
    Trait,
    Type,
    Function,
    Method,
}

impl NodeKind {
    fn from_symbol(k: SymbolKind) -> Self {
        match k {
            SymbolKind::Class => NodeKind::Class,
            SymbolKind::Struct => NodeKind::Struct,
            SymbolKind::Interface => NodeKind::Interface,
            SymbolKind::Enum => NodeKind::Enum,
            SymbolKind::Trait => NodeKind::Trait,
            SymbolKind::Type => NodeKind::Type,
            SymbolKind::Function => NodeKind::Function,
            SymbolKind::Method => NodeKind::Method,
        }
    }
}

/// Case-insensitive; `fn` is accepted for `function`.
impl FromStr for NodeKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "file" => Ok(NodeKind::File),
            "class" => Ok(NodeKind::Class),
            "struct" => Ok(NodeKind::Struct),
            "interface" => Ok(NodeKind::Interface),
            "enum" => Ok(NodeKind::Enum),
            "trait" => Ok(NodeKind::Trait),
            "type" => Ok(NodeKind::Type),
            "function" | "fn" => Ok(NodeKind::Function),
            "method" => Ok(NodeKind::Method),
            _ => Err(UnknownKind {
                what: "node kind",
                value: s.to_string(),
                expected: "file, class, struct, interface, enum, trait, type, function, method",
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// `relpath:Lstart` for symbols, bare `relpath` for files.
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    /// Repo-relative, forward-slash normalized.
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Calls,
    Imports,
    Implements,
    Contains,
}

/// Case-insensitive; the singular (`call`, `import`, …) is accepted too.
impl FromStr for EdgeKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "calls" | "call" => Ok(EdgeKind::Calls),
            "imports" | "import" => Ok(EdgeKind::Imports),
            "implements" | "implement" => Ok(EdgeKind::Implements),
            "contains" | "contain" => Ok(EdgeKind::Contains),
            _ => Err(UnknownKind {
                what: "edge kind",
                value: s.to_string(),
                expected: "calls, imports, implements, contains",
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    /// True when the target was resolved by name to more than one candidate.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ambiguous: bool,
    /// Number of source-level occurrences collapsed into this edge.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub count: u32,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn one() -> u32 {
    1
}
fn is_one(n: &u32) -> bool {
    *n == 1
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphStats {
    pub nodes: usize,
    pub edges: usize,
    /// Call/instantiation references with no in-repo definition (external/stdlib).
    pub unresolved_calls: usize,
    /// Edges whose target was resolved to multiple same-named candidates.
    pub ambiguous_edges: usize,
    /// Imports whose module did not resolve to an in-repo file.
    pub external_imports: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraph {
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    pub generator: String,
    pub stats: GraphStats,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl CodeGraph {
    /// Load a previously-written graph from JSON.
    pub fn load(path: &Path) -> Result<Self, GraphFileError> {
        let bytes = std::fs::read(path).map_err(|source| GraphFileError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| GraphFileError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Write the graph to `path` as pretty JSON, creating its parent directory.
    pub fn write(&self, path: &Path) -> Result<(), GraphFileError> {
        let json = serde_json::to_string_pretty(self).map_err(GraphFileError::Serialize)?;
        let write_error = |source| GraphFileError::Write {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(write_error)?;
        }
        std::fs::write(path, json).map_err(write_error)
    }
}

/// Accumulates edges, collapsing duplicate `(from, to, kind)` triples. An edge
/// is only "ambiguous" if *every* contributing resolution was ambiguous.
#[derive(Default)]
struct EdgeSet {
    map: HashMap<(String, String, EdgeKind), (bool, u32)>,
}

impl EdgeSet {
    fn add(&mut self, from: String, to: String, kind: EdgeKind, ambiguous: bool) {
        let e = self.map.entry((from, to, kind)).or_insert((ambiguous, 0));
        e.0 = e.0 && ambiguous;
        e.1 += 1;
    }

    fn into_sorted(self) -> Vec<Edge> {
        let mut v: Vec<Edge> = self
            .map
            .into_iter()
            .map(|((from, to, kind), (ambiguous, count))| Edge {
                from,
                to,
                kind,
                ambiguous,
                count,
            })
            .collect();
        v.sort_by(|a, b| {
            a.from
                .cmp(&b.from)
                .then(a.to.cmp(&b.to))
                .then(a.kind.cmp(&b.kind))
        });
        v
    }
}

/// Build a [`CodeGraph`] from one extraction pass.
pub fn build_graph(extract: &ExtractResult, root: &Path, head: Option<String>) -> CodeGraph {
    let symbols = &extract.symbols;

    // Pre-compute the relpath of every distinct file we touch.
    let mut rel: HashMap<PathBuf, String> = HashMap::new();
    for s in symbols {
        rel.entry(s.file.clone())
            .or_insert_with(|| relpath(root, &s.file));
    }
    for r in &extract.references {
        rel.entry(r.file.clone())
            .or_insert_with(|| relpath(root, &r.file));
    }
    for i in &extract.imports {
        rel.entry(i.file.clone())
            .or_insert_with(|| relpath(root, &i.file));
    }
    let rel_of = |p: &Path| -> String { rel.get(p).cloned().unwrap_or_else(|| relpath(root, p)) };

    // Nodes: a File node per distinct file, plus a node per symbol.
    let mut nodes: HashMap<String, Node> = HashMap::new();
    for r in rel.values() {
        nodes.entry(r.clone()).or_insert_with(|| Node {
            id: r.clone(),
            kind: NodeKind::File,
            name: file_name(r),
            file: r.clone(),
            line: None,
            language: None,
            docstring: None,
        });
    }
    let mut sym_ids: Vec<String> = Vec::with_capacity(symbols.len());
    for s in symbols {
        let r = rel_of(&s.file);
        let id = format!("{r}:L{}", s.line_start);
        nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::from_symbol(s.kind),
            name: s.name.clone(),
            file: r,
            line: Some(s.line_start),
            language: Some(s.language.clone()),
            docstring: s.docstring.clone(),
        });
        sym_ids.push(id);
    }

    let mut edges = EdgeSet::default();

    // --- Contains edges (parent → child; file → top-level symbol) ---
    let defs: Vec<Def> = symbols
        .iter()
        .zip(&sym_ids)
        .map(|(symbol, id)| Def { symbol, id })
        .collect();

    let parents = containment::parent_of_each(symbols);
    for (def, parent) in defs.iter().zip(&parents) {
        let from = match parent.and_then(|p| defs.get(p)) {
            Some(container) => container.id.clone(),
            None => rel_of(&def.symbol.file),
        };
        edges.add(from, def.id.clone(), EdgeKind::Contains, false);
    }

    // --- Name → candidate maps for resolution ---
    let mut call_targets: HashMap<&str, Vec<&Def>> = HashMap::new();
    let mut impl_targets: HashMap<&str, Vec<&Def>> = HashMap::new();
    for def in &defs {
        let s = def.symbol;
        call_targets.entry(s.name.as_str()).or_default().push(def);
        if s.kind.is_container() || s.kind == SymbolKind::Type {
            impl_targets.entry(s.name.as_str()).or_default().push(def);
        }
    }

    // Symbols bucketed by file for enclosing-definition lookup.
    let mut by_file: HashMap<&Path, Vec<&Def>> = HashMap::new();
    for def in &defs {
        by_file
            .entry(def.symbol.file.as_path())
            .or_default()
            .push(def);
    }
    let enclosing = |file: &Path, line: u32| -> Option<&Def> {
        by_file.get(file).and_then(|in_file| {
            in_file
                .iter()
                .copied()
                .filter(|d| d.symbol.line_start <= line && line <= d.symbol.line_end)
                .min_by_key(|d| d.symbol.line_end.saturating_sub(d.symbol.line_start))
        })
    };

    let mut unresolved_calls = 0usize;

    // --- Calls / Implements edges from references ---
    for r in &extract.references {
        let from_id = match enclosing(&r.file, r.line) {
            Some(def) => def.id.clone(),
            None => rel_of(&r.file),
        };
        let (candidates, kind) = match r.ref_kind {
            RefKind::Call | RefKind::Class => (call_targets.get(r.name.as_str()), EdgeKind::Calls),
            RefKind::Implementation => (impl_targets.get(r.name.as_str()), EdgeKind::Implements),
        };
        let Some(cands) = candidates else {
            if kind == EdgeKind::Calls {
                unresolved_calls += 1;
            }
            continue;
        };
        // Prefer a unique same-file candidate; else single global; else all (ambiguous).
        let same_file: Vec<&Def> = cands
            .iter()
            .copied()
            .filter(|d| d.symbol.file == r.file)
            .collect();
        match (same_file.as_slice(), cands.as_slice()) {
            ([only], _) | (_, [only]) => edges.add(from_id, only.id.clone(), kind, false),
            _ => {
                for def in cands {
                    edges.add(from_id.clone(), def.id.clone(), kind, true);
                }
            }
        }
    }

    // --- Imports edges (file → in-repo file, best-effort) ---
    let suffix_index = build_suffix_index(rel.values());
    let mut external_imports = 0usize;
    for imp in &extract.imports {
        let from_id = rel_of(&imp.file);
        match resolve_import(imp, &from_id, &suffix_index) {
            ImportResolution::Unique(t) => {
                if t != from_id {
                    edges.add(from_id, t, EdgeKind::Imports, false);
                }
            }
            ImportResolution::Ambiguous(targets) => {
                for t in targets {
                    if t != from_id {
                        edges.add(from_id.clone(), t, EdgeKind::Imports, true);
                    }
                }
            }
            ImportResolution::External => external_imports += 1,
        }
    }

    // --- Materialize, sort, stat ---
    let mut node_vec: Vec<Node> = nodes.into_values().collect();
    node_vec.sort_by(|a, b| a.id.cmp(&b.id));
    let edge_vec = edges.into_sorted();
    let ambiguous_edges = edge_vec.iter().filter(|e| e.ambiguous).count();

    let stats = GraphStats {
        nodes: node_vec.len(),
        edges: edge_vec.len(),
        unresolved_calls,
        ambiguous_edges,
        external_imports,
    };

    CodeGraph {
        root: root.to_string_lossy().replace('\\', "/"),
        head,
        generator: "hobby-tree-extract".to_string(),
        stats,
        nodes: node_vec,
        edges: edge_vec,
    }
}

struct Def<'a> {
    symbol: &'a Symbol,
    id: &'a String,
}

enum ImportResolution {
    Unique(String),
    Ambiguous(Vec<String>),
    External,
}

/// Maps a normalized path-suffix key (e.g. `app/utils`, `utils`) to the file
/// relpath(s) it could refer to.
type SuffixIndex = HashMap<String, Vec<String>>;

fn build_suffix_index<'a>(files: impl Iterator<Item = &'a String>) -> SuffixIndex {
    let unique: HashSet<&String> = files.collect();
    let mut idx: SuffixIndex = HashMap::new();
    for f in unique {
        let stem = f.rsplit_once('.').map(|(a, _)| a).unwrap_or(f);
        let mut segs: Vec<&str> = stem.split('/').collect();
        // Treat index files as standing for their directory.
        if matches!(
            segs.last().copied(),
            Some("mod" | "__init__" | "index" | "package")
        ) {
            segs.pop();
        }
        for tail in (0..segs.len()).filter_map(|start| segs.get(start..)) {
            let key = tail.join("/");
            if !key.is_empty() {
                idx.entry(key).or_default().push(f.clone());
            }
        }
    }
    for v in idx.values_mut() {
        v.sort();
        v.dedup();
    }
    idx
}

fn resolve_import(imp: &Import, importer_rel: &str, idx: &SuffixIndex) -> ImportResolution {
    let module = imp.module.trim();
    if module.is_empty() {
        return ImportResolution::External;
    }
    let key = if module.starts_with('.') {
        let importer_dir = importer_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        resolve_relative(module, importer_dir)
    } else {
        let norm = module.replace("::", "/").replace(['.', '\\'], "/");
        let norm = norm.trim_matches('/');
        norm.strip_prefix("crate/")
            .or_else(|| norm.strip_prefix("self/"))
            .or_else(|| norm.strip_prefix("super/"))
            .unwrap_or(norm)
            .to_string()
    };
    if key.is_empty() {
        return ImportResolution::External;
    }
    match idx.get(&key).map(Vec::as_slice) {
        Some([only]) => ImportResolution::Unique(only.clone()),
        Some(many @ [_, _, ..]) => ImportResolution::Ambiguous(many.to_vec()),
        _ => ImportResolution::External,
    }
}

/// Resolve a Python/TS-style relative import against the importer's directory.
fn resolve_relative(module: &str, importer_dir: &str) -> String {
    let leading_dots = module.bytes().take_while(|&b| b == b'.').count();
    let rest = module.trim_start_matches('.').trim_start_matches('/');

    let up = if module.contains('/') {
        // TS style: count ".." path segments.
        module
            .split('/')
            .take_while(|s| *s == ".." || *s == ".")
            .filter(|s| *s == "..")
            .count()
    } else {
        // Python style: 1 dot = current package, N dots = N-1 levels up.
        leading_dots.saturating_sub(1)
    };

    let mut dir_segs: Vec<&str> = if importer_dir.is_empty() {
        Vec::new()
    } else {
        importer_dir.split('/').collect()
    };
    for _ in 0..up {
        dir_segs.pop();
    }
    let rest_norm = rest.replace("::", "/").replace('.', "/");
    let rest_norm = rest_norm.trim_matches('/');
    let mut out = dir_segs.join("/");
    if !rest_norm.is_empty() {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(rest_norm);
    }
    out
}

fn relpath(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn file_name(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::Reference;

    fn sym(name: &str, kind: SymbolKind, file: &str, start: u32, end: u32) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file: PathBuf::from(file),
            line_start: start,
            line_end: end,
            docstring: None,
        }
    }

    #[test]
    fn contains_and_calls_resolve() {
        let root = Path::new("/repo");
        let symbols = vec![
            sym("Greeter", SymbolKind::Struct, "/repo/a.rs", 1, 10),
            sym("run", SymbolKind::Method, "/repo/a.rs", 3, 8),
            sym("helper", SymbolKind::Function, "/repo/a.rs", 12, 14),
        ];
        let references = vec![Reference {
            name: "helper".to_string(),
            ref_kind: RefKind::Call,
            language: "rust".to_string(),
            file: PathBuf::from("/repo/a.rs"),
            line: 5, // inside run()
        }];
        let extract = ExtractResult {
            symbols,
            references,
            imports: vec![],
        };
        let g = build_graph(&extract, root, None);

        // Contains: file → Greeter, Greeter → run, file → helper.
        assert!(
            g.edges
                .iter()
                .any(|e| e.kind == EdgeKind::Contains && e.from == "a.rs" && e.to == "a.rs:L1")
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.kind == EdgeKind::Contains && e.from == "a.rs:L1" && e.to == "a.rs:L3")
        );
        // Call: run() → helper(), attributed to run (innermost enclosing).
        assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Calls
            && e.from == "a.rs:L3"
            && e.to == "a.rs:L12"
            && !e.ambiguous));
    }

    #[test]
    fn ambiguous_when_two_same_named_targets() {
        let root = Path::new("/repo");
        let symbols = vec![
            sym("caller", SymbolKind::Function, "/repo/a.rs", 1, 5),
            sym("dup", SymbolKind::Function, "/repo/b.rs", 1, 2),
            sym("dup", SymbolKind::Function, "/repo/c.rs", 1, 2),
        ];
        let references = vec![Reference {
            name: "dup".to_string(),
            ref_kind: RefKind::Call,
            language: "rust".to_string(),
            file: PathBuf::from("/repo/a.rs"),
            line: 3,
        }];
        let g = build_graph(
            &ExtractResult {
                symbols,
                references,
                imports: vec![],
            },
            root,
            None,
        );
        let call_edges: Vec<&Edge> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(call_edges.len(), 2, "both candidates edged: {call_edges:?}");
        assert!(call_edges.iter().all(|e| e.ambiguous));
        assert_eq!(g.stats.ambiguous_edges, 2);
    }

    #[test]
    fn unresolved_call_counted_not_edged() {
        let root = Path::new("/repo");
        let symbols = vec![sym("caller", SymbolKind::Function, "/repo/a.rs", 1, 5)];
        let references = vec![Reference {
            name: "println".to_string(),
            ref_kind: RefKind::Call,
            language: "rust".to_string(),
            file: PathBuf::from("/repo/a.rs"),
            line: 3,
        }];
        let g = build_graph(
            &ExtractResult {
                symbols,
                references,
                imports: vec![],
            },
            root,
            None,
        );
        assert_eq!(g.stats.unresolved_calls, 1);
        assert!(!g.edges.iter().any(|e| e.kind == EdgeKind::Calls));
    }

    #[test]
    fn import_resolves_by_suffix() {
        let root = Path::new("/repo");
        let symbols = vec![
            sym("load", SymbolKind::Function, "/repo/app/utils.py", 1, 2),
            sym("main", SymbolKind::Function, "/repo/main.py", 1, 5),
        ];
        let imports = vec![Import {
            module: "app.utils".to_string(),
            symbol: Some("load".to_string()),
            language: "python".to_string(),
            file: PathBuf::from("/repo/main.py"),
            line: 1,
        }];
        let g = build_graph(
            &ExtractResult {
                symbols,
                references: vec![],
                imports,
            },
            root,
            None,
        );
        assert!(
            g.edges.iter().any(|e| e.kind == EdgeKind::Imports
                && e.from == "main.py"
                && e.to == "app/utils.py"),
            "edges: {:?}",
            g.edges
        );
        assert_eq!(g.stats.external_imports, 0);
    }

    #[test]
    fn external_import_counted() {
        let root = Path::new("/repo");
        let symbols = vec![sym("main", SymbolKind::Function, "/repo/main.py", 1, 5)];
        let imports = vec![Import {
            module: "numpy".to_string(),
            symbol: None,
            language: "python".to_string(),
            file: PathBuf::from("/repo/main.py"),
            line: 1,
        }];
        let g = build_graph(
            &ExtractResult {
                symbols,
                references: vec![],
                imports,
            },
            root,
            None,
        );
        assert_eq!(g.stats.external_imports, 1);
        assert!(!g.edges.iter().any(|e| e.kind == EdgeKind::Imports));
    }
}
