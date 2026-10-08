//! Result tables: rows and columns of cells, rendered as CSV text or JSON. No file access: the
//! caller writes the text where it wants.

use caladrius_nca::NcaResult;
use caladrius_project::{
    Analysis, AnalysisResult, ColumnData, FitRun, NcaSubjectResult, Outcome, SimulationRun,
    Worksheet,
};
use serde_json::{Value, json};

use crate::error::CommandError;

/// The tables that can be exported, by id.
pub(crate) const TABLES: [&str; 8] = [
    "worksheet",
    "nca.parameters",
    "fit.parameters",
    "fit.values",
    "fit.observations",
    "fit.curve",
    "fit.trace",
    "simulation",
];

/// One cell.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cell {
    Empty,
    Number(f64),
    Text(String),
}

impl Cell {
    fn number(x: Option<f64>) -> Cell {
        match x {
            Some(v) if v.is_finite() => Cell::Number(v),
            _ => Cell::Empty,
        }
    }

    fn text(s: &str) -> Cell {
        Cell::Text(s.to_owned())
    }

    fn csv(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Number(x) => format!("{x}"),
            Cell::Text(s) => {
                if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s.clone()
                }
            }
        }
    }

    fn json(&self) -> Value {
        match self {
            Cell::Empty => Value::Null,
            Cell::Number(x) => json!(x),
            Cell::Text(s) => json!(s),
        }
    }
}

/// A table of cells with named columns.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Table {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<Cell>>,
}

impl Table {
    fn new(columns: &[&str]) -> Self {
        Self {
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            rows: Vec::new(),
        }
    }

