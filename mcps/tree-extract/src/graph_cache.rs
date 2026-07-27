//! Process-wide cache of loaded code graphs, keyed by canonical repo path and
//! invalidated when `<repo>/.project-graph.json` changes on disk. If the graph
//! file is missing, it is lazily built (extract → build → write) on first
//! query so callers never have to orchestrate a build step.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use crate::extractor::Extractor;
use crate::graph::{CodeGraph, build_graph};
use crate::graph_query::LoadedGraph;

struct Entry {
    mtime: Option<SystemTime>,
    graph: Arc<LoadedGraph>,
}

static CACHE: OnceLock<Mutex<HashMap<PathBuf, Entry>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<PathBuf, Entry>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct GraphHandle {
    pub graph: Arc<LoadedGraph>,
    pub lazily_built: bool,
    pub path: PathBuf,
}

/// Default graph file path for a repo.
pub fn graph_path(repo: &Path) -> PathBuf {
    repo.join(".project-graph.json")
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Get a loaded graph for `repo_path`, using the cache when the on-disk graph
/// is unchanged. Lazily builds + writes the graph if it does not yet exist.
pub fn get_or_load(repo_path: &str) -> Result<GraphHandle, String> {
    let repo = Path::new(repo_path)
        .canonicalize()
        .map_err(|e| format!("canonicalize {repo_path}: {e}"))?;
    let gpath = graph_path(&repo);
    let disk_mtime = mtime_of(&gpath);

    if disk_mtime.is_some() {
        let guard = cache().lock().unwrap();
        if let Some(e) = guard.get(&repo)
            && e.mtime == disk_mtime
        {
            return Ok(GraphHandle {
                graph: Arc::clone(&e.graph),
                lazily_built: false,
                path: gpath,
            });
        }
    }

    let (graph, lazily_built) = if gpath.exists() {
        (CodeGraph::load(&gpath).map_err(|e| e.to_string())?, false)
    } else {
        let g = build_for(&repo)?;
        let json = g.to_json().map_err(|e| e.to_string())?;
        std::fs::write(&gpath, json).map_err(|e| format!("writing {}: {e}", gpath.display()))?;
        let _ = crate::gitignore::ensure_entry(&repo, &gpath);
        (g, true)
    };

    let loaded = Arc::new(LoadedGraph::build(graph));
    let mut guard = cache().lock().unwrap();
    guard.insert(
        repo.clone(),
        Entry {
            mtime: mtime_of(&gpath),
            graph: Arc::clone(&loaded),
        },
    );
    Ok(GraphHandle {
        graph: loaded,
        lazily_built,
        path: gpath,
    })
}

/// Build the graph for a repo (extract → build_graph). Shared by the lazy path
/// and the explicit `build_graph` / CLI paths.
pub fn build_for(repo: &Path) -> Result<CodeGraph, String> {
    let extractor = Extractor::new().map_err(|e| e.to_string())?;
    let extracted = extractor.extract_repo(repo);
    Ok(build_graph(&extracted, repo, git_head(repo)))
}

/// Build the graph and write it to `output` (defaulting to the standard path),
/// registering it in `.git/info/exclude`. Invalidates any cached entry so the
/// next query reloads. Returns the built graph.
pub fn build_and_write(repo: &Path, output: Option<&Path>) -> Result<(CodeGraph, PathBuf), String> {
    let graph = build_for(repo)?;
    let out = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| graph_path(repo));
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = graph.to_json().map_err(|e| e.to_string())?;
    std::fs::write(&out, json).map_err(|e| format!("writing {}: {e}", out.display()))?;
    let _ = crate::gitignore::ensure_entry(repo, &out);
    // Drop any stale cache entry for this repo.
    if let Ok(canon) = repo.canonicalize() {
        cache().lock().unwrap().remove(&canon);
    }
    Ok((graph, out))
}

fn git_head(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
