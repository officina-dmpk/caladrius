//! The error of a command: a stable code and a message that says what to fix.

use std::fmt;

use caladrius_project::ProjectError;
use serde::{Deserialize, Serialize};

/// Why a command did not run or did not finish.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandError {
    /// Stable machine-readable code: `unknown_command`, `invalid_parameters`, `nca_error`,
    /// `fit_error`, `model_error`, or the code of a project error (`unknown_worksheet`...).
    pub code: String,
    /// What went wrong and what to do.
    pub message: String,
}

impl CommandError {
    /// An error with this code.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    pub(crate) fn invalid(command: &str, detail: impl fmt::Display) -> Self {
        Self::new(
            "invalid_parameters",
            format!("invalid parameters for `{command}`: {detail}"),
        )
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<ProjectError> for CommandError {
    fn from(e: ProjectError) -> Self {
        Self {
            code: e.code,
            message: e.message,
        }
    }
}