    /// CSV text: header line, one line per row, `.` as decimal mark, `\n` line ends.
    pub(crate) fn to_csv(&self) -> String {
        let quote = |c: &String| Cell::Text(c.clone()).csv();
        let mut out = self.columns.iter().map(quote).collect::<Vec<_>>().join(",");
        out.push('\n');
        for row in &self.rows {
            out.push_str(&row.iter().map(Cell::csv).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        out
    }

    pub(crate) fn json_rows(&self) -> Vec<Value> {
        self.rows
            .iter()
            .map(|r| Value::Array(r.iter().map(Cell::json).collect()))
            .collect()
    }
}

/// The table of a worksheet: its columns as they are.
pub(crate) fn worksheet_table(ws: &Worksheet) -> Table {
    Table {
        columns: ws.columns().iter().map(|c| c.name.clone()).collect(),
        rows: (0..ws.n_rows())
            .map(|row| {
                ws.columns()
                    .iter()
                    .map(|c| match &c.data {
                        ColumnData::Number(v) => Cell::number(v.get(row).copied().flatten()),
                        ColumnData::Text(v) => match v.get(row) {
                            Some(s) if !s.is_empty() => Cell::text(s),
                            _ => Cell::Empty,
                        },
                    })
                    .collect()
            })
            .collect(),
    }
}

fn wrong(table: &str, analysis: &Analysis) -> CommandError {
    CommandError::new(
        "wrong_table_for_analysis",
        format!(
            "table `{table}` does not exist for a {} analysis; tables are: {}",
            analysis.spec().kind(),
            TABLES.join(", ")
        ),
    )
}

fn no_result(analysis: &Analysis) -> CommandError {
    CommandError::new(
        "no_result",
        format!(
            "analysis {} has not been run; run it before exporting its tables",
            analysis.id()
        ),
    )
}

fn reason_code(reason: caladrius_nca::NcReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn nca_parameters(subjects: &[NcaSubjectResult]) -> Table {
    let mut table = Table::new(&["subject", "parameter", "value", "not_calculated_reason"]);
    for s in subjects {
        match &s.outcome {
            Outcome::Ok(result) => table.rows.extend(nca_rows(&s.subject, result)),
            Outcome::Error(message) => table.rows.push(vec![
                Cell::text(&s.subject),
                Cell::Empty,
                Cell::Empty,
                Cell::Text(format!("error: {message}")),
            ]),
        }
    }
    table
}

fn nca_rows(subject: &str, result: &NcaResult) -> Vec<Vec<Cell>> {
    result
        .parameters()
        .iter()
        .map(|p| {
            vec![
                Cell::text(subject),
                Cell::text(&p.name),
                Cell::number(p.value.value()),
                p.value
                    .reason()
                    .map_or(Cell::Empty, |r| Cell::Text(reason_code(r))),
            ]
        })
        .collect()
}

fn fit_ok(run: &FitRun) -> Result<&caladrius_fit::FitResult, CommandError> {
    run.outcome.ok().ok_or_else(|| {
        CommandError::new(
            "no_result",
            "the fit did not produce a result; see the error of the analysis",
        )
    })
}

fn fit_parameters(run: &FitRun) -> Result<Table, CommandError> {
    let fit = fit_ok(run)?;
    let mut table = Table::new(&[
        "parameter",
        "estimate",
        "se",
        "cv_percent",
        "ci_lo",
        "ci_hi",
    ]);
    for p in fit.parameters() {
        let get = |stat: &str| Cell::number(fit.get(&format!("{stat}.{p}")));
        table.rows.push(vec![
            Cell::text(p),
            get("estimate"),
            get("se"),
            get("cv_percent"),
            get("ci_lo"),
            get("ci_hi"),
        ]);
    }
    Ok(table)
}

fn fit_values(run: &FitRun) -> Result<Table, CommandError> {
    let fit = fit_ok(run)?;
    let mut table = Table::new(&["name", "value"]);
    for (name, value) in fit.values() {
        table
            .rows
            .push(vec![Cell::text(name), Cell::number(Some(*value))]);
    }
    Ok(table)
}

fn fit_observations(run: &FitRun) -> Result<Table, CommandError> {
    let fit = fit_ok(run)?;
    let mut table = Table::new(&[
        "time",
        "observed",
        "predicted",
        "residual",
        "weighted_residual",
        "weight",
    ]);
    for o in fit.observations() {
        table.rows.push(
            [
                o.time,
                o.observed,
                o.predicted,
                o.residual,
                o.weighted_residual,
                o.weight,
            ]
            .into_iter()
            .map(|x| Cell::number(Some(x)))
            .collect(),
        );
    }
    Ok(table)
}

fn fit_curve(run: &FitRun) -> Result<Table, CommandError> {
    let fit = fit_ok(run)?;
    let mut table = Table::new(&["time", "conc"]);
    for p in fit.curve() {
        table
            .rows
            .push(vec![Cell::number(Some(p.time)), Cell::number(Some(p.conc))]);
    }
    Ok(table)
}

fn fit_trace(run: &FitRun) -> Result<Table, CommandError> {
    let fit = fit_ok(run)?;
    let mut columns: Vec<String> = vec!["iteration".to_owned(), "wrss".to_owned()];
    columns.extend(fit.parameters().iter().cloned());
    columns.extend(["step", "lambda", "halvings", "relative_decrease"].map(str::to_owned));
    let mut table = Table {
        columns,
        rows: Vec::new(),
    };
    for r in fit.trace() {
        let mut row = vec![
            Cell::number(Some(r.iteration as f64)),
            Cell::number(Some(r.wrss)),
        ];
        row.extend(
            fit.parameters()
                .iter()
                .enumerate()
                .map(|(i, _)| Cell::number(r.estimates.get(i).copied())),
        );
        row.push(Cell::number(r.step));
        row.push(Cell::number(Some(r.lambda)));
        row.push(Cell::number(Some(r.halvings as f64)));
        row.push(Cell::number(r.relative_decrease));
        table.rows.push(row);
    }
    Ok(table)
}

fn simulation(run: &SimulationRun) -> Result<Table, CommandError> {
    let out = run
        .outcome
        .ok()
        .ok_or_else(|| CommandError::new("no_result", "the simulation did not produce a result"))?;
    let mut table = Table::new(&["time", "conc", "auc"]);
    for ((t, c), a) in run.times.iter().zip(out.conc()).zip(out.auc()) {
        table.rows.push(vec![
            Cell::number(Some(*t)),
            Cell::number(Some(*c)),
            Cell::number(Some(*a)),
        ]);
    }
    Ok(table)
}

/// The table `name` of the last result of `analysis`.
pub(crate) fn analysis_table(name: &str, analysis: &Analysis) -> Result<Table, CommandError> {
    if !TABLES.contains(&name) {
        return Err(CommandError::invalid(
            "export.table",
            format!(
                "there is no table `{name}`; tables are: {}",
                TABLES.join(", ")
            ),
        ));
    }
    let result = analysis.result().ok_or_else(|| no_result(analysis))?;
    match (name, result) {
        ("nca.parameters", AnalysisResult::Nca { subjects }) => Ok(nca_parameters(subjects)),
        ("fit.parameters", AnalysisResult::Fit(run)) => fit_parameters(run),
        ("fit.values", AnalysisResult::Fit(run)) => fit_values(run),
        ("fit.observations", AnalysisResult::Fit(run)) => fit_observations(run),
        ("fit.curve", AnalysisResult::Fit(run)) => fit_curve(run),
        ("fit.trace", AnalysisResult::Fit(run)) => fit_trace(run),
        ("simulation", AnalysisResult::Simulation(run)) => simulation(run),
        _ => Err(wrong(name, analysis)),
    }
}

/// A file name made of letters, digits and dashes.
pub(crate) fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}
