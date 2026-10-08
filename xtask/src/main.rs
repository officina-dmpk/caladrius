#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Developer tasks for the Caladrius workspace, run as `cargo xtask <task>` (alias in `.cargo/config.toml`).

mod console;
mod error;
mod layers;
mod wasm;
mod workspace;

use std::ffi::OsString;
use std::process::ExitCode;

use crate::error::Result;

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  layers   print the layer table and check the dependency rules between crates
  wasm     cargo check the L0 to L3 crates for wasm32-unknown-unknown
  help     show this message";

/// The tasks this binary can run.
#[derive(Debug, PartialEq, Eq)]
enum Task {
    Layers,
    Wasm,
    Help,
}

/// Reads the command line (without the program name); the error is a message for the user.
fn parse_task(args: &[OsString]) -> std::result::Result<Task, String> {
    match args {
        [] => Err("missing task".to_owned()),
        [task] => match task.to_str() {
            Some("layers") => Ok(Task::Layers),
            Some("wasm") => Ok(Task::Wasm),
            Some("help" | "-h" | "--help") => Ok(Task::Help),
            _ => Err(format!("unknown task `{}`", task.to_string_lossy())),
        },
        [_, extra, ..] => Err(format!("unexpected argument `{}`", extra.to_string_lossy())),
    }
}

fn main() -> ExitCode {
    // `args_os`, not `args`: the latter panics on an argument that is not valid Unicode.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let task = match parse_task(&args) {
        Ok(task) => task,
        Err(message) => {
            console::err(&format!("error: {message}\n\n{USAGE}"));
            return ExitCode::from(2);
        }
    };
    let outcome: Result<()> = match task {
        Task::Layers => layers::run(),
        Task::Wasm => wasm::run(),
        Task::Help => {
            console::out(USAGE);
            Ok(())
        }
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            console::err(&format!("error: {error}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> std::result::Result<Task, String> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse_task(&args)
    }

    #[test]
    fn known_tasks_are_recognised() {
        assert_eq!(parse(&["layers"]), Ok(Task::Layers));
        assert_eq!(parse(&["wasm"]), Ok(Task::Wasm));
        assert_eq!(parse(&["help"]), Ok(Task::Help));
        assert_eq!(parse(&["--help"]), Ok(Task::Help));
        assert_eq!(parse(&["-h"]), Ok(Task::Help));
    }

    #[test]
    fn a_missing_unknown_or_extra_argument_is_a_usage_error() {
        assert_eq!(parse(&[]), Err("missing task".to_owned()));
        assert_eq!(parse(&["bogus"]), Err("unknown task `bogus`".to_owned()));
        assert_eq!(
            parse(&["layers", "--fast"]),
            Err("unexpected argument `--fast`".to_owned())
        );
    }
}
