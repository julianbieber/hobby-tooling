//! Query-engine tests over a hand-built graph (deterministic, no grammars),
//! plus one end-to-end test through the cache + real extraction.

use hobby_tree_extract::graph::{CodeGraph, Edge, EdgeKind, GraphStats, Node, NodeKind};
use hobby_tree_extract::graph_query::{Direction, LoadedGraph};

fn node(id: &str, kind: NodeKind, name: &str, file: &str, line: Option<u32>) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        file: file.to_string(),
        line,
        language: Some("rust".to_string()),
        docstring: None,
    }
}

fn edge(from: &str, to: &str, kind: EdgeKind) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        ambiguous: false,
        count: 1,
    }
}

/// foo → bar → baz → foo  (a 3-cycle of calls across two files).
fn fixture() -> LoadedGraph {
    let nodes = vec![
        node("a.rs", NodeKind::File, "a.rs", "a.rs", None),
        node("a.rs:L1", NodeKind::Function, "foo", "a.rs", Some(1)),
        node("a.rs:L10", NodeKind::Function, "bar", "a.rs", Some(10)),
        node("b.rs", NodeKind::File, "b.rs", "b.rs", None),
        node("b.rs:L1", NodeKind::Function, "baz", "b.rs", Some(1)),
    ];
    let edges = vec![
        edge("a.rs", "a.rs:L1", EdgeKind::Contains),
        edge("a.rs", "a.rs:L10", EdgeKind::Contains),
        edge("b.rs", "b.rs:L1", EdgeKind::Contains),
        edge("a.rs:L1", "a.rs:L10", EdgeKind::Calls), // foo → bar
        edge("a.rs:L10", "b.rs:L1", EdgeKind::Calls), // bar → baz
        edge("b.rs:L1", "a.rs:L1", EdgeKind::Calls),  // baz → foo
    ];
    let g = CodeGraph {
        root: "/repo".to_string(),
        head: None,
        generator: "test".to_string(),
        stats: GraphStats::default(),
        nodes,
        edges,
    };
    LoadedGraph::build(g)
}

#[test]
fn search_matches_and_limits() {
    let g = fixture();
    let r = g.search("ba", None, None, Some(10));
    let names: Vec<&str> = r.results.iter().map(|h| h.name.as_str()).collect();
    assert!(names.contains(&"bar"), "{names:?}");
    assert!(names.contains(&"baz"), "{names:?}");
    assert!(!names.contains(&"foo"));

    // limit truncates and flags
    let r = g.search("", Some(NodeKind::Function), None, Some(2));
    assert_eq!(r.returned, 2);
    assert!(r.truncated);
    assert_eq!(r.total, 3);
}

#[test]
fn get_symbol_reports_edge_counts() {
    let g = fixture();
    let d = g.get_symbol(Some("a.rs:L1"), None);
    assert!(d.found);
    assert_eq!(d.name.as_deref(), Some("foo"));
    assert_eq!(d.out_edges.calls, 1); // foo → bar
    assert_eq!(d.in_edges.calls, 1); // baz → foo

    // ambiguous name → candidates, not found
    let d = g.get_symbol(None, Some("nope"));
    assert!(!d.found);
}

#[test]
fn neighbors_directional() {
    let g = fixture();
    let out = g.neighbors("a.rs:L1", Direction::Out, Some(EdgeKind::Calls), None);
    assert_eq!(out.results.len(), 1);
    assert_eq!(out.results[0].name, "bar");

    let inc = g.neighbors("a.rs:L1", Direction::In, Some(EdgeKind::Calls), None);
    assert_eq!(inc.results.len(), 1);
    assert_eq!(inc.results[0].name, "baz");
}

#[test]
fn call_path_finds_shortest_and_misses_gracefully() {
    let g = fixture();
    let p = g.call_path("foo", "baz", None, None);
    assert!(p.found);
    assert_eq!(p.length, 2); // foo → bar → baz
    let chain: Vec<&str> = p.path.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(chain, vec!["foo", "bar", "baz"]);

    // depth cap prevents the path
    let p = g.call_path("foo", "baz", None, Some(1));
    assert!(!p.found);
}

#[test]
fn impact_reverse_reachability_with_depth() {
    let g = fixture();
    // Who depends on foo? baz (calls foo, depth 1), bar (→baz→foo, depth 2).
    let r = g.impact("a.rs:L1", None, Some(5), None, Some(false));
    let results = r.results.unwrap();
    let by_name: Vec<(&str, usize)> = results.iter().map(|n| (n.name.as_str(), n.depth)).collect();
    assert!(by_name.contains(&("baz", 1)), "{by_name:?}");
    assert!(by_name.contains(&("bar", 2)), "{by_name:?}");
    assert_eq!(r.total_symbols, 2);

    // depth 1 only reaches baz
    let r = g.impact("a.rs:L1", None, Some(1), None, Some(false));
    assert_eq!(r.total_symbols, 1);
}

#[test]
fn impact_file_grouping() {
    let g = fixture();
    let r = g.impact("a.rs:L1", None, Some(5), None, Some(true));
    let files = r.files.unwrap();
    // bar lives in a.rs, baz in b.rs → two files depend on foo.
    assert_eq!(files.len(), 2);
}

#[test]
fn list_modules_detects_cycle_and_hotspots() {
    let g = fixture();
    let m = g.list_modules(None, Some(10));
    // The 3-call cycle foo→bar→baz→foo is one SCC of size 3.
    assert_eq!(m.cycles.len(), 1);
    assert_eq!(m.cycles[0].size, 3);
    // Every function has fan_in+fan_out ≥ 1, so all three are hotspots.
    assert_eq!(m.hotspots.len(), 3);
    // Both files sit at root → one module "." with 3 symbols.
    assert!(
        m.modules
            .iter()
            .any(|md| md.dir == "." && md.symbol_count == 3)
    );
}

#[test]
fn end_to_end_through_cache() {
    // Build a real graph from source via the cache, then query it.
    let Ok(_) = hobby_tree_extract::extractor::Extractor::new() else {
        eprintln!("skipping: helix registry unavailable");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("lib.rs"),
        "fn helper() {}\nfn run() { helper(); helper(); }\n",
    )
    .unwrap();

    let handle = hobby_tree_extract::graph_cache::get_or_load(dir.path()).expect("load graph");
    assert!(handle.lazily_built, "graph should be built on first query");

    // run() calls helper() — there should be a resolved Calls edge, deduped to count 2.
    let neighbors = handle.graph.neighbors(
        &find_id(&handle, "run"),
        Direction::Out,
        Some(EdgeKind::Calls),
        None,
    );
    assert!(
        neighbors
            .results
            .iter()
            .any(|n| n.name == "helper" && n.count == 2),
        "expected run→helper call edge with count 2: {:?}",
        neighbors.results
    );

    // The graph file was written and is reused on the next load (not rebuilt).
    let handle2 = hobby_tree_extract::graph_cache::get_or_load(dir.path()).expect("reload graph");
    assert!(
        !handle2.lazily_built,
        "second load should hit the cache/disk"
    );
}

fn find_id(handle: &hobby_tree_extract::graph_cache::GraphHandle, name: &str) -> String {
    let r = handle.graph.search(name, None, None, Some(5));
    r.results
        .iter()
        .find(|h| h.name == name)
        .map(|h| h.id.clone())
        .unwrap_or_else(|| panic!("symbol {name} not found"))
}
