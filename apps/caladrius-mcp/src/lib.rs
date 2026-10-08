#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! App layer: an MCP (Model Context Protocol) server over standard input and output that exposes
//! every command of the registry (`caladrius-engine`) to an agent such as Apothicaire.
//!
//! Transport: one JSON-RPC 2.0 message per line on stdin, one per line on stdout; nothing else is
//! ever written to stdout. Methods: `initialize`, `ping`, `tools/list`, `tools/call`; the
//! notifications of the protocol are accepted and ignored; any other method is a JSON-RPC
//! "method not found" error. Malformed input gets a JSON-RPC error and the server carries on.
//!
//! Tools: one per command, named like the command id with `.` written `_` (`nca.run` is
//! `nca_run`: some agent hosts restrict tool names to letters, digits, `_` and `-`); a call by the
//! original id works too. `inputSchema` and `outputSchema` are the registry's JSON Schemas. A
//! result is JSON text content plus `structuredContent`; a command that fails is a tool error
//! (`isError: true`) whose text is `<code>: <message>`. The server holds one [`Engine`]: the
//! worksheets and analyses of a session persist between calls, as in the UI.

use std::io::{BufRead, Write};

use caladrius_engine::{CommandInfo, Engine, describe};
use serde_json::{Value, json};

/// Protocol versions this server speaks, newest first. The client's version is echoed back when
/// it is in the list, else the newest is proposed (MCP lifecycle negotiation).
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What the model reads once, at initialization.
const INSTRUCTIONS: &str = "Caladrius: pharmacokinetic analysis. Every tool is a command with a JSON schema. \
Typical session: data_import (CSV text in `csv`; roles and units are guessed from the headers and reported), \
data_describe to check them (data_set_column to fix a role or a unit), nca_run for a non-compartmental analysis \
or fit_run for a model fit (the answer holds the result and its status), export_table for CSV tables. \
Worksheets and analyses persist during the session; after data_set_cell or data_set_column an analysis is \
`stale` until analysis_run. project_save returns the whole project to keep. Errors say what to fix.";

/// The tool name of a command id.
pub fn tool_name(command: &str) -> String {
    command.replace('.', "_")
}

/// One MCP session: an engine and the tool table.
pub struct Server {
    engine: Engine,
    tools: Vec<CommandInfo>,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

impl Server {
    /// A server with an empty project.
    pub fn new() -> Self {
        Self::with_engine(Engine::new())
    }

    /// A server over an existing engine.
    pub fn with_engine(engine: Engine) -> Self {
        Self {
            engine,
            tools: describe(),
        }
    }

