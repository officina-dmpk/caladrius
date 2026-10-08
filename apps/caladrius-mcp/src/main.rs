#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! The `caladrius-mcp` binary: an MCP server on standard input and output. Standard output
//! carries protocol messages only; the process ends when standard input ends.

use std::io::{BufReader, stdin, stdout};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut server = caladrius_mcp::Server::new();
    match server.serve(BufReader::new(stdin().lock()), stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        // The host closed the pipe or the output is unusable: nothing left to serve.
        Err(e) => {
            eprintln!("caladrius-mcp: {e}");
            ExitCode::from(1)
        }
    }
}
