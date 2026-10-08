#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! App layer: the command-line application. It runs the engine's commands (`caladrius-engine`)
//! exactly as the UI and the MCP server do; this crate only reads arguments and files, calls
//! [`caladrius_engine::Engine::execute`] and prints the answer.
//!
//! ```text
//! caladrius-cli commands [--format text|json]
//! caladrius-cli <command-id> [--json FILE|-] [--param KEY=VALUE]... [--csv FILE]
//!               [--project FILE] [--format json|csv] [--table NAME]
//! ```
//!
//! Every failure is a one-line message on stderr and exit code 1.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use caladrius_engine::{CommandInfo, Engine, command_ids, describe};
use serde_json::{Map, Value, json};

/// Why the invocation failed. The message is exactly what the person reads.
#[derive(Debug, Clone, PartialEq)]
pub struct CliError(pub String);

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

fn fail<T>(message: impl Into<String>) -> Result<T, CliError> {
    Err(CliError(message.into()))
}

impl From<caladrius_engine::CommandError> for CliError {
    fn from(e: caladrius_engine::CommandError) -> Self {
        CliError(format!("{}: {}", e.code, e.message))
    }
}

/// What a successful invocation prints.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Output {
    /// For standard output, ends with a newline.
    pub stdout: String,
    /// Remarks for standard error, one per line: what an import guessed, unit warnings.
    pub notes: Vec<String>,
}

/// Output format of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Csv,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
struct RunArgs {
    command: String,
    json: Option<String>,
    params: Vec<String>,
    csv: Option<PathBuf>,
    project: Option<PathBuf>,
    format: Option<Format>,
    table: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Help,
    Commands(Option<Format>),
    Run(Box<RunArgs>),
}

/// The text printed by `help`.
pub const USAGE: &str = "\
usage:
  caladrius-cli commands [--format text|json]
  caladrius-cli <command-id> [options]

options for a command:
  --json FILE|-       parameters as a JSON object, from a file or from standard input
  --param KEY=VALUE   one parameter (repeatable, overrides --json). KEY may be dotted
                      (options.auc_method=linear). VALUE is JSON when it parses (1, true,
                      [1,2]), else text; @FILE reads the text of a file (csv=@data.csv)
  --csv FILE          import FILE as a worksheet first; commands that take a `worksheet`
                      use it when none is given
  --project FILE      load this project if it exists, and write it back after a command
                      that changes it
  --format json|csv   json (default): the command's result; csv: its table (an analysis
                      prints its main table, see --table)
  --table NAME        the table to print with --format csv (export.table names)

Every command is also listed, with the JSON schema of its parameters and results, by
`caladrius-cli commands --format json`. Failures print `error: ...` and exit with code 1.

example:
  caladrius-cli nca.run --csv oracle/data/theoph.csv --param route=extravascular --format csv";

fn parse_format(text: &str) -> Result<Format, CliError> {
    match text {
        "json" => Ok(Format::Json),
        "csv" => Ok(Format::Csv),
        "text" => Ok(Format::Text),
        other => fail(format!(
            "unknown format `{other}`; use json, csv or text (see `caladrius-cli help`)"
        )),
    }
}

/// Splits `--flag=value` and `--flag value`.
fn parse_args(args: &[String]) -> Result<Action, CliError> {
    let mut it = args.iter().peekable();
    let Some(first) = it.next() else {
        return fail(
            "missing command; run `caladrius-cli help`, or `caladrius-cli commands` for the list",
        );
    };
    if matches!(first.as_str(), "help" | "--help" | "-h") {
        return Ok(Action::Help);
    }
    let is_commands = first == "commands";
    let mut run = RunArgs {
        command: first.clone(),
        json: None,
        params: Vec::new(),
        csv: None,
        project: None,
        format: None,
        table: None,
    };
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_owned(), Some(v.to_owned())),
            _ => (arg.clone(), None),
        };
        if !flag.starts_with("--") {
            return fail(format!(
                "unexpected argument `{arg}`; options start with `--` (see `caladrius-cli help`)"
            ));
        }
        let value = |it: &mut std::iter::Peekable<std::slice::Iter<String>>| match inline {
            Some(v) => Ok(v),
            None => it
                .next()
                .cloned()
                .ok_or_else(|| CliError(format!("`{flag}` needs a value"))),
        };
        match flag.as_str() {
            "--json" if !is_commands => run.json = Some(value(&mut it)?),
            "--param" if !is_commands => run.params.push(value(&mut it)?),
            "--csv" if !is_commands => run.csv = Some(PathBuf::from(value(&mut it)?)),
            "--project" if !is_commands => run.project = Some(PathBuf::from(value(&mut it)?)),
            "--table" if !is_commands => run.table = Some(value(&mut it)?),
            "--format" => run.format = Some(parse_format(&value(&mut it)?)?),
            other => {
                return fail(format!(
                    "unknown option `{other}` (see `caladrius-cli help`)"
                ));
            }
        }
    }
    if is_commands {
        return Ok(Action::Commands(run.format));
    }
    Ok(Action::Run(Box::new(run)))
}

