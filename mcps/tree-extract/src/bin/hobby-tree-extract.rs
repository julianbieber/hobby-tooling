//! The `hobby-tree-extract` command: writes a repository's symbol map and
//! code graph, and logs what it wrote or why it failed to stderr.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use hobby_tree_extract::refresh::{GraphOutput, Outcome, Request, refresh};

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
    #[arg(long, conflicts_with = "no_graph")]
    graph_output: Option<PathBuf>,
}

impl Cli {
    fn into_request(self) -> Request {
        let graph = match (self.no_graph, self.graph_output) {
            (true, _) => GraphOutput::Skip,
            (false, Some(path)) => GraphOutput::At(path),
            (false, None) => GraphOutput::Default,
        };
        Request {
            repo: self.repo,
            output: self.output,
            size_threshold_bytes: Some(self.size_threshold_kib.saturating_mul(1024)),
            skip_if_unchanged: self.skip_if_unchanged,
            exclude_from_git: !self.no_gitignore,
            graph,
        }
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();

    let request = Cli::parse().into_request();
    match refresh(&request) {
        Ok(outcome) => {
            report(&outcome);
            ExitCode::SUCCESS
        }
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "refresh failed");
            ExitCode::FAILURE
        }
    }
}

fn report(outcome: &Outcome) {
    match outcome {
        Outcome::UpToDate { output, head } => {
            tracing::info!(path = %output.display(), %head, "up to date");
        }
        Outcome::Written(w) => {
            tracing::info!(path = %w.output.display(), symbols = w.symbols, bytes = w.bytes, "wrote symbol map");
            if let Some(g) = &w.graph {
                tracing::info!(
                    path = %g.path.display(),
                    nodes = g.stats.nodes,
                    edges = g.stats.edges,
                    ambiguous = g.stats.ambiguous_edges,
                    unresolved_calls = g.stats.unresolved_calls,
                    "wrote graph"
                );
            }
        }
    }
}
