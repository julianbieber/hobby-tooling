//! The `hobby-struct-parser` command line: read a repo's standards, count the
//! code against them, and report.

use std::process::ExitCode;

use struct_parser::cli::{self, Args, Verdict};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "invalid arguments");
            tracing::info!("{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    let mut out = Vec::new();
    let result = cli::run(&args, &mut out);
    print!("{}", String::from_utf8_lossy(&out));
    match result {
        Ok(Verdict::Pass) => ExitCode::SUCCESS,
        Ok(Verdict::Fail) => ExitCode::FAILURE,
        Err(e) => {
            let error: &dyn std::error::Error = &e;
            tracing::error!(error, "could not check the standards");
            ExitCode::from(2)
        }
    }
}
