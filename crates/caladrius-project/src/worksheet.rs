//! A worksheet: named columns of data, each with a role and a unit (units are data, golden rule 5).

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, Result};
use crate::units::{UnitWarning, derived_units, unit_warnings};

/// Stable identifier of a worksheet inside a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorksheetId(pub u64);

impl std::fmt::Display for WorksheetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The label of the only subject of a worksheet without a subject column.
pub const SINGLE_SUBJECT: &str = "1";

/// What a column means for the analyses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnRole {
    /// Sampling time since the dose (numbers).
    Time,
    /// Concentration (numbers; a missing value is an empty cell).
    Concentration,
    /// Subject label (text or numbers).
    Subject,
    /// Dose given at time 0 (numbers).
    Dose,
    /// Route of administration (text: `extravascular`, `iv_bolus`).
    Route,
    /// Anything else; not used by the analyses.
    Other,
}

impl ColumnRole {
    /// The stable id, as in the serialized form.
    pub fn id(&self) -> &'static str {
        match self {
            ColumnRole::Time => "time",
            ColumnRole::Concentration => "concentration",
            ColumnRole::Subject => "subject",
            ColumnRole::Dose => "dose",
            ColumnRole::Route => "route",
            ColumnRole::Other => "other",
        }
    }

    fn needs_numbers(&self) -> bool {
        matches!(
            self,
            ColumnRole::Time | ColumnRole::Concentration | ColumnRole::Dose
        )
    }
}

/// The values of a column. A number column has one `Option<f64>` per row, `None` for a missing
/// value (never NaN or infinity); a text column has one string per row, empty for missing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnData {
    /// Numbers.
    Number(Vec<Option<f64>>),
    /// Text.
    Text(Vec<String>),
}

impl ColumnData {
    /// Number of rows.
    pub fn len(&self) -> usize {
        match self {
            ColumnData::Number(v) => v.len(),
            ColumnData::Text(v) => v.len(),
        }
    }

    /// True without rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of missing values.
    pub fn n_missing(&self) -> usize {
        match self {
            ColumnData::Number(v) => v.iter().filter(|x| x.is_none()).count(),
            ColumnData::Text(v) => v.iter().filter(|s| s.is_empty()).count(),
        }
    }

    /// The cell of `row` as text (numbers shortest round trip), empty when missing or absent.
    pub fn text_at(&self, row: usize) -> String {
        match self {
            ColumnData::Number(v) => v
                .get(row)
                .copied()
                .flatten()
                .map(format_number)
                .unwrap_or_default(),
            ColumnData::Text(v) => v.get(row).cloned().unwrap_or_default(),
        }
    }
}

/// A number as a label: integers without a decimal point.
fn format_number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{x:.0}")
    } else {
        format!("{x}")
    }
}

/// One column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    /// Name, unique in the worksheet.
    pub name: String,
    /// Meaning for the analyses.
    pub role: ColumnRole,
    /// Unit as written by the user (`h`, `ng/mL`, `mg`); `None` when not given.
    pub unit: Option<String>,
    /// The values.
    pub data: ColumnData,
}

/// The data of one subject, ready for an analysis. Missing concentrations are NaN, as the NCA
/// engine expects (NCA-DAT-03).
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// Subject label.
    pub subject: String,
    /// Sampling times, in worksheet order.
    pub time: Vec<f64>,
    /// Concentrations, one per time; NaN when missing.
    pub conc: Vec<f64>,
    /// The dose of the subject, when the worksheet has a dose column with a value for it.
    pub dose: Option<f64>,
    /// The route text of the subject, when the worksheet has a route column.
    pub route: Option<String>,
}

/// A worksheet of a project. Changing its content bumps `revision`, which marks the results of
/// the analyses that read it as stale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Worksheet {
    id: WorksheetId,
    name: String,
    columns: Vec<Column>,
    revision: u64,
}

