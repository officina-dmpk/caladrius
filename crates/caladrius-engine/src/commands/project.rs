//! `project.describe`, `project.save`, `project.load`, `project.new`, `history.list`.

use caladrius_project::Project;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CommandDef, parse, respond};
use crate::Engine;
use crate::error::CommandError;
use crate::export::slug;
use crate::schema::{array_of, integer, nullable, object, one_of_strings, reference, root, string};

fn overview(engine: &Engine) -> Result<Value, CommandError> {
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

fn overview_schema() -> Value {
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
                        ("kind", one_of_strings(&["nca", "fit", "simulation"])),
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

fn save(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let _: Empty = parse("project.save", params)?;
    let project: Value = serde_json::from_slice(&engine.project.to_bytes()?).map_err(|e| {
        CommandError::new("save_failed", format!("the project cannot be saved: {e}"))
    })?;
    respond(&json!({
        "file_name": format!("{}.caladrius.json", slug(engine.project.name())),
        "project": project,
    }))
}

pub(crate) const SAVE: CommandDef = CommandDef {
    id: "project.save",
    title: "Save the project",
    description: "The whole project (worksheets, analyses with options and results) as a JSON document and a suggested file name; the caller writes the file. `Engine::save_bytes` gives the same document as bytes. Stale results are saved as such.",
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadParams {
    #[serde(default)]
    project: Option<Value>,
    #[serde(default)]
    text: Option<String>,
}

fn load(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: LoadParams = parse("project.load", params)?;
    let bytes = match (p.project, p.text) {
        (Some(v), None) => serde_json::to_vec(&v).map_err(|e| {
            CommandError::invalid("project.load", format!("`project` cannot be read: {e}"))
        })?,
        (None, Some(t)) => t.into_bytes(),
        _ => {
            return Err(CommandError::invalid(
                "project.load",
                "give exactly one of `project` (the document) and `text` (its JSON text)",
            ));
        }
    };
    engine.project = Project::from_bytes(&bytes)?;
    overview(engine)
}

pub(crate) const LOAD: CommandDef = CommandDef {
    id: "project.load",
    title: "Load a project",
    description: "Replaces the current project by a saved one, given as the document (`project`) or its JSON text (`text`). The file is checked first; a refused file changes nothing. The history is not replaced: loading is one more entry.",
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
    result: || root("project.load result", overview_schema()),
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
    description: "The commands executed so far, oldest first, with their parameters and whether they succeeded. The history is append-only: entries are never changed or removed. Failed commands are recorded too.",
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
