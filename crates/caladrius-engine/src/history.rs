//! The history of executed commands: append-only, in order, failures included.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::CommandError;

/// Parameters larger than this (as JSON text) are not kept: the entry holds their size and a digest.
pub const MAX_RECORDED_PARAMS_BYTES: usize = 64 * 1024;

/// The parameters as the history keeps them: unchanged up to [`MAX_RECORDED_PARAMS_BYTES`], else
/// `{"omitted": true, "bytes": <size>, "digest": "fnv1a64:<16 hex digits>"}`. The digest is a
/// fingerprint to tell two large inputs apart, not a cryptographic hash.
pub(crate) fn recorded(params: Value) -> Value {
    let text = params.to_string();
    if text.len() <= MAX_RECORDED_PARAMS_BYTES {
        return params;
    }
    let digest = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    serde_json::json!({
        "omitted": true,
        "bytes": text.len(),
        "digest": format!("fnv1a64:{digest:016x}"),
    })
}

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

    /// Adopts the history of a saved project. Only when this one is empty, so what happened in
    /// this session is never replaced; checked: positions run from 1 without a gap.
    pub(crate) fn adopt(&mut self, entries: Vec<HistoryEntry>) -> Result<bool, CommandError> {
        if !self.entries.is_empty() {
            return Ok(false);
        }
        for (i, e) in entries.iter().enumerate() {
            if e.seq != i as u64 + 1 {
                return Err(CommandError::new(
                    "load_failed",
                    format!(
                        "the history of the project is damaged: entry {} has position {}",
                        i + 1,
                        e.seq
                    ),
                ));
            }
        }
        self.entries = entries;
        Ok(true)
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
            params: recorded(params),
            ok: outcome.is_ok(),
            error: outcome.as_ref().err().cloned(),
            changed_project: mutates && outcome.is_ok(),
        });
    }
}
