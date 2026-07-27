use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Parser;

use hobby_tree_extract::extractor::Extractor;
use hobby_tree_extract::md_render::{RenderOptions, render};
use hobby_tree_extract::{gitignore, graph};

#[derive(Parser, Debug)]
#[command(
    name = "tree-extract",
    about = "Walk a repo with tree-sitter and write a .project-description.md symbol map"
)]
struct Cli {
    /// Repository root to scan (defaults to cwd).
    #[arg(default_value = ".")]
    repo: PathBuf,

    /// Output path. Defaults to <repo>/.project-description.md.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Skip writing the file if its current `head:` front matter matches the
    /// current git HEAD (idempotent refresh).
    #[arg(long)]
    skip_if_unchanged: bool,

    /// Soft size threshold (KiB). When exceeded, method docstrings are
    /// dropped on a second render pass.
    #[arg(long, default_value_t = 50)]
    size_threshold_kib: usize,

    /// Don't add the output file to <repo>/.git/info/exclude.
    #[arg(long)]
    no_gitignore: bool,

    /// Skip writing the code graph (.project-graph.json).
    #[arg(long)]
    no_graph: bool,

    /// Code graph output path. Defaults to <repo>/.project-graph.json.
    #[arg(long)]
    graph_output: Option<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let repo = cli
        .repo
        .canonicalize()
        .with_context(|| format!("canonicalize {}", cli.repo.display()))?;
    let output = cli
        .output
        .clone()
        .unwrap_or_else(|| repo.join(".project-description.md"));

    let head_hash = git_head(&repo).ok();
    if cli.skip_if_unchanged
        && let Some(current) = &head_hash
        && let Ok(existing) = std::fs::read_to_string(&output)
        && front_matter_head(&existing).as_deref() == Some(current.as_str())
    {
        eprintln!(
            "tree-extract: {} is up to date (HEAD {})",
            output.display(),
            current
        );
        return Ok(());
    }

    eprintln!("tree-extract: scanning {}", repo.display());
    let extractor = Extractor::new()?;
    let extracted = extractor.extract_repo(&repo);
    let symbols = &extracted.symbols;
    eprintln!("tree-extract: extracted {} symbols", symbols.len());

    let mut opts = RenderOptions::new(repo.clone());
    opts.size_threshold_bytes = cli.size_threshold_kib * 1024;
    opts.head_hash = head_hash.clone();
    let body = render(symbols, &opts);

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&output, body).with_context(|| format!("writing {}", output.display()))?;
    eprintln!("tree-extract: wrote {}", output.display());

    if !cli.no_gitignore {
        gitignore::ensure_entry(&repo, &output)?;
    }

    if !cli.no_graph {
        let graph_output = cli
            .graph_output
            .clone()
            .unwrap_or_else(|| repo.join(".project-graph.json"));
        let g = graph::build_graph(&extracted, &repo, head_hash);
        let json = g.to_json()?;
        if let Some(parent) = graph_output.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&graph_output, json)
            .with_context(|| format!("writing {}", graph_output.display()))?;
        eprintln!(
            "tree-extract: wrote {} ({} nodes, {} edges, {} ambiguous, {} unresolved calls)",
            graph_output.display(),
            g.stats.nodes,
            g.stats.edges,
            g.stats.ambiguous_edges,
            g.stats.unresolved_calls
        );
        if !cli.no_gitignore {
            gitignore::ensure_entry(&repo, &graph_output)?;
        }
    }
    Ok(())
}

fn git_head(repo: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .context("running git rev-parse HEAD")?;
    if !out.status.success() {
        anyhow::bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn front_matter_head(body: &str) -> Option<String> {
    let mut lines = body.lines();
    if lines.next()? != "---" {
        return None;
    }
    for line in lines {
        if line == "---" {
            return None;
        }
        if let Some(rest) = line.strip_prefix("head: ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}
