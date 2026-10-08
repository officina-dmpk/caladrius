//! The command table. Every user-visible action is a command with a stable id, parameters that
//! deserialize from JSON, a result that serializes to JSON, and the schemas of both.

mod analysis;
mod data;
mod project;

pub(crate) use project::document;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Engine;
use crate::error::CommandError;

/// A command: metadata, schemas and the function that runs it.
pub(crate) struct CommandDef {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) description: &'static str,
    /// True when a successful run can change the project.
    pub(crate) mutates: bool,
    pub(crate) params: fn() -> Value,
    pub(crate) result: fn() -> Value,
    /// Valid parameters. Run in table order against a fresh engine, the examples all succeed.
    pub(crate) example: fn() -> Value,
    pub(crate) run: fn(&mut Engine, Value) -> Result<Value, CommandError>,
}

/// In table order: the examples are a runnable script (`tests/`), so a command comes after the
/// ones that create what its example refers to.
pub(crate) const COMMANDS: &[CommandDef] = &[
    data::IMPORT,
    data::DESCRIBE,
    data::SET_COLUMN,
    data::SET_CELL,
    analysis::NCA_RUN,
    analysis::ANALYSIS_GET,
    analysis::FIT_INITIAL_ESTIMATES,
    analysis::FIT_RUN,
    analysis::MODEL_SIMULATE,
    analysis::ANALYSIS_RUN,
    analysis::EXPORT_TABLE,
    project::DESCRIBE,
    project::SAVE,
    project::HISTORY_LIST,
    analysis::ANALYSIS_REMOVE,
    data::REMOVE,
    project::NEW,
    project::LOAD,
];

pub(crate) fn find(id: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Deserializes the parameters of `command`. `null` means no parameters.
pub(crate) fn parse<T: DeserializeOwned>(command: &str, params: Value) -> Result<T, CommandError> {
    let params = if params.is_null() { json!({}) } else { params };
    if !params.is_object() {
        return Err(CommandError::invalid(
            command,
            "the parameters must be a JSON object (or null for none)",
        ));
    }
    serde_json::from_value(params).map_err(|e| CommandError::invalid(command, e))
}

/// Serializes a result.
pub(crate) fn respond<T: Serialize>(value: &T) -> Result<Value, CommandError> {
    serde_json::to_value(value).map_err(|e| {
        CommandError::new(
            "internal_error",
            format!("the result cannot be serialized: {e}"),
        )
    })
}

/// A `nullable` double option: absent is `None`, `null` is `Some(None)`, a value is `Some(Some)`.
pub(crate) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// A subject label given as a string or as a number (`subject: 1` and `subject: "1"` name the
/// same subject); absent or null means no subject.
pub(crate) fn label<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        // `1`, `1.0` and "1" are the same label (the worksheet writes whole numbers without a point).
        Some(Value::Number(n)) => Ok(Some(match n.as_f64() {
            Some(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{x:.0}"),
            _ => n.to_string(),
        })),
        Some(_) => Err(D::Error::custom("a subject is a string or a number")),
    }
}
