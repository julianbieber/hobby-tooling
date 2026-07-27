//! Query engine over a loaded [`CodeGraph`]. Pure data-in/data-out — no rmcp
//! types — so it is unit-testable without spawning an MCP server. The MCP layer
//! is a thin wrapper that serializes these result structs to JSON.
//!
//! All list-returning queries apply default + hard-cap limits and report a
//! `{ok, summary, total, returned, truncated, results}` envelope so an LLM
//! always knows whether it is seeing a subset, without re-reading raw files.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::graph::{CodeGraph, EdgeKind, Node, NodeKind};

// Result-size discipline (see module docs).
const SEARCH_DEFAULT: usize = 30;
const SEARCH_MAX: usize = 100;
const NEIGHBORS_DEFAULT: usize = 50;
const NEIGHBORS_MAX: usize = 200;
const IMPACT_DEFAULT: usize = 100;
const IMPACT_MAX: usize = 300;
const PATH_DEFAULT_DEPTH: usize = 12;
const PATH_MAX_DEPTH: usize = 25;
const IMPACT_DEFAULT_DEPTH: usize = 5;
const IMPACT_MAX_DEPTH: usize = 10;
const DOCSTRING_CLAMP: usize = 280;
const SAMPLE_PER_FILE: usize = 5;

struct Adj {
    node: usize,
    kind: EdgeKind,
    ambiguous: bool,
    count: u32,
}

/// A `CodeGraph` with adjacency indexes built for traversal.
pub struct LoadedGraph {
    graph: CodeGraph,
    id_to_idx: HashMap<String, usize>,
    out_adj: Vec<Vec<Adj>>,
    in_adj: Vec<Vec<Adj>>,
}

impl LoadedGraph {
    pub fn build(graph: CodeGraph) -> Self {
        let id_to_idx: HashMap<String, usize> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let n = graph.nodes.len();
        let mut out_adj: Vec<Vec<Adj>> = (0..n).map(|_| Vec::new()).collect();
        let mut in_adj: Vec<Vec<Adj>> = (0..n).map(|_| Vec::new()).collect();
        for e in &graph.edges {
            if let (Some(&f), Some(&t)) = (id_to_idx.get(&e.from), id_to_idx.get(&e.to)) {
                out_adj[f].push(Adj {
                    node: t,
                    kind: e.kind,
                    ambiguous: e.ambiguous,
                    count: e.count,
                });
                in_adj[t].push(Adj {
                    node: f,
                    kind: e.kind,
                    ambiguous: e.ambiguous,
                    count: e.count,
                });
            }
        }
        Self {
            graph,
            id_to_idx,
            out_adj,
            in_adj,
        }
    }

    pub fn stats(&self) -> &crate::graph::GraphStats {
        &self.graph.stats
    }

    fn node(&self, idx: usize) -> &Node {
        &self.graph.nodes[idx]
    }

