//! The `hatlas` command line: argument parsing and the one-line report.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage:
  hatlas <repo dir> [--out <dir>]

Builds the repo's rustdoc JSON (nightly; the repo's own toolchain when it is one, else
`+nightly` into <target>/hatlas-nightly) and writes linked HTML pages to
<target>/hatlas, or to --out. A run executes the repo's build scripts and proc macros.";

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse(&args) {
        Ok(options) => options,
        Err(why) => {
            eprintln!("hatlas: {why}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match hatlas::run(&options) {
        Ok(s) => {
            println!(
                "{} ({} crates, {} modules, {} plugins, {} components, {} resources, {} pages)",
                s.index.display(),
                s.crates,
                s.modules,
                s.plugins,
                s.components,
                s.resources,
                s.pages
            );
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("hatlas: {why}");
            ExitCode::FAILURE
        }
    }
}