/// Parses `key=value` into a path and a JSON value (text when the value is not JSON).
fn parse_param(
    text: &str,
    read_file: &dyn Fn(&Path) -> Result<String, CliError>,
) -> Result<(Vec<String>, Value), CliError> {
    let Some((key, raw)) = text.split_once('=') else {
        return fail(format!("`--param {text}` must be KEY=VALUE"));
    };
    let path: Vec<String> = key.split('.').map(str::to_owned).collect();
    if key.is_empty() || path.iter().any(String::is_empty) {
        return fail(format!("`--param {text}` has an empty parameter name"));
    }
    let value = if let Some(file) = raw.strip_prefix('@') {
        Value::String(read_file(Path::new(file))?)
    } else {
        serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()))
    };
    Ok((path, value))
}

fn set_path(root: &mut Map<String, Value>, path: &[String], value: Value) -> Result<(), CliError> {
    match path {
        [] => Ok(()),
        [last] => {
            root.insert(last.clone(), value);
            Ok(())
        }
        [head, rest @ ..] => {
            let entry = root
                .entry(head.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            match entry {
                Value::Object(map) => set_path(map, rest, value),
                _ => fail(format!(
                    "`{head}` is not an object, so `{head}.{}` cannot be set",
                    rest.join(".")
                )),
            }
        }
    }
}

fn read_text(path: &Path) -> Result<String, CliError> {
    let bytes =
        fs::read(path).map_err(|e| CliError(format!("cannot read {}: {e}", path.display())))?;
    String::from_utf8(bytes).map_err(|_| {
        CliError(format!(
            "{} is not UTF-8 text; save it as UTF-8",
            path.display()
        ))
    })
}

/// Writes to a temporary file then renames, so an interrupted run never leaves half a project.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    fs::write(&temp, bytes)
        .map_err(|e| CliError(format!("cannot write {}: {e}", temp.display())))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        CliError(format!("cannot write {}: {e}", path.display()))
    })
}

fn to_json_text(value: &Value) -> Result<String, CliError> {
    serde_json::to_string_pretty(value)
        .map(|mut s| {
            s.push('\n');
            s
        })
        .map_err(|e| CliError(format!("cannot print the result: {e}")))
}

fn list_commands(format: Option<Format>) -> Result<Output, CliError> {
    let all = describe();
    let stdout = match format.unwrap_or(Format::Text) {
        Format::Json => to_json_text(&json!(all))?,
        Format::Text => {
            let width = all.iter().map(|c| c.id.len()).max().unwrap_or(0);
            let mut out = String::new();
            for c in &all {
                out.push_str(&format!("{:<width$}  {}\n", c.id, c.title));
            }
            out
        }
        Format::Csv => return fail("`commands` prints text or json, not csv"),
    };
    Ok(Output {
        stdout,
        notes: Vec::new(),
    })
}

/// The `worksheet` a command should use when none is given: the one just imported, or the only
/// one of the project.
fn default_worksheet(engine: &Engine, imported: Option<&Value>) -> Option<Value> {
    if let Some(id) = imported {
        return Some(id.clone());
    }
    match engine.project().worksheets() {
        [only] => Some(json!(only.id())),
        _ => None,
    }
}

fn takes(info: &CommandInfo, property: &str) -> bool {
    info.params_schema["properties"].get(property).is_some()
}

/// A table of one result as CSV.
fn csv_of(
    engine: &mut Engine,
    params: &Value,
    result: &Value,
    table: Option<&str>,
) -> Result<String, CliError> {
    // export.table prints its own table.
    if let Some(csv) = result.get("csv").and_then(Value::as_str) {
        return Ok(csv.to_owned());
    }
    // A simulation that was not stored: its curves.
    if table.is_none() {
        if let (Some(times), Some(conc), Some(auc)) = (
            result.get("times").and_then(Value::as_array),
            result.get("conc").and_then(Value::as_array),
            result.get("auc").and_then(Value::as_array),
        ) {
            let mut out = String::from("time,conc,auc\n");
            for ((t, c), a) in times.iter().zip(conc).zip(auc) {
                out.push_str(&format!(
                    "{},{},{}
",
                    number(t),
                    number(c),
                    number(a)
                ));
            }
            return Ok(out);
        }
    }
    let default_table = match result.get("kind").and_then(Value::as_str) {
        Some("nca") => Some("nca.parameters"),
        Some("fit") => Some("fit.parameters"),
        Some("simulation") => Some("simulation"),
        _ => None,
    };
    let analysis = result
        .get("id")
        .cloned()
        .or_else(|| params.get("analysis").cloned());
    let Some(table) = table.or(default_table) else {
        return fail(
            "this command has no table; use --format json, or --table NAME to export one (see `export.table`)",
        );
    };
    let mut export = json!({ "table": table });
    if table == "worksheet" {
        if let Some(ws) = params
            .get("worksheet")
            .cloned()
            .or_else(|| result["worksheet"].get("id").cloned())
        {
            export["worksheet"] = ws;
        }
    } else if let Some(id) = analysis {
        export["analysis"] = id;
    }
    let exported = engine.execute("export.table", export)?;
    Ok(exported["csv"].as_str().unwrap_or_default().to_owned())
}

