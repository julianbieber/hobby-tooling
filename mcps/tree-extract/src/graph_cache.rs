//! Process-wide cache of loaded code graphs, keyed by canonical repo path and
//! invalidated when `<repo>/.project-graph.json` changes on disk. If the graph
//! file is missing, it is lazily built (extract → build → write) on first
//! query so callers never have to orchestrate a build step.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::SystemTime;

use parking_lot::Mutex;

use crate::extractor::{Extractor, RegistryUnavailable};
use crate::graph::{CodeGraph, GraphFileError, build_graph};
use crate::graph_query::LoadedGraph;

/// A repository's code graph could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("repository {} cannot be opened", path.display())]
    Repo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Registry(#[from] RegistryUnavailable),
    #[error(transparent)]
    File(#[from] GraphFileError),
}

struct Entry {
    mtime: Option<SystemTime>,
    graph: Arc<LoadedGraph>,
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, Entry>>> = LazyLock::new(Mutex::default);

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

/// `repo_path` resolved to the canonical directory every cache entry is keyed by.
pub fn canonical_repo(repo_path: &Path) -> Result<PathBuf, GraphError> {
    repo_path.canonicalize().map_err(|source| GraphError::Repo {
        path: repo_path.to_path_buf(),
        source,
    })
}

/// Get a loaded graph for `repo_path`, using the cache when the on-disk graph
/// is unchanged. Lazily builds + writes the graph if it does not yet exist.
pub fn get_or_load(repo_path: &Path) -> Result<GraphHandle, GraphError> {
    let repo = canonical_repo(repo_path)?;
    let gpath = graph_path(&repo);
    let disk_mtime = mtime_of(&gpath);

    if disk_mtime.is_some()
        && let Some(e) = CACHE.lock().get(&repo)
        && e.mtime == disk_mtime
    {
        return Ok(GraphHandle {
            graph: Arc::clone(&e.graph),
            lazily_built: false,
            path: gpath,
        });
    }

    let (graph, lazily_built) = if gpath.exists() {
        (CodeGraph::load(&gpath)?, false)
    } else {
        let g = build_for(&repo)?;
        g.write(&gpath)?;
        crate::gitignore::ensure_entry_or_warn(&repo, &gpath);
        (g, true)
    };

    let loaded = Arc::new(LoadedGraph::build(graph));
    CACHE.lock().insert(
        repo,
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
pub fn build_for(repo: &Path) -> Result<CodeGraph, GraphError> {
    let extractor = Extractor::new()?;
    let extracted = extractor.extract_repo(repo);
    Ok(build_graph(&extracted, repo, crate::git::head(repo)))
}

/// Build the graph and write it to `output` (defaulting to the standard path),
/// registering it in `.git/info/exclude`. Invalidates any cached entry so the
/// next query reloads. Returns the built graph.
pub fn build_and_write(
    repo: &Path,
    output: Option<&Path>,
) -> Result<(CodeGraph, PathBuf), GraphError> {
    let repo = canonical_repo(repo)?;
    let graph = build_for(&repo)?;
    let out = output.map_or_else(|| graph_path(&repo), Path::to_path_buf);
    graph.write(&out)?;
    crate::gitignore::ensure_entry_or_warn(&repo, &out);
    CACHE.lock().remove(&repo);
    Ok((graph, out))
}
