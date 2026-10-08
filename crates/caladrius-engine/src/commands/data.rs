//! `data.import`, `data.describe`, `data.set_column`, `data.set_cell`.

use caladrius_project::{
    AnalysisStatus, Column, ColumnData, ColumnRole, CsvOptions, ImportedTable, Project, Worksheet,
    WorksheetId,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CommandDef, parse, present, respond};
use crate::Engine;
use crate::error::CommandError;
use crate::schema::{
    array_of, boolean, described, integer, nullable, object, reference, root, string,
};

/// The worksheet as a machine and a person read it.
pub(crate) fn summary(ws: &Worksheet) -> Value {
    let columns: Vec<Value> = ws
        .columns()
        .iter()
        .map(|c| {
            let mut v = json!({
                "name": c.name,
                "role": c.role,
                "unit": c.unit,
                "type": match c.data { ColumnData::Number(_) => "number", ColumnData::Text(_) => "text" },
                "missing": c.data.n_missing(),
            });
            if let (ColumnData::Number(values), Some(map)) = (&c.data, v.as_object_mut()) {
                let present: Vec<f64> = values.iter().flatten().copied().collect();
                let min = present.iter().copied().reduce(f64::min);
                let max = present.iter().copied().reduce(f64::max);
                if let (Some(min), Some(max)) = (min, max) {
                    map.insert("min".to_owned(), json!(min));
                    map.insert("max".to_owned(), json!(max));
                }
            }
            v
        })
        .collect();
    json!({
        "id": ws.id(),
        "name": ws.name(),
        "revision": ws.revision(),
        "rows": ws.n_rows(),
        "subjects": ws.subjects(),
        "columns": columns,
        "unit_warnings": ws.unit_warnings(),
        "derived_units": ws.derived_units(),
    })
}

/// The analyses whose stored result is out of date.
pub(crate) fn stale_analyses(project: &Project) -> Vec<Value> {
    project
        .analyses()
        .iter()
        .filter(|a| matches!(project.status(a.id()), Ok(AnalysisStatus::Stale { .. })))
        .map(|a| json!(a.id()))
        .collect()
}

fn worksheet_schema() -> Value {
    object(
        vec![
            ("id", reference("Id")),
            ("name", string()),
            ("revision", integer()),
            ("rows", integer()),
            ("subjects", array_of(string())),
            (
                "columns",
                array_of(crate::schema::open_object(
                    vec![
                        ("name", string()),
                        ("role", reference("ColumnRole")),
                        ("unit", nullable(string())),
                        ("type", crate::schema::one_of_strings(&["number", "text"])),
                        ("missing", integer()),
                    ],
                    &["name", "role", "unit", "type", "missing"],
                )),
            ),
            ("unit_warnings", array_of(reference("UnitWarning"))),
            (
                "derived_units",
                json!({ "type": "object", "additionalProperties": string() }),
            ),
        ],
        &[
            "id",
            "name",
            "revision",
            "rows",
            "subjects",
            "columns",
            "unit_warnings",
            "derived_units",
        ],
    )
}

// ---- data.import -------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ColumnOverride {
    name: String,
    #[serde(default)]
    role: Option<ColumnRole>,
    #[serde(default, deserialize_with = "present")]
    unit: Option<Option<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    name: String,
    csv: String,
    #[serde(default)]
    delimiter: Option<char>,
    #[serde(default)]
    decimal_comma: Option<bool>,
    #[serde(default)]
    columns: Vec<ColumnOverride>,
}

fn import(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: ImportParams = parse("data.import", params)?;
    if p.name.trim().is_empty() {
        return Err(CommandError::invalid("data.import", "`name` is empty"));
    }
    let options = CsvOptions {
        delimiter: p.delimiter,
        decimal_comma: p.decimal_comma,
    };
    let mut table = ImportedTable::from_csv(p.csv.as_bytes(), &options)?;
    for o in &p.columns {
        override_column(&mut table.columns, o)?;
    }
    let id = engine.project.add_worksheet(&p.name, table.columns)?;
    let ws = engine.project.worksheet(id)?;
    respond(&json!({
        "worksheet": summary(ws),
        "delimiter": table.delimiter.to_string(),
        "notes": table.notes,
    }))
}

