//! The history of executed commands: append-only, in order, failures included.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::CommandError;

/// One executed command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Position, from 1, without gaps.
    pub seq: u64,
    /// Command id.
    pub command: String,
    /// The parameters as received.
    pub params: Value,
    /// Whether it succeeded.
    pub ok: bool,
    /// The error, when it failed.
    pub error: Option<CommandError>,
    /// True when it succeeded and changed the project (a command that only reads does not).
    pub changed_project: bool,
}

/// The entries so far. There is no way to remove or change one: the only operation that adds is
/// private to the crate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    entries: Vec<HistoryEntry>,
}

impl History {
    /// Every entry, oldest first.
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True before the first command.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn push(
        &mut self,
        command: &str,
        params: Value,
        outcome: &Result<Value, CommandError>,
        mutates: bool,
    ) {
        let seq = self.entries.len() as u64 + 1;
        self.entries.push(HistoryEntry {
            seq,
            command: command.to_owned(),
            params,
            ok: outcome.is_ok(),
            error: outcome.as_ref().err().cloned(),
            changed_project: mutates && outcome.is_ok(),
        });
    }
}
