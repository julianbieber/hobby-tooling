//! What a subcommand hands back to the command line: the exact text for standard
//! output and the exit status that goes with it.

use std::process::ExitCode;

/// The text a subcommand prints on standard output, byte for byte, and the exit status
/// it ends with.
#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub code: ExitCode,
}

impl Output {
    /// `line` followed by a newline.
    pub fn line(line: &str, code: ExitCode) -> Self {
        Self {
            stdout: format!("{line}\n"),
            code,
        }
    }
}
