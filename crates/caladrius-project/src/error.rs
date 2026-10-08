//! Errors of the project layer. Every message says what to fix (golden rule 6).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a project operation was refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectError {
    /// Stable machine-readable code (`unknown_worksheet`, `csv_ragged_row`...).
    pub code: String,
    /// What went wrong and what to do.
    pub message: String,
}

impl ProjectError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProjectError {}

/// Shorthand.
pub(crate) type Result<T> = std::result::Result<T, ProjectError>;
