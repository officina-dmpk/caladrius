//! `project.describe`, `project.save`, `project.load`, `project.replay`, `project.new`,
//! `history.list`.

use caladrius_project::Project;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CommandDef, parse, respond};
use crate::Engine;
use crate::error::CommandError;
use crate::export::slug;
use crate::history::{HistoryEntry, check_positions};
use crate::schema::{array_of, integer, nullable, object, one_of_strings, reference, root, string};

pub(super) fn overview(engine: &Engine) -> Result<Value, CommandError> {
    let project = &engine.project;
    let worksheets: Vec<Value> = project
        .worksheets()
        .iter()
        .map(|w| {
            json!({
                "id": w.id(),
                "name": w.name(),
                "rows": w.n_rows(),
                "columns": w.columns().len(),
                "subjects": w.subjects(),
                "revision": w.revision(),
            })
        })
        .collect();
    let mut analyses = Vec::new();
    for a in project.analyses() {
        analyses.push(json!({
            "id": a.id(),
            "label": project.label_of(a.id())?,
            "kind": a.spec().kind(),
            "status": project.status(a.id())?,
        }));
    }
    respond(&json!({
        "name": project.name(),
        "worksheets": worksheets,
        "analyses": analyses,
        "history_length": engine.history.len(),
    }))
}

pub(super) fn overview_schema() -> Value {
    object(
        vec![
            ("name", string()),
            (
                "worksheets",
                array_of(object(
                    vec![
                        ("id", reference("Id")),
                        ("name", string()),
                        ("rows", integer()),
                        ("columns", integer()),
                        ("subjects", array_of(string())),
                        ("revision", integer()),
                    ],
                    &["id", "name", "rows", "columns", "subjects", "revision"],
                )),
            ),
            (
                "analyses",
                array_of(object(
                    vec![
                        ("id", reference("Id")),
                        ("label", string()),
                        (
                            "kind",
                            one_of_strings(&["nca", "fit", "pop_fit", "simulation", "plot"]),
                        ),
                        ("status", reference("AnalysisStatus")),
                    ],
                    &["id", "label", "kind", "status"],
                )),
            ),
            ("history_length", integer()),
        ],
        &["name", "worksheets", "analyses", "history_length"],
    )
}