impl Worksheet {
    /// A worksheet, checked: at least one column, unique non-empty names, equal lengths, and
    /// every role consistent with the data (numbers for time, concentration and dose; at most
    /// one column per role other than `other`).
    pub(crate) fn new(id: WorksheetId, name: &str, columns: Vec<Column>) -> Result<Self> {
        let ws = Self {
            id,
            name: name.to_owned(),
            columns,
            revision: 0,
        };
        ws.validate()?;
        Ok(ws)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.columns.is_empty() {
            return Err(ProjectError::new(
                "no_columns",
                format!("worksheet `{}` has no column", self.name),
            ));
        }
        let rows = self.n_rows();
        for (i, c) in self.columns.iter().enumerate() {
            if c.name.trim().is_empty() {
                return Err(ProjectError::new(
                    "empty_column_name",
                    format!("column {} of worksheet `{}` has no name", i + 1, self.name),
                ));
            }
            if self.columns.iter().skip(i + 1).any(|d| d.name == c.name) {
                return Err(ProjectError::new(
                    "duplicate_column",
                    format!(
                        "worksheet `{}` has two columns named `{}`; rename one",
                        self.name, c.name
                    ),
                ));
            }
            if c.data.len() != rows {
                return Err(ProjectError::new(
                    "ragged_columns",
                    format!(
                        "column `{}` has {} rows but the first column has {rows}",
                        c.name,
                        c.data.len()
                    ),
                ));
            }
            if c.role.needs_numbers() && !matches!(c.data, ColumnData::Number(_)) {
                return Err(ProjectError::new(
                    "role_needs_numbers",
                    format!(
                        "column `{}` contains text and cannot be the {} column; replace the text (for example BLQ) by numbers",
                        c.name,
                        c.role.id()
                    ),
                ));
            }
            if c.role == ColumnRole::Route && !matches!(c.data, ColumnData::Text(_)) {
                return Err(ProjectError::new(
                    "role_needs_text",
                    format!(
                        "column `{}` holds numbers and cannot be the route column",
                        c.name
                    ),
                ));
            }
            if let ColumnData::Number(v) = &c.data {
                if v.iter().flatten().any(|x| !x.is_finite()) {
                    return Err(ProjectError::new(
                        "non_finite_value",
                        format!(
                            "column `{}` holds a value that is not a finite number",
                            c.name
                        ),
                    ));
                }
            }
            if c.role != ColumnRole::Other
                && self.columns.iter().skip(i + 1).any(|d| d.role == c.role)
            {
                return Err(ProjectError::new(
                    "duplicate_role",
                    format!(
                        "two columns of worksheet `{}` have the role {}; at most one column per role",
                        self.name,
                        c.role.id()
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Identifier in the project.
    pub fn id(&self) -> WorksheetId {
        self.id
    }

    /// Name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Revision counter: starts at 0, +1 for every change of content, role or unit.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The columns, in order.
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Number of rows.
    pub fn n_rows(&self) -> usize {
        self.columns.first().map_or(0, |c| c.data.len())
    }

    /// The column called `name`.
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// The column with this role.
    pub fn column_by_role(&self, role: ColumnRole) -> Option<&Column> {
        self.columns.iter().find(|c| c.role == role)
    }

    /// Renames the worksheet. Not a change of content: results stay fresh.
    pub fn rename(&mut self, name: &str) {
        self.name = name.to_owned();
    }

    fn column_index(&self, name: &str) -> Result<usize> {
        self.columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| {
                let names: Vec<&str> = self.columns.iter().map(|c| c.name.as_str()).collect();
                ProjectError::new(
                    "unknown_column",
                    format!(
                        "worksheet `{}` has no column `{name}`; its columns are: {}",
                        self.name,
                        names.join(", ")
                    ),
                )
            })
    }

    /// Gives a column a role. Another column that had the role becomes `other`. A column of text
    /// cannot get a numeric role.
    pub fn set_role(&mut self, column: &str, role: ColumnRole) -> Result<()> {
        let index = self.column_index(column)?;
        let mut candidate = self.clone();
        if role != ColumnRole::Other {
            for c in &mut candidate.columns {
                if c.role == role {
                    c.role = ColumnRole::Other;
                }
            }
        }
        if let Some(c) = candidate.columns.get_mut(index) {
            c.role = role;
        }
        candidate.validate()?;
        if candidate.columns != self.columns {
            candidate.revision = self.revision + 1;
            *self = candidate;
        }
        Ok(())
    }

    /// Sets or clears the unit of a column. An empty text clears it.
    pub fn set_unit(&mut self, column: &str, unit: Option<&str>) -> Result<()> {
        let index = self.column_index(column)?;
        let unit = unit
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(str::to_owned);
        if let Some(c) = self.columns.get_mut(index) {
            if c.unit != unit {
                c.unit = unit;
                self.revision += 1;
            }
        }
        Ok(())
    }

    /// Sets one cell of a number column; `None` makes it missing.
    pub fn set_number(&mut self, column: &str, row: usize, value: Option<f64>) -> Result<()> {
        let index = self.column_index(column)?;
        if value.is_some_and(|x| !x.is_finite()) {
            return Err(ProjectError::new(
                "non_finite_value",
                "a cell must be a finite number or empty (missing)",
            ));
        }
        let rows = self.n_rows();
        let name = self.name.clone();
        let Some(c) = self.columns.get_mut(index) else {
            return Ok(());
        };
        let ColumnData::Number(v) = &mut c.data else {
            return Err(ProjectError::new(
                "not_a_number_column",
                format!("column `{}` holds text; set a text value", c.name),
            ));
        };
        let Some(cell) = v.get_mut(row) else {
            return Err(ProjectError::new(
                "row_out_of_range",
                format!(
                    "worksheet `{name}` has {rows} rows; row {row} does not exist (rows start at 0)"
                ),
            ));
        };
        if *cell != value {
            *cell = value;
            self.revision += 1;
        }
        Ok(())
    }

    /// Sets one cell of a text column.
    pub fn set_text(&mut self, column: &str, row: usize, value: &str) -> Result<()> {
        let index = self.column_index(column)?;
        let rows = self.n_rows();
        let name = self.name.clone();
        let Some(c) = self.columns.get_mut(index) else {
            return Ok(());
        };
        let ColumnData::Text(v) = &mut c.data else {
            return Err(ProjectError::new(
                "not_a_text_column",
                format!(
                    "column `{}` holds numbers; set a number or leave it empty",
                    c.name
                ),
            ));
        };
        let Some(cell) = v.get_mut(row) else {
            return Err(ProjectError::new(
                "row_out_of_range",
                format!(
                    "worksheet `{name}` has {rows} rows; row {row} does not exist (rows start at 0)"
                ),
            ));
        };
        if cell != value {
            *cell = value.to_owned();
            self.revision += 1;
        }
        Ok(())
    }

    /// Subject labels in order of first appearance; the single label [`SINGLE_SUBJECT`] when the
    /// worksheet has no subject column. Rows with an empty subject belong to no subject.
    pub fn subjects(&self) -> Vec<String> {
        let Some(col) = self.column_by_role(ColumnRole::Subject) else {
            return vec![SINGLE_SUBJECT.to_owned()];
        };
        let mut out: Vec<String> = Vec::new();
        for row in 0..col.data.len() {
            let label = col.data.text_at(row);
            if !label.is_empty() && !out.contains(&label) {
                out.push(label);
            }
        }
        out
    }

    /// The data of one subject. `None` is accepted when the worksheet has a single subject.
    pub fn profile(&self, subject: Option<&str>) -> Result<Profile> {
        let subjects = self.subjects();
        let label = match subject {
            Some(s) => {
                if !subjects.iter().any(|x| x == s) {
                    return Err(ProjectError::new(
                        "unknown_subject",
                        format!(
                            "worksheet `{}` has no subject `{s}`; its subjects are: {}",
                            self.name,
                            subjects.join(", ")
                        ),
                    ));
                }
                s.to_owned()
            }
            None => match subjects.as_slice() {
                [only] => only.clone(),
                _ => {
                    return Err(ProjectError::new(
                        "subject_required",
                        format!(
                            "worksheet `{}` has {} subjects ({}); say which one",
                            self.name,
                            subjects.len(),
                            subjects.join(", ")
                        ),
                    ));
                }
            },
        };
        let time_col = self.column_by_role(ColumnRole::Time).ok_or_else(|| {
            ProjectError::new(
                "no_time_column",
                format!(
                    "worksheet `{}` has no time column; give a column the role time",
                    self.name
                ),
            )
        })?;
        let conc_col = self
            .column_by_role(ColumnRole::Concentration)
            .ok_or_else(|| {
                ProjectError::new(
                    "no_concentration_column",
                    format!(
                        "worksheet `{}` has no concentration column; give a column the role concentration",
                        self.name
                    ),
                )
            })?;
        let (ColumnData::Number(times), ColumnData::Number(concs)) =
            (&time_col.data, &conc_col.data)
        else {
            return Err(ProjectError::new(
                "role_needs_numbers",
                "the time and concentration columns must hold numbers",
            ));
        };
        let subject_col = self.column_by_role(ColumnRole::Subject);
        let dose_col = self.column_by_role(ColumnRole::Dose);
        let route_col = self.column_by_role(ColumnRole::Route);
        let mut profile = Profile {
            subject: label.clone(),
            time: Vec::new(),
            conc: Vec::new(),
            dose: None,
            route: None,
        };
        for row in 0..self.n_rows() {
            if let Some(sc) = subject_col {
                if sc.data.text_at(row) != label {
                    continue;
                }
            }
            let Some(t) = times.get(row).copied().flatten() else {
                return Err(ProjectError::new(
                    "missing_time",
                    format!(
                        "row {} of subject {label} has no time; fill it or delete the row (rows are counted from 1 here)",
                        row + 1
                    ),
                ));
            };
            profile.time.push(t);
            profile
                .conc
                .push(concs.get(row).copied().flatten().unwrap_or(f64::NAN));
            if let Some(dc) = dose_col {
                if let ColumnData::Number(d) = &dc.data {
                    if let Some(x) = d.get(row).copied().flatten() {
                        match profile.dose {
                            None => profile.dose = Some(x),
                            Some(first) if first != x => {
                                return Err(ProjectError::new(
                                    "dose_varies",
                                    format!(
                                        "subject {label} has two different doses ({first} and {x}) in column `{}`; one dose per profile",
                                        dc.name
                                    ),
                                ));
                            }
                            Some(_) => {}
                        }
                    }
                }
            }
            if profile.route.is_none() {
                if let Some(rc) = route_col {
                    let text = rc.data.text_at(row);
                    if !text.is_empty() {
                        profile.route = Some(text);
                    }
                }
            }
        }
        Ok(profile)
    }

    /// The unit of the column with this role.
    pub fn unit_of(&self, role: ColumnRole) -> Option<&str> {
        self.column_by_role(role).and_then(|c| c.unit.as_deref())
    }

    /// Checks of the time, concentration and dose units together (friction 5).
    pub fn unit_warnings(&self) -> Vec<UnitWarning> {
        unit_warnings(
            self.unit_of(ColumnRole::Time),
            self.unit_of(ColumnRole::Concentration),
            self.unit_of(ColumnRole::Dose),
        )
    }

    /// Names of the derived units (AUC, CL, V...) from the column units.
    pub fn derived_units(&self) -> std::collections::BTreeMap<String, String> {
        derived_units(
            self.unit_of(ColumnRole::Time),
            self.unit_of(ColumnRole::Concentration),
            self.unit_of(ColumnRole::Dose),
        )
    }
}
