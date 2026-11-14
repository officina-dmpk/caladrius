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
//! Each entry of the history carries a [`Seal`](seal::Seal): a SHA-256 of the command, its
//! parameters and its answer, so that a saved project can be replayed and checked bit for bit
//! (`project.replay`). There is no file access here and the engine reads no clock: a CSV comes in
//! as text or bytes, a saved project goes out as bytes, the program that runs the engine may give
//! it a clock ([`Engine::set_clock`]) to date the entries, so the crate compiles to WebAssembly.
//!
//! A command that fails leaves the project as it was.

mod commands;
mod error;
mod export;
mod history;
mod job;
mod plot;
pub mod replay;
mod run;
mod schema;
pub mod seal;
mod svg;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_compare;
#[cfg(test)]
mod tests_job;
#[cfg(test)]
mod tests_pk2;
#[cfg(test)]
mod tests_pk3;
#[cfg(test)]
mod tests_plot;
#[cfg(test)]
mod tests_pop;
#[cfg(test)]
mod tests_replay;
#[cfg(test)]
mod tests_seal;
#[cfg(test)]
mod tests_units;

pub use caladrius_fit::{Control, FitResult, Observer, Progress, Step, TraceRow};
pub use caladrius_project::{self as project, Project};
pub use error::CommandError;
pub use history::{History, HistoryEntry, MAX_RECORDED_PARAMS_BYTES};
pub use job::{Completed, FIT_RUN, Job};
pub use seal::{Clock, Seal};
pub use svg::{COLORS as SVG_COLORS, SvgColors};

use history::Stamp;

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
#[derive(Debug)]
pub struct Engine {
    pub(crate) project: Project,
    pub(crate) history: History,
    /// Counts the commands that changed the project (see [`Engine::revision`]).
    pub(crate) revision: u64,
    /// Where the time of an entry comes from, if anywhere (see [`Engine::set_clock`]).
    pub(crate) clock: Option<Clock>,
    /// True when the seals leave out the version and the commit of this build.
    pub(crate) without_provenance: bool,
    /// The job that is running, if one is (see [`Engine::begin`]).
    pub(crate) running: Option<job::Running>,
    /// How many jobs this engine has begun; the number of the next one is this plus one.
    pub(crate) jobs_begun: u64,
    /// Tells this engine from every other one in the process, so that a job begun by one engine
    /// is never taken by another. Never in an answer, a seal or the history.
    pub(crate) identity: u64,
}

impl Default for Engine {
    fn default() -> Self {
        Self::with_project(Project::default())
    }
}

/// A copy of the project, the history and the settings. A job that is running is not copied: the
/// copy has no running job and takes an identity of its own, so a job of the original cannot be
/// finished on it.
impl Clone for Engine {
    fn clone(&self) -> Self {
        Self {
            project: self.project.clone(),
            history: self.history.clone(),
            revision: self.revision,
            clock: self.clock,
            without_provenance: self.without_provenance,
            running: None,
            jobs_begun: self.jobs_begun,
            identity: job::new_identity(),
        }
    }
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
            clock: None,
            without_provenance: false,
            running: None,
            jobs_begun: 0,
            identity: job::new_identity(),
        }
    }

    /// Gives the engine a clock: the entries of the history are then dated (`at`, next to the
    /// seal and never inside it). Without one they carry no time. The engine reads no clock
    /// itself, so that it compiles to WebAssembly: the desktop application and the command line
    /// pass the system clock, the page passes the one of the browser.
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = Some(clock);
    }

    /// Whether the seals say which engine made them (the version and the commit; on by default).
    /// The workload of `cargo xtask digest` turns it off, so that its digest does not depend on
    /// the commit it was built from.
    pub fn set_provenance(&mut self, on: bool) {
        self.without_provenance = !on;
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
    /// history, whether it succeeds or not, with its seal. A failed command leaves the project
    /// unchanged.
    pub fn execute(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        // A job is running: a command that would change the project under it is refused, and
        // nothing is recorded for it.
        if let Some(refusal) = self.refuse_while_running(id, &params) {
            return Err(refusal);
        }
        let received = params.clone();
        // What the command is about to read, hashed before it can change it.
        let input = if seal::is_sealed(id) {
            seal::input_hash(&self.project, id, &received)
        } else {
            None
        };
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
        self.record(id, received, input, &outcome, mutates);
        outcome
    }

    /// Appends the entry of a command that ran: sealed, unless it is one of the commands that
    /// carry no seal.
    fn record(
        &mut self,
        id: &str,
        received: Value,
        input: Option<String>,
        outcome: &Result<Value, CommandError>,
        mutates: bool,
    ) {
        let seal = seal::is_sealed(id)
            .then(|| seal::seal(id, &received, outcome, input, !self.without_provenance));
        let stamp = Stamp {
            seal,
            at: self.now(),
        };
        self.history.push(id, received, outcome, mutates, stamp);
    }

    /// Appends the entry of a refusal before any command ran (text that is not JSON, bytes that
    /// are not text): there is no command and no answer to seal.
    fn record_refusal(&mut self, id: &str, params: Value, outcome: &Result<Value, CommandError>) {
        let stamp = Stamp {
            seal: None,
            at: self.now(),
        };
        self.history.push(id, params, outcome, false, stamp);
    }

    fn now(&self) -> Option<String> {
        self.clock.and_then(|clock| clock()).map(seal::rfc3339)
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
                    self.record_refusal(id, Value::String(params.to_owned()), &outcome);
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
                self.record_refusal("data.import", json!({ "name": name }), &outcome);
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
                self.record_refusal("project.load", Value::Null, &outcome);
                outcome
            }
        }
    }

    /// `project.replay` from the bytes of a saved file: the same file, and the same refusal when
    /// it is not text, as [`Engine::load_bytes`] gives.
    pub fn replay_bytes(&mut self, bytes: &[u8]) -> Result<Value, CommandError> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.execute("project.replay", json!({ "text": text })),
            Err(_) => {
                let outcome = Err(CommandError::new(
                    "load_failed",
                    "this is not a Caladrius project: the file is not UTF-8 text",
                ));
                self.record_refusal("project.replay", Value::Null, &outcome);
                outcome
            }
        }
    }
}