pub(crate) const DESCRIBE: CommandDef = CommandDef {
    id: "project.describe",
    title: "Describe the project",
    description: "The worksheets (with their revision) and the analyses (labelled from their content, with their status: no_result, fresh or stale), and the length of the history.",
    mutates: false,
    params: || root("project.describe parameters", object(vec![], &[])),
    result: || root("project.describe result", overview_schema()),
    example: || json!({}),
    run: |engine, params| {
        let _: Empty = parse("project.describe", params)?;
        overview(engine)
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

// ---- save and load -----------------------------------------------------------------------

/// The saved document: the project as the project layer writes it, with the history of the
/// commands executed so far under `history`.
pub(crate) fn document(engine: &Engine) -> Result<Value, CommandError> {
    let mut doc: Value = serde_json::from_slice(&engine.project.to_bytes()?).map_err(|e| {
        CommandError::new("save_failed", format!("the project cannot be saved: {e}"))
    })?;
    if let Some(map) = doc.as_object_mut() {
        map.insert("history".to_owned(), respond(&engine.history.entries())?);
    }
    Ok(doc)
}

fn save(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let _: Empty = parse("project.save", params)?;
    respond(&json!({
        "file_name": format!("{}.caladrius.json", slug(engine.project.name())),
        "project": document(engine)?,
    }))
}

pub(crate) const SAVE: CommandDef = CommandDef {
    id: "project.save",
    title: "Save the project",
    description: "The whole project (worksheets, analyses with options and results) and the history of the commands executed so far, with their seals (under `history`, without this save), as a JSON document, and a suggested file name; the caller writes the file. `Engine::save_bytes` gives the same document as bytes, including the save in the history. Stale results are saved as such.",
    mutates: false,
    params: || root("project.save parameters", object(vec![], &[])),
    result: || {
        root(
            "project.save result",
            object(
                vec![
                    ("file_name", string()),
                    ("project", json!({ "type": "object" })),
                ],
                &["file_name", "project"],
            ),
        )
    },
    example: || json!({}),
    run: save,
};

/// The parameters of the commands that take a saved document: the document itself, or its text.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentParams {
    #[serde(default)]
    project: Option<Value>,
    #[serde(default)]
    text: Option<String>,
}

/// A saved document taken apart: the project, and the history it carried as the reader found it
/// (`Err` holds what the reader said about it).
pub(crate) struct Saved {
    pub(crate) project: Project,
    pub(crate) history: Result<Vec<HistoryEntry>, String>,
}

/// The document written in `text`.
pub(crate) fn document_of_text(text: &str) -> Result<Value, CommandError> {
    serde_json::from_str(text).map_err(|e| {
        CommandError::new(
            "load_failed",
            format!("this is not a readable Caladrius project: {e}"),
        )
    })
}

/// Checks a saved document and takes it apart. A history that cannot be read is not an error
/// here: the project is what was asked for, and the caller decides what to do without the history.
pub(crate) fn read_saved(mut doc: Value) -> Result<Saved, CommandError> {
    let saved_history = doc.as_object_mut().and_then(|m| m.remove("history"));
    let history = match saved_history {
        None => Ok(Vec::new()),
        Some(h) => serde_json::from_value::<Vec<HistoryEntry>>(h).map_err(|e| e.to_string()),
    };
    let bytes = serde_json::to_vec(&doc).map_err(|e| {
        CommandError::new("load_failed", format!("the project cannot be read: {e}"))
    })?;
    let project = Project::from_bytes(&bytes)?;
    Ok(Saved { project, history })
}

fn load(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: DocumentParams = parse("project.load", params)?;
    let doc: Value = match (p.project, p.text) {
        (Some(v), None) => v,
        (None, Some(t)) => document_of_text(&t)?,
        _ => {
            return Err(CommandError::invalid(
                "project.load",
                "give exactly one of `project` (the document) and `text` (its JSON text)",
            ));
        }
    };
    // A history that cannot be read or is damaged is left out: the project itself is what the
    // person asked for, and the result says that the history was not restored.
    let Saved { project, history } = read_saved(doc)?;
    let (entries, mut history_note) = match history {
        Ok(entries) => (Some(entries), None),
        Err(e) => (
            None,
            Some(format!(
                "the saved history is not readable and was left out: {e}"
            )),
        ),
    };
    // Everything is checked: from here on nothing fails, so a refused file changes nothing.
    let restored = match entries.map(|e| engine.history.adopt(e)) {
        Some(Ok(restored)) => restored,
        Some(Err(e)) => {
            history_note = Some(format!("the saved history was left out: {}", e.message));
            false
        }
        None => false,
    };
    engine.project = project;
    let mut out = overview(engine)?;
    if let Some(map) = out.as_object_mut() {
        map.insert("history_restored".to_owned(), json!(restored));
        if let Some(note) = history_note {
            map.insert("history_note".to_owned(), json!(note));
        }
    }
    Ok(out)
}

pub(crate) const LOAD: CommandDef = CommandDef {
    id: "project.load",
    title: "Load a project",
    description: "Replaces the current project by a saved one, given as the document (`project`) or its JSON text (`text`). The file is checked first; a refused file changes nothing. The history saved with the project is adopted when this session has none yet (`history_restored`), so a project keeps its provenance across sessions; a session that already has a history keeps it, and loading is one more entry.",
    mutates: true,
    params: || {
        let mut schema = object(
            vec![("project", json!({ "type": "object" })), ("text", string())],
            &[],
        );
        if let Some(map) = schema.as_object_mut() {
            map.insert(
                "oneOf".to_owned(),
                json!([{ "required": ["project"] }, { "required": ["text"] }]),
            );
        }
        root("project.load parameters", schema)
    },
    result: || {
        let mut schema = overview_schema();
        if let Some(map) = schema.as_object_mut() {
            if let Some(Value::Object(props)) = map.get_mut("properties") {
                props.insert("history_restored".to_owned(), crate::schema::boolean());
                props.insert("history_note".to_owned(), string());
            }
            if let Some(Value::Array(req)) = map.get_mut("required") {
                req.push(json!("history_restored"));
            }
        }
        root("project.load result", schema)
    },
    example: || {
        json!({ "project": {
            "format": "caladrius-project",
            "format_version": 1,
            "name": "Loaded project",
            "worksheets": [],
            "analyses": [],
            "next_id": 1,
        } })
    },
    run: load,
};

// ---- replay ------------------------------------------------------------------------------

/// A saved document ready to replay: the project, and its history, which must be readable and
/// numbered from 1 without a gap.
fn to_replay(saved: Saved) -> Result<(Project, Vec<HistoryEntry>), CommandError> {
    let entries = saved.history.map_err(|e| {
        CommandError::new(
            "history_damaged",
            format!("the saved history is not readable: {e}"),
        )
    })?;
    check_positions(&entries).map_err(|message| CommandError::new("history_damaged", message))?;
    Ok((saved.project, entries))
}

fn replay(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: DocumentParams = parse("project.replay", params)?;
    let (project, entries) = match (p.project, p.text) {
        // Neither: the project and the history of this session.
        (None, None) => (engine.project.clone(), engine.history.entries().to_vec()),
        (Some(doc), None) => to_replay(read_saved(doc)?)?,
        (None, Some(text)) => to_replay(read_saved(document_of_text(&text)?)?)?,
        (Some(_), Some(_)) => {
            return Err(CommandError::invalid(
                "project.replay",
                "give at most one of `project` (the document) and `text` (its JSON text)",
            ));
        }
    };
    respond(&crate::replay::replay(
        &project,
        &entries,
        !engine.without_provenance,
    ))
}

fn hex_digest() -> Value {
    json!({ "type": "string", "pattern": "^[0-9a-f]{64}$" })
}

pub(crate) const REPLAY: CommandDef = CommandDef {
    id: "project.replay",
    title: "Replay a project and check its seals",
    description: "Runs the history of a saved project again, from an empty project and with the parameters the history kept, and checks every step against its stored seal. The project is given as the document (`project`) or its JSON text (`text`); with neither, the history of this session is replayed. Nothing of the current project is read or changed. The result lists each step with its stored seal, the seal computed now and its status (`match`; `diverged`; `no_seal` for a project saved before seals, or a command that carries none; `not_replayable` when the parameters were too large to be kept; `blocked` after such a step), the first step that diverges, and whether the project the steps produce is the project stored in the file (`state`). The verdict is `verified` (every sealed step reproduces its seal and the project is the stored one), `diverged`, `incomplete` (some steps could not be run again), `unsealed` (a project saved before seals) or `empty`. A history that cannot be read is the error `history_damaged`.",
    mutates: false,
    params: || {
        root(
            "project.replay parameters",
            object(
                vec![("project", json!({ "type": "object" })), ("text", string())],
                &[],
            ),
        )
    },
    result: || {
        let count = || json!({ "type": "integer", "minimum": 0 });
        root(
            "project.replay result",
            object(
                vec![
                    (
                        "verdict",
                        one_of_strings(&[
                            "verified",
                            "diverged",
                            "incomplete",
                            "unsealed",
                            "empty",
                        ]),
                    ),
                    ("summary", string()),
                    (
                        "counts",
                        object(
                            vec![
                                ("matched", count()),
                                ("diverged", count()),
                                ("no_seal", count()),
                                ("not_replayable", count()),
                                ("blocked", count()),
                            ],
                            &[
                                "matched",
                                "diverged",
                                "no_seal",
                                "not_replayable",
                                "blocked",
                            ],
                        ),
                    ),
                    ("first_divergence", nullable(reference("ReplayStep"))),
                    (
                        "state",
                        object(
                            vec![
                                ("matches", crate::schema::boolean()),
                                ("stored", hex_digest()),
                                ("replayed", hex_digest()),
                                ("differing", array_of(string())),
                                ("note", string()),
                            ],
                            &["differing"],
                        ),
                    ),
                    ("engine", reference("EngineId")),
                    ("sealed_by", array_of(reference("EngineId"))),
                    ("steps", array_of(reference("ReplayStep"))),
                ],
                &[
                    "verdict",
                    "summary",
                    "counts",
                    "first_divergence",
                    "state",
                    "sealed_by",
                    "steps",
                ],
            ),
        )
    },
    example: || json!({}),
    run: replay,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewParams {
    #[serde(default)]
    name: Option<String>,
}

fn new_project(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: NewParams = parse("project.new", params)?;
    engine.project = match p.name {
        Some(name) => Project::new(&name),
        None => Project::default(),
    };
    overview(engine)
}

pub(crate) const NEW: CommandDef = CommandDef {
    id: "project.new",
    title: "Start a new empty project",
    description: "Replaces the current project by an empty one. Save first if the current one matters. The history is kept.",
    mutates: true,
    params: || {
        root(
            "project.new parameters",
            object(vec![("name", string())], &[]),
        )
    },
    result: || root("project.new result", overview_schema()),
    example: || json!({ "name": "New project" }),
    run: new_project,
};

// ---- history -----------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryParams {
    #[serde(default = "first")]
    from: u64,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn first() -> u64 {
    1
}

fn default_limit() -> usize {
    100
}

fn history_list(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: HistoryParams = parse("history.list", params)?;
    let entries: Vec<_> = engine
        .history
        .entries()
        .iter()
        .filter(|e| e.seq >= p.from)
        .take(p.limit.min(1000))
        .collect();
    respond(&json!({ "total": engine.history.len(), "entries": entries }))
}

pub(crate) const HISTORY_LIST: CommandDef = CommandDef {
    id: "history.list",
    title: "List the executed commands",
    description: "The commands executed so far, oldest first, with their parameters and whether they succeeded, the seal of each answer (`seal`: the SHA-256 of the command, its parameters and its answer, and the engine that made it) and, when the program that runs the engine has a clock, the time (`at`). The history is append-only: entries are never changed or removed. Failed commands are recorded too.",
    mutates: false,
    params: || {
        root(
            "history.list parameters",
            object(
                vec![
                    ("from", json!({ "type": "integer", "minimum": 1 })),
                    (
                        "limit",
                        json!({ "type": "integer", "minimum": 1, "maximum": 1000 }),
                    ),
                ],
                &[],
            ),
        )
    },
    result: || {
        root(
            "history.list result",
            object(
                vec![
                    ("total", integer()),
                    (
                        "entries",
                        array_of(object(
                            vec![
                                ("seq", json!({ "type": "integer", "minimum": 1 })),
                                ("command", string()),
                                ("params", json!({})),
                                ("ok", crate::schema::boolean()),
                                (
                                    "error",
                                    nullable(object(
                                        vec![("code", string()), ("message", string())],
                                        &["code", "message"],
                                    )),
                                ),
                                ("changed_project", crate::schema::boolean()),
                                ("seal", crate::schema::reference("Seal")),
                                ("at", string()),
                            ],
                            &["seq", "command", "params", "ok", "error", "changed_project"],
                        )),
                    ),
                ],
                &["total", "entries"],
            ),
        )
    },
    example: || json!({ "from": 1, "limit": 20 }),
    run: history_list,
};