    /// The engine, for inspection.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The `tools/list` entry of a command.
    fn tool(command: &CommandInfo) -> Value {
        json!({
            "name": tool_name(&command.id),
            "title": command.title,
            "description": format!("{}. Command `{}`. {}", command.title, command.id, command.description),
            "inputSchema": command.params_schema,
            "outputSchema": command.result_schema,
            "annotations": {
                "title": command.title,
                "readOnlyHint": !command.mutates,
                "openWorldHint": false,
            },
        })
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let Some(map) = params.as_object() else {
            return Err((
                INVALID_PARAMS,
                "`tools/call` needs params {name, arguments}".to_owned(),
            ));
        };
        let Some(name) = map.get("name").and_then(Value::as_str) else {
            return Err((
                INVALID_PARAMS,
                "`tools/call` needs a string `name`".to_owned(),
            ));
        };
        let Some(command) = self
            .tools
            .iter()
            .find(|c| c.id == name || tool_name(&c.id) == name)
            .map(|c| c.id.clone())
        else {
            let known: Vec<String> = self.tools.iter().map(|c| tool_name(&c.id)).collect();
            return Err((
                INVALID_PARAMS,
                format!("unknown tool `{name}`; the tools are: {}", known.join(", ")),
            ));
        };
        let arguments = map.get("arguments").cloned().unwrap_or(Value::Null);
        Ok(match self.engine.execute(&command, arguments) {
            Ok(value) => {
                let text =
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                let mut result = json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false,
                });
                if value.is_object() {
                    result["structuredContent"] = value;
                }
                result
            }
            Err(e) => json!({
                "content": [{ "type": "text", "text": format!("{}: {}", e.code, e.message) }],
                "isError": true,
            }),
        })
    }

    /// Handles one decoded message. `None` for a notification or a response (no answer owed).
    fn handle_message(&mut self, message: &Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error_response(
                Value::Null,
                INVALID_REQUEST,
                "a JSON-RPC message is an object",
            ));
        };
        // A response to a request of ours (we send none): ignore.
        if object.contains_key("result") || object.contains_key("error") {
            return None;
        }
        let id = object.get("id").cloned();
        let id_ok = matches!(
            &id,
            None | Some(Value::Null | Value::String(_) | Value::Number(_))
        );
        if !id_ok {
            return Some(error_response(
                Value::Null,
                INVALID_REQUEST,
                "`id` must be a string, a number or null",
            ));
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Some(error_response(
                id.unwrap_or(Value::Null),
                INVALID_REQUEST,
                "a request needs a string `method`",
            ));
        };
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error_response(
                id.unwrap_or(Value::Null),
                INVALID_REQUEST,
                "`jsonrpc` must be \"2.0\"",
            ));
        }
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        // No id: a notification (`notifications/initialized`, `notifications/cancelled`...).
        let id = id?;
        let outcome: Result<Value, (i64, String)> = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self.tools.iter().map(Self::tool).collect::<Vec<_>>(),
            })),
            "tools/call" => self.call_tool(&params),
            other => Err((
                METHOD_NOT_FOUND,
                format!("method `{other}` is not supported"),
            )),
        };
        Some(match outcome {
            Ok(result) => result_response(id, result),
            Err((code, message)) => error_response(id, code, &message),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = requested
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "caladrius-mcp",
                "title": "Caladrius",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": INSTRUCTIONS,
        })
    }

    /// Handles one line of input and returns the line to write back, if any. Never panics and
    /// never fails: bad input is answered with a JSON-RPC error.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let response = match serde_json::from_str::<Value>(line) {
            Err(e) => Some(error_response(
                Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
            )),
            // A batch (JSON-RPC 2.0, MCP 2024-11-05): answer each request, in order.
            Ok(Value::Array(items)) => {
                if items.is_empty() {
                    Some(error_response(
                        Value::Null,
                        INVALID_REQUEST,
                        "an empty batch",
                    ))
                } else {
                    let answers: Vec<Value> = items
                        .iter()
                        .filter_map(|m| self.handle_message(m))
                        .collect();
                    if answers.is_empty() {
                        None
                    } else {
                        Some(Value::Array(answers))
                    }
                }
            }
            Ok(message) => self.handle_message(&message),
        };
        response.map(|r| r.to_string())
    }

    /// Reads lines from `input` until it ends and writes the answers to `output`, flushing each.
    /// Bytes that are not UTF-8 are answered with a parse error. Returns when the input ends or
    /// the output cannot be written.
    pub fn serve<R: BufRead, W: Write>(
        &mut self,
        mut input: R,
        mut output: W,
    ) -> std::io::Result<()> {
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            if input.read_until(b'\n', &mut buffer)? == 0 {
                return Ok(());
            }
            let answer = match std::str::from_utf8(&buffer) {
                Ok(line) => self.handle_line(line),
                Err(_) => Some(
                    error_response(
                        Value::Null,
                        PARSE_ERROR,
                        "parse error: the line is not valid UTF-8",
                    )
                    .to_string(),
                ),
            };
            if let Some(line) = answer {
                output.write_all(line.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
    }
}

/// The tool table as `tools/list` returns it, without a session.
pub fn tools() -> Vec<Value> {
    describe().iter().map(Server::tool).collect()
}

#[cfg(test)]
mod tests;