    /// Resolve a string that is either a node id (`relpath:Lstart`) or an exact
    /// symbol name. Returns all matching node indices.
    fn resolve(&self, s: &str) -> Vec<usize> {
        if let Some(&i) = self.id_to_idx.get(s) {
            return vec![i];
        }
        self.graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.name == s)
            .map(|(i, _)| i)
            .collect()
    }

    // --- search_symbols -------------------------------------------------

    pub fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        language: Option<&str>,
        limit: Option<usize>,
    ) -> Envelope<SymbolHit> {
        let limit = clamp(limit, SEARCH_DEFAULT, SEARCH_MAX);
        let kind_filter = kind.and_then(node_kind_from_str);

        let candidates: Vec<usize> = self
            .graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| kind_filter.is_none_or(|k| n.kind == k))
            .filter(|(_, n)| language.is_none_or(|l| n.language.as_deref() == Some(l)))
            .map(|(i, _)| i)
            .collect();

        let mut scored: Vec<(usize, Option<u32>)> = if query.trim().is_empty() {
            candidates.into_iter().map(|i| (i, None)).collect()
        } else {
            use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
            use nucleo_matcher::{Config, Matcher, Utf32Str};
            let mut matcher = Matcher::new(Config::DEFAULT);
            let pat = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
            let mut buf = Vec::new();
            candidates
                .into_iter()
                .filter_map(|i| {
                    let hay = Utf32Str::new(&self.graph.nodes[i].name, &mut buf);
                    pat.score(hay, &mut matcher).map(|s| (i, Some(s)))
                })
                .collect()
        };
        // sort by score desc, then name for determinism
        scored.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| self.node(a.0).name.cmp(&self.node(b.0).name))
                .then_with(|| self.node(a.0).id.cmp(&self.node(b.0).id))
        });

        let total = scored.len();
        let truncated = total > limit;
        let results: Vec<SymbolHit> = scored
            .into_iter()
            .take(limit)
            .map(|(i, score)| self.hit(i, score))
            .collect();
        Envelope {
            ok: true,
            summary: format!(
                "{} match{} for {:?}{}",
                total,
                if total == 1 { "" } else { "es" },
                query,
                if truncated {
                    format!(", showing {limit}")
                } else {
                    String::new()
                }
            ),
            returned: results.len(),
            total,
            truncated,
            results,
        }
    }

    fn hit(&self, idx: usize, score: Option<u32>) -> SymbolHit {
        let n = self.node(idx);
        SymbolHit {
            id: n.id.clone(),
            name: n.name.clone(),
            kind: n.kind,
            file: n.file.clone(),
            line: n.line,
            score,
        }
    }

    // --- get_symbol -----------------------------------------------------

    pub fn get_symbol(&self, id: Option<&str>, name: Option<&str>) -> SymbolDetail {
        let needle = id.or(name).unwrap_or("");
        if needle.is_empty() {
            return SymbolDetail::not_found("provide `id` or `name`", vec![]);
        }
        let matches = self.resolve(needle);
        match matches.as_slice() {
            [] => SymbolDetail::not_found(&format!("no node matches {needle:?}"), vec![]),
            [i] => self.detail(*i),
            many => {
                let cands: Vec<String> = many.iter().map(|&i| self.node(i).id.clone()).collect();
                SymbolDetail::not_found(
                    &format!("{} symbols named {needle:?}; pass an `id`", many.len()),
                    cands,
                )
            }
        }
    }

    fn detail(&self, idx: usize) -> SymbolDetail {
        let n = self.node(idx);
        SymbolDetail {
            ok: true,
            found: true,
            id: Some(n.id.clone()),
            name: Some(n.name.clone()),
            kind: Some(n.kind),
            file: Some(n.file.clone()),
            line: n.line,
            language: n.language.clone(),
            docstring: n.docstring.as_deref().map(clamp_doc),
            out_edges: EdgeCounts::tally(&self.out_adj[idx]),
            in_edges: EdgeCounts::tally(&self.in_adj[idx]),
            candidates: vec![],
            summary: format!(
                "{} `{}` at {}{}",
                kind_label(n.kind),
                n.name,
                n.file,
                n.line.map(|l| format!(":L{l}")).unwrap_or_default()
            ),
        }
    }

    // --- get_neighbors --------------------------------------------------

    pub fn neighbors(
        &self,
        id: &str,
        direction: Option<&str>,
        edge_kind: Option<&str>,
        limit: Option<usize>,
    ) -> Envelope<NeighborHit> {
        let limit = clamp(limit, NEIGHBORS_DEFAULT, NEIGHBORS_MAX);
        let dir = direction.unwrap_or("out");
        let kind_filter = edge_kind.and_then(edge_kind_from_str);

        let Some(&start) = self.id_to_idx.get(id) else {
            // allow name resolution to a single node
            let r = self.resolve(id);
            if r.len() != 1 {
                return Envelope::empty(format!("no unique node for {id:?}"));
            }
            return self.neighbors_from(r[0], dir, kind_filter, limit);
        };
        self.neighbors_from(start, dir, kind_filter, limit)
    }

    fn neighbors_from(
        &self,
        start: usize,
        dir: &str,
        kind_filter: Option<EdgeKind>,
        limit: usize,
    ) -> Envelope<NeighborHit> {
        let mut hits: Vec<NeighborHit> = Vec::new();
        if dir == "out" || dir == "both" {
            for a in &self.out_adj[start] {
                if kind_filter.is_none_or(|k| a.kind == k) {
                    hits.push(self.neighbor_hit(a, "out"));
                }
            }
        }
        if dir == "in" || dir == "both" {
            for a in &self.in_adj[start] {
                if kind_filter.is_none_or(|k| a.kind == k) {
                    hits.push(self.neighbor_hit(a, "in"));
                }
            }
        }
        hits.sort_by(|a, b| a.id.cmp(&b.id).then(a.edge_kind.cmp(&b.edge_kind)));
        let total = hits.len();
        let truncated = total > limit;
        hits.truncate(limit);
        Envelope {
            ok: true,
            summary: format!(
                "{} neighbor{} of {} ({dir})",
                total,
                if total == 1 { "" } else { "s" },
                self.node(start).id
            ),
            returned: hits.len(),
            total,
            truncated,
            results: hits,
        }
    }

    fn neighbor_hit(&self, a: &Adj, direction: &'static str) -> NeighborHit {
        let n = self.node(a.node);
        NeighborHit {
            id: n.id.clone(),
            name: n.name.clone(),
            kind: n.kind,
            file: n.file.clone(),
            line: n.line,
            edge_kind: a.kind,
            direction,
            ambiguous: a.ambiguous,
            count: a.count,
        }
    }

    // --- call_path (shortest path over chosen edge kinds) ---------------

    pub fn call_path(
        &self,
        from: &str,
        to: &str,
        edge_kinds: Option<&[String]>,
        max_depth: Option<usize>,
    ) -> PathResult {
        let kinds = parse_edge_kinds(edge_kinds, &[EdgeKind::Calls]);
        let max_depth = max_depth.unwrap_or(PATH_DEFAULT_DEPTH).min(PATH_MAX_DEPTH);
        let sources = self.resolve(from);
        let targets: HashSet<usize> = self.resolve(to).into_iter().collect();
        if sources.is_empty() || targets.is_empty() {
            return PathResult::none(format!("unresolved endpoint(s) {from:?} → {to:?}"));
        }

        // Multi-source BFS with parent pointers.
        let mut parent: HashMap<usize, usize> = HashMap::new();
        let mut depth: HashMap<usize, usize> = HashMap::new();
        let mut queue: VecDeque<usize> = VecDeque::new();
        for &s in &sources {
            depth.insert(s, 0);
            queue.push_back(s);
            if targets.contains(&s) {
                return self.build_path(s, &parent);
            }
        }
        while let Some(cur) = queue.pop_front() {
            let d = depth[&cur];
            if d >= max_depth {
                continue;
            }
            for a in &self.out_adj[cur] {
                if !kinds.contains(&a.kind) || depth.contains_key(&a.node) {
                    continue;
                }
                parent.insert(a.node, cur);
                depth.insert(a.node, d + 1);
                if targets.contains(&a.node) {
                    return self.build_path(a.node, &parent);
                }
                queue.push_back(a.node);
            }
        }
        PathResult::none(format!(
            "no path {from:?} → {to:?} within depth {max_depth}"
        ))
    }

    fn build_path(&self, end: usize, parent: &HashMap<usize, usize>) -> PathResult {
        let mut chain = vec![end];
        let mut cur = end;
        while let Some(&p) = parent.get(&cur) {
            chain.push(p);
            cur = p;
        }
        chain.reverse();
        let path: Vec<PathNode> = chain.iter().map(|&i| self.path_node(i)).collect();
        PathResult {
            ok: true,
            found: true,
            length: path.len().saturating_sub(1),
            summary: format!("path of {} hop(s)", path.len().saturating_sub(1)),
            path,
        }
    }

    fn path_node(&self, idx: usize) -> PathNode {
        let n = self.node(idx);
        PathNode {
            id: n.id.clone(),
            name: n.name.clone(),
            file: n.file.clone(),
            line: n.line,
        }
    }

    // --- impact_of (reverse reachability) -------------------------------

    pub fn impact(
        &self,
        id: &str,
        edge_kinds: Option<&[String]>,
        max_depth: Option<usize>,
        limit: Option<usize>,
        group_by_file: Option<bool>,
    ) -> ImpactResult {
        let kinds = parse_edge_kinds(edge_kinds, &[EdgeKind::Calls, EdgeKind::Imports]);
        let max_depth = max_depth
            .unwrap_or(IMPACT_DEFAULT_DEPTH)
            .min(IMPACT_MAX_DEPTH);
        let limit = clamp(limit, IMPACT_DEFAULT, IMPACT_MAX);
        let group = group_by_file.unwrap_or(true);

        // Seeds: the node itself, or (for a file) the file node + its symbols.
        let seeds: Vec<usize> = match self.id_to_idx.get(id) {
            Some(&i) if self.node(i).kind == NodeKind::File => {
                let file = self.node(i).file.clone();
                let mut s = vec![i];
                for (j, n) in self.graph.nodes.iter().enumerate() {
                    if j != i && n.file == file && n.kind != NodeKind::File {
                        s.push(j);
                    }
                }
                s
            }
            Some(&i) => vec![i],
            None => self.resolve(id),
        };
        if seeds.is_empty() {
            return ImpactResult::empty(format!("unresolved {id:?}"));
        }

        // Reverse BFS, tracking minimum depth per dependent.
        let seed_set: HashSet<usize> = seeds.iter().copied().collect();
        let mut depth: HashMap<usize, usize> = HashMap::new();
        let mut queue: VecDeque<usize> = VecDeque::new();
        for &s in &seeds {
            depth.insert(s, 0);
            queue.push_back(s);
        }
        while let Some(cur) = queue.pop_front() {
            let d = depth[&cur];
            if d >= max_depth {
                continue;
            }
            for a in &self.in_adj[cur] {
                if !kinds.contains(&a.kind) || depth.contains_key(&a.node) {
                    continue;
                }
                depth.insert(a.node, d + 1);
                queue.push_back(a.node);
            }
        }

        // Dependents = reached nodes minus the seeds.
        let mut deps: Vec<(usize, usize)> = depth
            .iter()
            .filter(|(idx, _)| !seed_set.contains(idx))
            .map(|(&idx, &d)| (idx, d))
            .collect();
        deps.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then(self.node(a.0).id.cmp(&self.node(b.0).id))
        });

        let total_symbols = deps.len();
        let depth_used = deps.iter().map(|(_, d)| *d).max().unwrap_or(0);
        let files: HashSet<&str> = deps
            .iter()
            .map(|(i, _)| self.node(*i).file.as_str())
            .collect();
        let total_files = files.len();

        let summary = format!(
            "{total_symbols} symbol(s) across {total_files} file(s) depend on {id} (depth≤{max_depth})"
        );

        if group {
            // Group by file with at most SAMPLE_PER_FILE samples each.
            let mut by_file: HashMap<String, (usize, usize, Vec<ImpactNode>)> = HashMap::new();
            for (idx, d) in &deps {
                let n = self.node(*idx);
                let entry = by_file
                    .entry(n.file.clone())
                    .or_insert((0, usize::MAX, Vec::new()));
                entry.0 += 1;
                entry.1 = entry.1.min(*d);
                if entry.2.len() < SAMPLE_PER_FILE {
                    entry.2.push(self.impact_node(*idx, *d));
                }
            }
            let mut files: Vec<ImpactFile> = by_file
                .into_iter()
                .map(|(file, (count, min_depth, samples))| ImpactFile {
                    file,
                    symbol_count: count,
                    min_depth,
                    sample_symbols: samples,
                })
                .collect();
            files.sort_by(|a, b| a.min_depth.cmp(&b.min_depth).then(a.file.cmp(&b.file)));
            let truncated = files.len() > limit;
            files.truncate(limit);
            ImpactResult {
                ok: true,
                summary,
                truncated,
                total_symbols,
                total_files,
                depth_used,
                files: Some(files),
                results: None,
            }
        } else {
            let truncated = deps.len() > limit;
            let results: Vec<ImpactNode> = deps
                .into_iter()
                .take(limit)
                .map(|(idx, d)| self.impact_node(idx, d))
                .collect();
            ImpactResult {
                ok: true,
                summary,
                truncated,
                total_symbols,
                total_files,
                depth_used,
                files: None,
                results: Some(results),
            }
        }
    }

    fn impact_node(&self, idx: usize, depth: usize) -> ImpactNode {
        let n = self.node(idx);
        ImpactNode {
            id: n.id.clone(),
            name: n.name.clone(),
            file: n.file.clone(),
            line: n.line,
            depth,
        }
    }

    // --- list_modules (directory modules + hotspots + cycles) -----------

    pub fn list_modules(
        &self,
        min_size: Option<usize>,
        top_hotspots: Option<usize>,
    ) -> ModulesResult {
        let min_size = min_size.unwrap_or(1).max(1);
        let top = top_hotspots.unwrap_or(10).min(30);

        // Directory modules.
        let mut mod_syms: HashMap<&str, usize> = HashMap::new();
        for n in &self.graph.nodes {
            if n.kind != NodeKind::File {
                *mod_syms.entry(dir_of(&n.file)).or_insert(0) += 1;
            }
        }
        let mut internal: HashMap<&str, usize> = HashMap::new();
        let mut inbound: HashMap<&str, usize> = HashMap::new();
        let mut outbound: HashMap<&str, usize> = HashMap::new();
        for e in &self.graph.edges {
            if e.kind == EdgeKind::Contains {
                continue;
            }
            let (Some(&f), Some(&t)) = (self.id_to_idx.get(&e.from), self.id_to_idx.get(&e.to))
            else {
                continue;
            };
            let fd = dir_of(&self.node(f).file);
            let td = dir_of(&self.node(t).file);
            if fd == td {
                *internal.entry(fd).or_insert(0) += 1;
            } else {
                *outbound.entry(fd).or_insert(0) += 1;
                *inbound.entry(td).or_insert(0) += 1;
            }
        }
        let mut modules: Vec<ModuleInfo> = mod_syms
            .into_iter()
            .filter(|(_, c)| *c >= min_size)
            .map(|(dir, count)| ModuleInfo {
                dir: dir.to_string(),
                symbol_count: count,
                internal_edges: internal.get(dir).copied().unwrap_or(0),
                inbound_edges: inbound.get(dir).copied().unwrap_or(0),
                outbound_edges: outbound.get(dir).copied().unwrap_or(0),
            })
            .collect();
        modules.sort_by(|a, b| b.symbol_count.cmp(&a.symbol_count).then(a.dir.cmp(&b.dir)));

        // Hotspots: call fan-in/out per symbol node.
        let mut hotspots: Vec<Hotspot> = (0..self.graph.nodes.len())
            .filter(|&i| self.node(i).kind != NodeKind::File)
            .map(|i| {
                let fan_in = self.in_adj[i]
                    .iter()
                    .filter(|a| a.kind == EdgeKind::Calls)
                    .count();
                let fan_out = self.out_adj[i]
                    .iter()
                    .filter(|a| a.kind == EdgeKind::Calls)
                    .count();
                let n = self.node(i);
                Hotspot {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    file: n.file.clone(),
                    line: n.line,
                    fan_in,
                    fan_out,
                    score: fan_in + fan_out,
                }
            })
            .filter(|h| h.score > 0)
            .collect();
        hotspots.sort_by(|a, b| b.score.cmp(&a.score).then(a.id.cmp(&b.id)));
        hotspots.truncate(top);

        // Cycles: strongly-connected components over Calls + Imports.
        let cycles = self.cycles();

        ModulesResult {
            ok: true,
            summary: format!(
                "{} module(s), {} hotspot(s), {} cycle(s)",
                modules.len(),
                hotspots.len(),
                cycles.len()
            ),
            modules,
            hotspots,
            cycles,
        }
    }

    /// Strongly-connected components (size > 1) over Calls + Imports edges,
    /// using petgraph's iterative Tarjan implementation.
    fn cycles(&self) -> Vec<Cycle> {
        use petgraph::graph::{DiGraph, NodeIndex};
        let mut g: DiGraph<(), ()> = DiGraph::new();
        let idx: Vec<NodeIndex> = (0..self.graph.nodes.len())
            .map(|_| g.add_node(()))
            .collect();
        for e in &self.graph.edges {
            if !matches!(e.kind, EdgeKind::Calls | EdgeKind::Imports) {
                continue;
            }
            if let (Some(&f), Some(&t)) = (self.id_to_idx.get(&e.from), self.id_to_idx.get(&e.to)) {
                g.add_edge(idx[f], idx[t], ());
            }
        }
        let mut cycles: Vec<Cycle> = petgraph::algo::tarjan_scc(&g)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| {
                let mut members: Vec<PathNode> =
                    scc.iter().map(|ni| self.path_node(ni.index())).collect();
                members.sort_by(|a, b| a.id.cmp(&b.id));
                Cycle {
                    size: members.len(),
                    members,
                }
            })
            .collect();
        cycles.sort_by(|a, b| {
            b.size
                .cmp(&a.size)
                .then(a.members[0].id.cmp(&b.members[0].id))
        });
        cycles
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Envelope<T> {
    pub ok: bool,
    pub summary: String,
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub results: Vec<T>,
}

