#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Layer L2: command registry, history and export.
//!
//! Every user-visible action is a command with a stable id (`data.import`, `nca.run`,
//! `fit.run`...), JSON parameters and a JSON result (golden rule 4). The UI, the CLI, the MCP
//! server and agents all go through [`Engine::execute`]; [`describe`] lists the commands with the
//! JSON Schema of their parameters and results, the same values the MCP server serves.
//!
//! An [`Engine`] owns a [`Project`] and an append-only [`History`] of the commands it executed.
//! There is no file access and no clock here: a CSV comes in as text or bytes, a saved project
//! goes out as bytes, so the crate compiles to WebAssembly.
//!
//! A command that fails leaves the project as it was.

mod commands;
mod error;
mod export;
mod history;
mod run;
mod schema;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_units;

pub use caladrius_project::{self as project, Project};
pub use error::CommandError;
pub use history::{History, HistoryEntry, MAX_RECORDED_PARAMS_BYTES};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One command as listed by [`describe`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandInfo {
    /// Stable id, for example `nca.run`.
    pub id: String,
    /// A short title.
    pub title: String,
    /// What the command does, what it returns and how it fails.
    pub description: String,
    /// True when a successful run can change the project (false: it only reads, or computes).
    pub mutates: bool,
    /// JSON Schema (draft 2020-12) of the parameters.
    pub params_schema: Value,
    /// JSON Schema of the result.
    pub result_schema: Value,
    /// Valid parameters, as an example.
    pub example: Value,
}

/// Every command, in a stable order.
pub fn describe() -> Vec<CommandInfo> {
    commands::COMMANDS
        .iter()
        .map(|c| CommandInfo {
            id: c.id.to_owned(),
            title: c.title.to_owned(),
            description: c.description.to_owned(),
            mutates: c.mutates,
            params_schema: (c.params)(),
            result_schema: (c.result)(),
            example: (c.example)(),
        })
        .collect()
}

/// The ids of the commands, in the order of [`describe`].
pub fn command_ids() -> Vec<&'static str> {
    commands::COMMANDS.iter().map(|c| c.id).collect()
}

/// A project and the history of the commands executed on it.
#[derive(Debug, Clone, Default)]
pub struct Engine {
    pub(crate) project: Project,
    pub(crate) history: History,
    /// Counts the commands that changed the project (see [`Engine::revision`]).
    pub(crate) revision: u64,
}

impl Engine {
    /// An engine with an empty project and an empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// An engine that starts from `project`.
    pub fn with_project(project: Project) -> Self {
        Self {
            project,
            history: History::default(),
            revision: 0,
        }
    }

    /// The current project, for reading (the UI draws from it).
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The commands executed so far.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Counts the commands that changed the project. The project cannot change without it
    /// moving, so a caller can cache something computed from the project (such as "differs from
    /// the saved one") and recompute it only when this number changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Executes command `id` with JSON `params` (`null` means none) and records it in the
    /// history, whether it succeeds or not. A failed command leaves the project unchanged.
    pub fn execute(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let recorded = params.clone();
        let (outcome, mut mutates) = match commands::find(id) {
            Some(command) => ((command.run)(self, params), command.mutates),
            None => (
                Err(CommandError::new(
                    "unknown_command",
                    format!(
                        "there is no command `{id}`; the commands are: {}",
                        command_ids().join(", ")
                    ),
                )),
                false,
            ),
        };
        // `model.simulate` only reads, except when it is asked to store: then it changed the project.
        if let Ok(answer) = &outcome {
            if id == "model.simulate" && answer.get("analysis").is_some_and(|a| !a.is_null()) {
                mutates = true;
            }
        }
        if mutates && outcome.is_ok() {
            self.revision = self.revision.wrapping_add(1);
        }
        self.history.push(id, recorded, &outcome, mutates);
        outcome
    }

    /// As [`Engine::execute`], with the parameters as JSON text (what a CLI or an MCP server
    /// receives). Text that is not JSON is an `invalid_parameters` error.
    pub fn execute_text(&mut self, id: &str, params: &str) -> Result<Value, CommandError> {
        let value = if params.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str(params) {
                Ok(v) => v,
                Err(e) => {
                    let outcome = Err(CommandError::invalid(id, format!("not valid JSON: {e}")));
                    self.history
                        .push(id, Value::String(params.to_owned()), &outcome, false);
                    return outcome;
                }
            }
        };
        self.execute(id, value)
    }

    /// `data.import` for a file read as bytes. The bytes must be UTF-8 text.
    pub fn import_csv(&mut self, name: &str, bytes: &[u8]) -> Result<Value, CommandError> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.execute("data.import", json!({ "name": name, "csv": text })),
            Err(e) => {
                let outcome = Err(CommandError::new(
                    "csv_not_utf8",
                    format!(
                        "the file is not valid UTF-8 (problem near byte {}); save it as CSV UTF-8",
                        e.valid_up_to()
                    ),
                ));
                self.history
                    .push("data.import", json!({ "name": name }), &outcome, false);
                outcome
            }
        }
    }

    /// `project.save`, as the bytes to write to a file.
    pub fn save_bytes(&mut self) -> Result<Vec<u8>, CommandError> {
        self.execute("project.save", Value::Null)?;
        let document = commands::document(self)?;
        serde_json::to_vec_pretty(&document).map_err(|e| {
            CommandError::new("save_failed", format!("the project cannot be saved: {e}"))
        })
    }

    /// `project.load` from the bytes of a saved file.
    pub fn load_bytes(&mut self, bytes: &[u8]) -> Result<Value, CommandError> {
        match String::from_utf8(bytes.to_vec()) {
            Ok(text) => self.execute("project.load", json!({ "text": text })),
            Err(_) => {
                let outcome = Err(CommandError::new(
                    "load_failed",
                    "this is not a Caladrius project: the file is not UTF-8 text",
                ));
                self.history
                    .push("project.load", Value::Null, &outcome, false);
                outcome
            }
        }
    }
}
