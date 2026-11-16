//! `hcoord`: the coordinator session's tooling outside the Claude pane.

use std::process::ExitCode;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    hcoord::cli::run(&args)
}
