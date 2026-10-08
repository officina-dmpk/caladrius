//! Console output that cannot panic: `println!` aborts when the pipe is closed, these helpers ignore the failure.

use std::io::Write;

/// Writes one line to standard output.
pub fn out(line: &str) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

/// Writes one line to standard error.
pub fn err(line: &str) {
    let _ = writeln!(std::io::stderr(), "{line}");
}
