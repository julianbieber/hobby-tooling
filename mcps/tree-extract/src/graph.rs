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

use serde::{Deserialize, Serialize};

use crate::containment;
use crate::extractor::{ExtractResult, Import, RefKind, SymbolKind};

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
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
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
    let parents = containment::parent_of_each(symbols);
    for (i, parent) in parents.iter().enumerate() {
        match parent {
            Some(p) => edges.add(
                sym_ids[*p].clone(),
                sym_ids[i].clone(),
                EdgeKind::Contains,
                false,
            ),
            None => edges.add(
                rel_of(&symbols[i].file),
                sym_ids[i].clone(),
                EdgeKind::Contains,
                false,
            ),
        }
    }

    // --- Name → candidate-index maps for resolution ---
    let mut call_targets: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut impl_targets: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, s) in symbols.iter().enumerate() {
        call_targets.entry(s.name.as_str()).or_default().push(i);
        if s.kind.is_container() || s.kind == SymbolKind::Type {
            impl_targets.entry(s.name.as_str()).or_default().push(i);
        }
    }

    // Symbols bucketed by file for enclosing-definition lookup.
    let mut by_file: HashMap<&Path, Vec<usize>> = HashMap::new();
    for (i, s) in symbols.iter().enumerate() {
        by_file.entry(s.file.as_path()).or_default().push(i);
    }
    let enclosing_idx = |file: &Path, line: u32| -> Option<usize> {
        by_file.get(file).and_then(|idxs| {
            idxs.iter()
                .copied()
                .filter(|&i| symbols[i].line_start <= line && line <= symbols[i].line_end)
                .min_by_key(|&i| symbols[i].line_end.saturating_sub(symbols[i].line_start))
        })
    };

    let mut unresolved_calls = 0usize;

    // --- Calls / Implements edges from references ---
    for r in &extract.references {
        let from_id = match enclosing_idx(&r.file, r.line) {
            Some(i) => sym_ids[i].clone(),
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
        let same_file: Vec<usize> = cands
            .iter()
            .copied()
            .filter(|&i| symbols[i].file == r.file)
            .collect();
        if same_file.len() == 1 {
            edges.add(from_id, sym_ids[same_file[0]].clone(), kind, false);
        } else if cands.len() == 1 {
            edges.add(from_id, sym_ids[cands[0]].clone(), kind, false);
        } else {
            for &i in cands {
                edges.add(from_id.clone(), sym_ids[i].clone(), kind, true);
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

// ---------------------------------------------------------------------------
// Import resolution (best-effort path-suffix matching)
// ---------------------------------------------------------------------------

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
        for start in 0..segs.len() {
            let key = segs[start..].join("/");
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
    match idx.get(&key) {
        Some(v) if v.len() == 1 => ImportResolution::Unique(v[0].clone()),
        Some(v) if v.len() > 1 => ImportResolution::Ambiguous(v.clone()),
        _ => ImportResolution::External,
    }
}

/// Resolve a Python/TS-style relative import against the importer's directory.
fn resolve_relative(module: &str, importer_dir: &str) -> String {
    let leading_dots = module.bytes().take_while(|&b| b == b'.').count();
    let rest = module[leading_dots..].trim_start_matches('/');

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

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

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
    use crate::extractor::{Reference, Symbol};

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