fn override_column(columns: &mut [Column], o: &ColumnOverride) -> Result<(), CommandError> {
    if !columns.iter().any(|c| c.name == o.name) {
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        return Err(CommandError::new(
            "unknown_column",
            format!(
                "the CSV has no column `{}`; its columns are: {}",
                o.name,
                names.join(", ")
            ),
        ));
    }
    if let Some(role) = o.role {
        if role != ColumnRole::Other {
            for c in columns.iter_mut().filter(|c| c.role == role) {
                c.role = ColumnRole::Other;
            }
        }
        if let Some(c) = columns.iter_mut().find(|c| c.name == o.name) {
            c.role = role;
        }
    }
    if let Some(unit) = &o.unit {
        if let Some(c) = columns.iter_mut().find(|c| c.name == o.name) {
            c.unit = unit
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_owned);
        }
    }
    Ok(())
}

pub(crate) const IMPORT: CommandDef = CommandDef {
    id: "data.import",
    title: "Import a CSV into a new worksheet",
    description: "Reads CSV text (header line, one line per row; comma, semicolon or tab; a decimal comma is understood with a semicolon) into a new worksheet. Roles (time, concentration, subject, dose, route) and units written in the headers, such as `Conc (ng/mL)`, are recognised; `notes` lists what was guessed or ignored. Use `columns` to set a role or a unit explicitly.",
    mutates: true,
    params: || {
        root(
            "data.import parameters",
            object(
                vec![
                    ("name", json!({ "type": "string", "minLength": 1 })),
                    ("csv", described(string(), "The CSV text.")),
                    (
                        "delimiter",
                        json!({ "type": "string", "minLength": 1, "maxLength": 1 }),
                    ),
                    ("decimal_comma", boolean()),
                    (
                        "columns",
                        described(
                            array_of(object(
                                vec![
                                    ("name", string()),
                                    ("role", reference("ColumnRole")),
                                    ("unit", nullable(string())),
                                ],
                                &["name"],
                            )),
                            "Explicit role or unit for named columns; a null unit clears it.",
                        ),
                    ),
                ],
                &["name", "csv"],
            ),
        )
    },
    result: || {
        root(
            "data.import result",
            object(
                vec![
                    ("worksheet", worksheet_schema()),
                    ("delimiter", string()),
                    ("notes", array_of(string())),
                ],
                &["worksheet", "delimiter", "notes"],
            ),
        )
    },
    example: || {
        json!({
            "name": "oral dose",
            "csv": "Time (h),Conc (mg/L),Dose (mg)\n0,0,100\n0.25,1.279,100\n0.5,2.195,100\n1,3.293,100\n2,3.971,100\n4,3.611,100\n6,2.989,100\n8,2.451,100\n12,1.643,100\n24,0.495,100\n",
        })
    },
    run: import,
};

// ---- data.describe -----------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeParams {
    worksheet: WorksheetId,
    #[serde(default = "default_preview")]
    preview_rows: usize,
}

fn default_preview() -> usize {
    5
}

fn describe(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: DescribeParams = parse("data.describe", params)?;
    let ws = engine.project.worksheet(p.worksheet)?;
    let shown = p.preview_rows.min(1000).min(ws.n_rows());
    let preview: Vec<Value> = (0..shown)
        .map(|row| {
            Value::Array(
                ws.columns()
                    .iter()
                    .map(|c| match &c.data {
                        ColumnData::Number(v) => json!(v.get(row).copied().flatten()),
                        ColumnData::Text(v) => json!(v.get(row)),
                    })
                    .collect(),
            )
        })
        .collect();
    respond(&json!({
        "worksheet": summary(ws),
        "preview": {
            "columns": ws.columns().iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "rows": preview,
        },
    }))
}

pub(crate) const DESCRIBE: CommandDef = CommandDef {
    id: "data.describe",
    title: "Describe a worksheet",
    description: "Columns with role, unit, type, missing count and range; subjects; unit warnings (dose, concentration and time checked together); the derived units (AUC, clearance, volume); and the first rows.",
    mutates: false,
    params: || {
        root(
            "data.describe parameters",
            object(
                vec![
                    ("worksheet", reference("Id")),
                    (
                        "preview_rows",
                        json!({ "type": "integer", "minimum": 0, "maximum": 1000 }),
                    ),
                ],
                &["worksheet"],
            ),
        )
    },
    result: || {
        root(
            "data.describe result",
            object(
                vec![
                    ("worksheet", worksheet_schema()),
                    (
                        "preview",
                        object(
                            vec![
                                ("columns", array_of(string())),
                                ("rows", array_of(json!({ "type": "array" }))),
                            ],
                            &["columns", "rows"],
                        ),
                    ),
                ],
                &["worksheet", "preview"],
            ),
        )
    },
    example: || json!({ "worksheet": 1 }),
    run: describe,
};

// ---- data.set_column ---------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetColumnParams {
    worksheet: WorksheetId,
    column: String,
    #[serde(default)]
    role: Option<ColumnRole>,
    #[serde(default, deserialize_with = "present")]
    unit: Option<Option<String>>,
}

