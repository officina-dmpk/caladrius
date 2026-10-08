//! The error type of the tasks: a readable message and nothing else.

use std::fmt;
use std::path::Path;

/// A user-facing error. The message is exactly what the person running the task reads.
#[derive(Debug)]
pub struct XtaskError(String);

impl XtaskError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// An I/O failure, with the action that failed and the path involved.
    pub fn io(action: &str, path: &Path, source: &std::io::Error) -> Self {
        Self(format!("cannot {action} {}: {source}", path.display()))
    }
}

impl fmt::Display for XtaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for XtaskError {}

pub type Result<T> = std::result::Result<T, XtaskError>;
