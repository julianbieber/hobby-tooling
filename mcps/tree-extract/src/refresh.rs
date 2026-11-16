//! Writing a repository's markdown symbol map, and its code graph beside it,
//! to disk.

use std::path::{Path, PathBuf};

use crate::extractor::{Extractor, RegistryUnavailable};
use crate::graph::{GraphFileError, GraphStats, build_graph};
use crate::graph_cache::graph_path;
use crate::md_render::{RenderOptions, render};

/// The symbol map or the graph could not be written.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("repository {} cannot be opened", path.display())]
    Repo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Registry(#[from] RegistryUnavailable),
    #[error("cannot write symbol map {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Graph(#[from] GraphFileError),
}

/// Where the code graph goes.
#[derive(Debug, Clone)]
pub enum GraphOutput {
    Skip,
    /// `<repo>/.project-graph.json`.
    Default,
    At(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Request {
    pub repo: PathBuf,
    /// Defaults to `<repo>/.project-description.md`.
    pub output: Option<PathBuf>,
    /// Renderer default when `None`.
    pub size_threshold_bytes: Option<usize>,
    /// Write nothing when the map's front matter already records the current
    /// git `HEAD`.
    pub skip_if_unchanged: bool,
    /// Add what is written to `<repo>/.git/info/exclude`.
    pub exclude_from_git: bool,
    pub graph: GraphOutput,
}

#[derive(Debug)]
pub enum Outcome {
    UpToDate { output: PathBuf, head: String },
    Written(Written),
}

#[derive(Debug)]
pub struct Written {
    pub output: PathBuf,
    pub symbols: usize,
    pub bytes: usize,
    pub graph: Option<WrittenGraph>,
}

#[derive(Debug)]
pub struct WrittenGraph {
    pub path: PathBuf,
    pub stats: GraphStats,
}

pub fn refresh(request: &Request) -> Result<Outcome, RefreshError> {
    let repo = request
        .repo
        .canonicalize()
        .map_err(|source| RefreshError::Repo {
            path: request.repo.clone(),
            source,
        })?;
    let output = request
        .output
        .clone()
        .unwrap_or_else(|| repo.join(".project-description.md"));

    let head = crate::git::head(&repo);
    if request.skip_if_unchanged
        && let Some(current) = &head
        && is_up_to_date(&output, current)
    {
        return Ok(Outcome::UpToDate {
            output,
            head: current.clone(),
        });
    }

    tracing::info!(repo = %repo.display(), "scanning");
    let extractor = Extractor::new()?;
    let extracted = extractor.extract_repo(&repo);
    tracing::info!(symbols = extracted.symbols.len(), "extracted");

    let mut opts = RenderOptions::new(repo.clone());
    if let Some(bytes) = request.size_threshold_bytes {
        opts.size_threshold_bytes = bytes;
    }
    opts.head_hash = head.clone();
    let body = render(&extracted.symbols, &opts);
    write_map(&output, &body)?;
    if request.exclude_from_git {
        crate::gitignore::ensure_entry_or_warn(&repo, &output);
    }

    let graph_output = match &request.graph {
        GraphOutput::Skip => None,
        GraphOutput::Default => Some(graph_path(&repo)),
        GraphOutput::At(path) => Some(path.clone()),
    };
    let graph = match graph_output {
        None => None,
        Some(path) => {
            let g = build_graph(&extracted, &repo, head);
            g.write(&path)?;
            if request.exclude_from_git {
                crate::gitignore::ensure_entry_or_warn(&repo, &path);
            }
            Some(WrittenGraph {
                path,
                stats: g.stats,
            })
        }
    };

    Ok(Outcome::Written(Written {
        output,
        symbols: extracted.symbols.len(),
        bytes: body.len(),
        graph,
    }))
}

fn write_map(output: &Path, body: &str) -> Result<(), RefreshError> {
    let write_error = |source| RefreshError::Write {
        path: output.to_path_buf(),
        source,
    };
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(write_error)?;
    }
    std::fs::write(output, body).map_err(write_error)
}

fn is_up_to_date(output: &Path, head: &str) -> bool {
    std::fs::read_to_string(output).is_ok_and(|existing| front_matter_head(&existing) == Some(head))
}

fn front_matter_head(body: &str) -> Option<&str> {
    let mut lines = body.lines();
    if lines.next()? != "---" {
        return None;
    }
    lines
        .take_while(|line| *line != "---")
        .find_map(|line| line.strip_prefix("head: "))
        .map(str::trim)
}