fn run_command(args: &RunArgs, stdin: &mut dyn Read) -> Result<Output, CliError> {
    let Some(info) = describe().into_iter().find(|c| c.id == args.command) else {
        // The engine's own message lists the commands.
        let mut engine = Engine::new();
        return match engine.execute(&args.command, Value::Null) {
            Err(e) => Err(e.into()),
            Ok(_) => fail("unknown command"),
        };
    };
    // Parameters: the JSON object first, then each --param on top.
    let mut params = match &args.json {
        None => Map::new(),
        Some(source) => {
            let text = if source == "-" {
                let mut buffer = String::new();
                stdin
                    .read_to_string(&mut buffer)
                    .map_err(|e| CliError(format!("cannot read standard input: {e}")))?;
                buffer
            } else {
                read_text(Path::new(source))?
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(map)) => map,
                Ok(_) => return fail(format!("{source} must hold a JSON object of parameters")),
                Err(e) => return fail(format!("{source} is not valid JSON: {e}")),
            }
        }
    };
    for p in &args.params {
        let (path, value) = parse_param(p, &read_text)?;
        set_path(&mut params, &path, value)?;
    }

    let mut notes = Vec::new();
    let mut engine = Engine::new();
    if let Some(path) = &args.project {
        match fs::read(path) {
            Ok(bytes) => {
                engine.load_bytes(&bytes)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        }
    }
    let mut imported_id = None;
    if let Some(csv) = &args.csv {
        let bytes =
            fs::read(csv).map_err(|e| CliError(format!("cannot read {}: {e}", csv.display())))?;
        let name = csv
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("data")
            .to_owned();
        let imported = engine.import_csv(&name, &bytes)?;
        for note in imported["notes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            notes.push(format!("note: {note}"));
        }
        for w in imported["worksheet"]["unit_warnings"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(m) = w["message"].as_str() {
                notes.push(format!("warning: {m}"));
            }
        }
        imported_id = Some(imported["worksheet"]["id"].clone());
    }
    if takes(&info, "worksheet") && !params.contains_key("worksheet") {
        if let Some(id) = default_worksheet(&engine, imported_id.as_ref()) {
            params.insert("worksheet".to_owned(), id);
        }
    }
    let params = Value::Object(params);
    let result = engine.execute(&args.command, params.clone())?;

    let format = args.format.unwrap_or(Format::Json);
    let stdout = match format {
        Format::Json => to_json_text(&result)?,
        Format::Csv => csv_of(&mut engine, &params, &result, args.table.as_deref())?,
        Format::Text => return fail("`--format text` is only for `commands`; use json or csv"),
    };
    if format == Format::Json && args.table.is_some() {
        return fail("--table needs --format csv");
    }
    if let Some(path) = &args.project {
        if info.mutates || imported_id.is_some() {
            write_atomic(path, &engine.save_bytes()?)?;
        }
    }
    Ok(Output { stdout, notes })
}

/// Runs the command line `args` (without the program name). `stdin` is read only for `--json -`.
pub fn run(args: &[String], stdin: &mut dyn Read) -> Result<Output, CliError> {
    match parse_args(args)? {
        Action::Help => Ok(Output {
            stdout: format!("{USAGE}\n"),
            notes: Vec::new(),
        }),
        Action::Commands(format) => list_commands(format),
        Action::Run(run) => run_command(&run, stdin),
    }
}

/// The ids of the commands the CLI can run.
pub fn commands() -> Vec<&'static str> {
    command_ids()
}

#[cfg(test)]
mod tests;

/// A JSON number as CSV text: the shortest form that reads back (`2`, not `2.0`).
fn number(v: &Value) -> String {
    v.as_f64().map_or_else(|| v.to_string(), |x| format!("{x}"))
}
