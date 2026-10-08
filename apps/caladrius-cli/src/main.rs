#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! The `caladrius-cli` binary: arguments in, answer on standard output, `error: ...` on standard
//! error and exit code 1 on any failure. See the library for what it does.

use std::io::{Write, stderr, stdin, stdout};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = match caladrius_cli::text_args(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            let _ = writeln!(stderr(), "error: {e}");
            return ExitCode::from(1);
        }
    };
    match caladrius_cli::run(&args, &mut stdin()) {
        Ok(output) => {
            for note in &output.notes {
                let _ = writeln!(stderr(), "{note}");
            }
            // A closed pipe (`| head`) is not a failure of the command.
            let _ = stdout().write_all(output.stdout.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = writeln!(stderr(), "error: {e}");
            ExitCode::from(1)
        }
    }
}
