//! The `hatlas` command line: argument parsing and the one-line report.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage:
  hatlas <repo dir> [--out <dir>]

Builds the repo's rustdoc JSON (nightly; the repo's own toolchain when it is one, else
`+nightly` into <target>/hatlas-nightly) and writes linked HTML pages to
<target>/hatlas, or to --out. Systems, observers and registrations come from
`rust-analyzer scip` (needs rust-analyzer on that toolchain; roughly 3.5 GB of memory
on a large game) joined with the workspace source. A run executes the repo's build
scripts and proc macros. The dependencies the workspace uses are documented too (public\nitems, one `cargo doc`); one that fails is listed on the index, not fatal.";

fn parse(args: &[String]) -> Result<hatlas::Options, String> {
    let mut repo = None;
    let mut out = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => {
                out = Some(PathBuf::from(it.next().ok_or("--out needs a directory")?));
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            path if repo.is_none() => repo = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument {extra}")),
        }
    }
    let repo = repo.ok_or("missing <repo dir>")?;
    Ok(hatlas::Options { repo, out })
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse(&args) {
        Ok(options) => options,
        Err(why) => {
            tracing::error!(reason = %why, "bad arguments");
            tracing::info!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match hatlas::run(&options) {
        Ok(s) => {
            println!("{}", summary_line(&s));
            ExitCode::SUCCESS
        }
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "hatlas failed");
            ExitCode::FAILURE
        }
    }
}

fn summary_line(s: &hatlas::Summary) -> String {
    format!(
        "{} ({} crates, {} modules, {} plugins, {} components, {} resources, {} systems, {} observers, {} events, {} dependencies ({} items, {} undocumented, {} unmapped), {} unresolved, {} pages)",
        s.index.display(),
        s.crates,
        s.modules,
        s.plugins,
        s.components,
        s.resources,
        s.systems,
        s.observers,
        s.events,
        s.dependencies,
        s.dependency_items,
        s.undocumented,
        s.unmapped,
        s.unresolved,
        s.pages
    )
}