impl<T> Envelope<T> {
    fn empty(summary: String) -> Self {
        Envelope {
            ok: false,
            summary,
            total: 0,
            returned: 0,
            truncated: false,
            results: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SymbolHit {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<u32>,
}

#[derive(Debug, Default, Serialize)]
pub struct EdgeCounts {
    pub calls: u32,
    pub imports: u32,
    pub implements: u32,
    pub contains: u32,
}

impl EdgeCounts {
    fn tally(adj: &[Adj]) -> Self {
        let mut c = EdgeCounts::default();
        for a in adj {
            match a.kind {
                EdgeKind::Calls => c.calls += 1,
                EdgeKind::Imports => c.imports += 1,
                EdgeKind::Implements => c.implements += 1,
                EdgeKind::Contains => c.contains += 1,
            }
        }
        c
    }
}

#[derive(Debug, Serialize)]
pub struct SymbolDetail {
    pub ok: bool,
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<NodeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    pub out_edges: EdgeCounts,
    pub in_edges: EdgeCounts,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    pub summary: String,
}

impl SymbolDetail {
    fn not_found(summary: &str, candidates: Vec<String>) -> Self {
        SymbolDetail {
            ok: candidates.is_empty(),
            found: false,
            id: None,
            name: None,
            kind: None,
            file: None,
            line: None,
            language: None,
            docstring: None,
            out_edges: EdgeCounts::default(),
            in_edges: EdgeCounts::default(),
            candidates,
            summary: summary.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NeighborHit {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub edge_kind: EdgeKind,
    pub direction: &'static str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub ambiguous: bool,
    pub count: u32,
}

#[derive(Debug, Serialize)]
pub struct PathNode {
    pub id: String,
    pub name: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PathResult {
    pub ok: bool,
    pub found: bool,
    pub length: usize,
    pub path: Vec<PathNode>,
    pub summary: String,
}

impl PathResult {
    fn none(summary: String) -> Self {
        PathResult {
            ok: true,
            found: false,
            length: 0,
            path: Vec::new(),
            summary,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ImpactNode {
    pub id: String,
    pub name: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub depth: usize,
}

#[derive(Debug, Serialize)]
pub struct ImpactFile {
    pub file: String,
    pub symbol_count: usize,
    pub min_depth: usize,
    pub sample_symbols: Vec<ImpactNode>,
}

#[derive(Debug, Serialize)]
pub struct ImpactResult {
    pub ok: bool,
    pub summary: String,
    pub truncated: bool,
    pub total_symbols: usize,
    pub total_files: usize,
    pub depth_used: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<ImpactFile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<ImpactNode>>,
}

impl ImpactResult {
    fn empty(summary: String) -> Self {
        ImpactResult {
            ok: false,
            summary,
            truncated: false,
            total_symbols: 0,
            total_files: 0,
            depth_used: 0,
            files: None,
            results: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModuleInfo {
    pub dir: String,
    pub symbol_count: usize,
    pub internal_edges: usize,
    pub inbound_edges: usize,
    pub outbound_edges: usize,
}

#[derive(Debug, Serialize)]
pub struct Hotspot {
    pub id: String,
    pub name: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub fan_in: usize,
    pub fan_out: usize,
    pub score: usize,
}

#[derive(Debug, Serialize)]
pub struct Cycle {
    pub size: usize,
    pub members: Vec<PathNode>,
}

#[derive(Debug, Serialize)]
pub struct ModulesResult {
    pub ok: bool,
    pub summary: String,
    pub modules: Vec<ModuleInfo>,
    pub hotspots: Vec<Hotspot>,
    pub cycles: Vec<Cycle>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn clamp(requested: Option<usize>, default: usize, max: usize) -> usize {
    requested.unwrap_or(default).clamp(1, max)
}

fn clamp_doc(s: &str) -> String {
    let one_line: String = s
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if one_line.chars().count() <= DOCSTRING_CLAMP {
        one_line
    } else {
        let mut out: String = one_line.chars().take(DOCSTRING_CLAMP).collect();
        out.push('…');
        out
    }
}

fn dir_of(rel: &str) -> &str {
    match rel.rsplit_once('/') {
        Some((d, _)) => d,
        None => ".",
    }
}

fn node_kind_from_str(s: &str) -> Option<NodeKind> {
    Some(match s.to_ascii_lowercase().as_str() {
        "file" => NodeKind::File,
        "class" => NodeKind::Class,
        "struct" => NodeKind::Struct,
        "interface" => NodeKind::Interface,
        "enum" => NodeKind::Enum,
        "trait" => NodeKind::Trait,
        "type" => NodeKind::Type,
        "function" | "fn" => NodeKind::Function,
        "method" => NodeKind::Method,
        _ => return None,
    })
}

fn kind_label(k: NodeKind) -> &'static str {
    match k {
        NodeKind::File => "file",
        NodeKind::Class => "class",
        NodeKind::Struct => "struct",
        NodeKind::Interface => "interface",
        NodeKind::Enum => "enum",
        NodeKind::Trait => "trait",
        NodeKind::Type => "type",
        NodeKind::Function => "function",
        NodeKind::Method => "method",
    }
}

fn edge_kind_from_str(s: &str) -> Option<EdgeKind> {
    Some(match s.to_ascii_lowercase().as_str() {
        "calls" | "call" => EdgeKind::Calls,
        "imports" | "import" => EdgeKind::Imports,
        "implements" | "implement" => EdgeKind::Implements,
        "contains" | "contain" => EdgeKind::Contains,
        _ => return None,
    })
}

fn parse_edge_kinds(opt: Option<&[String]>, default: &[EdgeKind]) -> Vec<EdgeKind> {
    match opt {
        Some(list) if !list.is_empty() => {
            let parsed: Vec<EdgeKind> = list.iter().filter_map(|s| edge_kind_from_str(s)).collect();
            if parsed.is_empty() {
                default.to_vec()
            } else {
                parsed
            }
        }
        _ => default.to_vec(),
    }
}