fn set_column(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: SetColumnParams = parse("data.set_column", params)?;
    if p.role.is_none() && p.unit.is_none() {
        return Err(CommandError::invalid(
            "data.set_column",
            "give `role`, `unit` or both",
        ));
    }
    engine.project.edit_worksheet(p.worksheet, |w| {
        // All or nothing: work on a copy.
        let mut copy = w.clone();
        if let Some(role) = p.role {
            copy.set_role(&p.column, role)?;
        }
        if let Some(unit) = &p.unit {
            copy.set_unit(&p.column, unit.as_deref())?;
        }
        *w = copy;
        Ok(())
    })?;
    changed(engine, p.worksheet)
}

/// The answer of a command that edits a worksheet: the new state, and what became stale.
fn changed(engine: &Engine, id: WorksheetId) -> Result<Value, CommandError> {
    let ws = engine.project.worksheet(id)?;
    respond(&json!({
        "worksheet": summary(ws),
        "stale_analyses": stale_analyses(&engine.project),
    }))
}

fn changed_schema() -> Value {
    object(
        vec![
            ("worksheet", worksheet_schema()),
            (
                "stale_analyses",
                described(
                    array_of(reference("Id")),
                    "Analyses whose result no longer matches the data; run them again.",
                ),
            ),
        ],
        &["worksheet", "stale_analyses"],
    )
}

pub(crate) const SET_COLUMN: CommandDef = CommandDef {
    id: "data.set_column",
    title: "Set the role or the unit of a column",
    description: "Gives a column a role (time, concentration, subject, dose, route, other) and/or a unit. A role held by another column moves to this one. A change marks the results of the analyses that read the worksheet as stale.",
    mutates: true,
    params: || {
        root(
            "data.set_column parameters",
            object(
                vec![
                    ("worksheet", reference("Id")),
                    ("column", string()),
                    ("role", reference("ColumnRole")),
                    (
                        "unit",
                        described(
                            nullable(string()),
                            "The unit text; null or empty clears it.",
                        ),
                    ),
                ],
                &["worksheet", "column"],
            ),
        )
    },
    result: || root("data.set_column result", changed_schema()),
    example: || json!({ "worksheet": 1, "column": "Conc", "unit": "mg/L" }),
    run: set_column,
};

// ---- data.set_cell -----------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetCellParams {
    worksheet: WorksheetId,
    column: String,
    row: usize,
    value: Value,
}

fn set_cell(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: SetCellParams = parse("data.set_cell", params)?;
    engine.project.edit_worksheet(p.worksheet, |w| {
        let is_number = matches!(
            w.column(&p.column).map(|c| &c.data),
            Some(ColumnData::Number(_))
        );
        if is_number {
            let value = match &p.value {
                Value::Null => None,
                Value::Number(n) => n.as_f64(),
                other => {
                    return Err(caladrius_project::ProjectError {
                        code: "invalid_parameters".to_owned(),
                        message: format!(
                            "column `{}` holds numbers; `value` must be a number or null, not {other}",
                            p.column
                        ),
                    });
                }
            };
            w.set_number(&p.column, p.row, value)
        } else {
            let text = match &p.value {
                Value::Null => "",
                Value::String(s) => s.as_str(),
                other => {
                    return Err(caladrius_project::ProjectError {
                        code: "invalid_parameters".to_owned(),
                        message: format!(
                            "column `{}` holds text; `value` must be a string or null, not {other}",
                            p.column
                        ),
                    });
                }
            };
            w.set_text(&p.column, p.row, text)
        }
    })?;
    changed(engine, p.worksheet)
}

pub(crate) const SET_CELL: CommandDef = CommandDef {
    id: "data.set_cell",
    title: "Change one cell of a worksheet",
    description: "Sets the value of one cell (row counted from 0). A number column takes a number, or null for a missing value; a text column takes a string. The results of the analyses that read the worksheet become stale.",
    mutates: true,
    params: || {
        root(
            "data.set_cell parameters",
            object(
                vec![
                    ("worksheet", reference("Id")),
                    ("column", string()),
                    ("row", json!({ "type": "integer", "minimum": 0 })),
                    (
                        "value",
                        json!({ "anyOf": [{ "type": "number" }, { "type": "string" }, { "type": "null" }] }),
                    ),
                ],
                &["worksheet", "column", "row", "value"],
            ),
        )
    },
    result: || root("data.set_cell result", changed_schema()),
    example: || json!({ "worksheet": 1, "column": "Conc", "row": 3, "value": 3.3 }),
    run: set_cell,
};
