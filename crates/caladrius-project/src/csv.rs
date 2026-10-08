//! CSV import: bytes in, columns out. No file access (the caller reads the file).
//!
//! The delimiter (comma, semicolon or tab) is detected from the header unless given. With a
//! delimiter other than a comma, a comma is read as the decimal mark (French spreadsheets).
//! Column types and roles are guessed from the data and the header; every guess that matters is
//! reported in `notes`, and every role can be changed afterwards.

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, Result};
use crate::worksheet::{Column, ColumnData, ColumnRole};

/// Options of a CSV import. The defaults detect everything.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CsvOptions {
    /// Field delimiter; `None` detects `,`, `;` or a tab from the header line.
    pub delimiter: Option<char>,
    /// Read `,` as the decimal mark; `None`: yes when the delimiter is not a comma.
    pub decimal_comma: Option<bool>,
}

/// The result of reading a CSV: the columns and what was guessed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedTable {
    /// The columns, in file order, with guessed roles and units.
    pub columns: Vec<Column>,
    /// The delimiter that was used.
    pub delimiter: char,
    /// Whether a comma was read as the decimal mark.
    pub decimal_comma: bool,
    /// Things to check: ignored text in a numeric column, roles not found, units read from
    /// headers.
    pub notes: Vec<String>,
}

impl ImportedTable {
    /// Reads UTF-8 CSV `bytes` (a header line, then one line per row).
    pub fn from_csv(bytes: &[u8], options: &CsvOptions) -> Result<ImportedTable> {
        let text = std::str::from_utf8(bytes).map_err(|e| {
            ProjectError::new(
                "csv_not_utf8",
                format!(
                    "the file is not valid UTF-8 (problem near byte {}); save it as CSV UTF-8",
                    e.valid_up_to()
                ),
            )
        })?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let delimiter = options.delimiter.unwrap_or_else(|| detect_delimiter(text));
        let decimal_comma = options.decimal_comma.unwrap_or(delimiter != ',');
        let records = records(text, delimiter)?;
        let mut rows = records.into_iter();
        let Some((_, header)) = rows.next() else {
            return Err(ProjectError::new(
                "csv_empty",
                "the file is empty; it needs a header line with the column names",
            ));
        };
        let width = header.len();
        let mut cells: Vec<Vec<String>> = vec![Vec::new(); width];
        for (line, row) in rows {
            if row.len() != width {
                return Err(ProjectError::new(
                    "csv_ragged_row",
                    format!(
                        "line {line} has {} fields but the header has {width}; check the delimiter and the quotes",
                        row.len()
                    ),
                ));
            }
            for (column, value) in cells.iter_mut().zip(row) {
                column.push(value);
            }
        }
        let mut notes = Vec::new();
        let mut columns = Vec::with_capacity(width);
        for (i, (head, values)) in header.iter().zip(cells).enumerate() {
            let (name, unit) = split_header(head, i);
            if let Some(u) = &unit {
                notes.push(format!(
                    "unit `{u}` read from the header of column `{name}`"
                ));
            }
            let data = type_column(&name, values, decimal_comma, &mut notes);
            columns.push(Column {
                name,
                role: ColumnRole::Other,
                unit,
                data,
            });
        }
        assign_roles(&mut columns, &mut notes);
        Ok(ImportedTable {
            columns,
            delimiter,
            decimal_comma,
            notes,
        })
    }
}

fn detect_delimiter(text: &str) -> char {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut in_quotes = false;
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    for c in line.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes {
            for (d, n) in &mut counts {
                if c == *d {
                    *n += 1;
                }
            }
        }
    }
    let mut best = (',', 0usize);
    for (d, n) in counts {
        if n > best.1 {
            best = (d, n);
        }
    }
    best.0
}

/// Splits into records of fields; each record keeps its 1-based line number. Blank lines are
/// skipped. Quoted fields may contain the delimiter, doubled quotes and line breaks.
fn records(text: &str, delimiter: char) -> Result<Vec<(usize, Vec<String>)>> {
    let mut out = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut was_quoted = false;
    let mut line = 1usize;
    let mut record_line = 1usize;
    let mut chars = text.chars().peekable();
    let mut finish = |fields: &mut Vec<String>, field: &mut String, record_line: usize| {
        fields.push(field.trim().to_owned());
        field.clear();
        let blank = fields.len() == 1 && fields.first().is_some_and(|f| f.is_empty());
        let done = std::mem::take(fields);
        if !blank {
            out.push((record_line, done));
        }
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                other => field.push(other),
            }
            continue;
        }
        match c {
            '"' if field.trim().is_empty() => {
                in_quotes = true;
                was_quoted = true;
                field.clear();
            }
            '\r' => {}
            '\n' => {
                finish(&mut fields, &mut field, record_line);
                line += 1;
                record_line = line;
                was_quoted = false;
            }
            d if d == delimiter => {
                fields.push(field.trim().to_owned());
                field.clear();
                was_quoted = false;
            }
            other => field.push(other),
        }
    }
    if in_quotes {
        return Err(ProjectError::new(
            "csv_unterminated_quote",
            format!("a quoted field that starts on line {record_line} is never closed"),
        ));
    }
    if !field.is_empty() || !fields.is_empty() || was_quoted {
        finish(&mut fields, &mut field, record_line);
    }
    Ok(out)
}

/// A header such as `Conc (ng/mL)` or `Time [h]` gives the name `Conc` and the unit `ng/mL`.
fn split_header(head: &str, index: usize) -> (String, Option<String>) {
    let head = head.trim();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let Some(inner_end) = head.strip_suffix(close) {
            if let Some(at) = inner_end.rfind(open) {
                let name = inner_end.get(..at).unwrap_or("").trim();
                let unit = inner_end.get(at + 1..).unwrap_or("").trim();
                if !name.is_empty() && !unit.is_empty() {
                    return (name.to_owned(), Some(unit.to_owned()));
                }
            }
        }
    }
    if head.is_empty() {
        (format!("column{}", index + 1), None)
    } else {
        (head.to_owned(), None)
    }
}

enum Cell {
    Missing,
    Number(f64),
    Text,
}

fn classify(raw: &str, decimal_comma: bool) -> Cell {
    let t = raw.trim();
    if t.is_empty() {
        return Cell::Missing;
    }
    let lower = t.to_ascii_lowercase();
    if matches!(lower.as_str(), "na" | "n/a" | "nan" | "." | "-" | "null") {
        return Cell::Missing;
    }
    let normalised;
    let candidate = if decimal_comma && t.contains(',') && !t.contains('.') {
        normalised = t.replace(',', ".");
        normalised.as_str()
    } else {
        t
    };
    match candidate.parse::<f64>() {
        Ok(x) if x.is_finite() => Cell::Number(x),
        _ => Cell::Text,
    }
}

/// One comma followed by exactly three digits after one to three digits that do not start with a
/// zero: written like a thousands separator, read as a decimal mark under a decimal comma.
fn looks_like_thousands(text: &str) -> bool {
    let t = text.trim().trim_start_matches(['+', '-']);
    t.split_once(',').is_some_and(|(int, frac)| {
        (1..=3).contains(&int.len())
            && frac.len() == 3
            && !int.starts_with('0')
            && int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit())
    })
}

fn type_column(
    name: &str,
    values: Vec<String>,
    decimal_comma: bool,
    notes: &mut Vec<String>,
) -> ColumnData {
    let cells: Vec<Cell> = values.iter().map(|v| classify(v, decimal_comma)).collect();
    let n_numbers = cells
        .iter()
        .filter(|c| matches!(c, Cell::Number(_)))
        .count();
    let n_text = cells.iter().filter(|c| matches!(c, Cell::Text)).count();
    if n_numbers > 0 && n_text == 0 {
        if decimal_comma {
            if let Some(example) = values.iter().find(|v| looks_like_thousands(v)) {
                notes.push(format!(
                    "column `{name}`: values such as `{example}` are read with the comma as the decimal mark; if the comma is a thousands separator, choose the decimal point and open the file again"
                ));
            }
        }
        return ColumnData::Number(
            cells
                .iter()
                .map(|c| match c {
                    Cell::Number(x) => Some(*x),
                    _ => None,
                })
                .collect(),
        );
    }
    if n_numbers > 0 {
        let example = cells
            .iter()
            .zip(&values)
            .find(|(c, _)| matches!(c, Cell::Text))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        notes.push(format!(
            "column `{name}` mixes numbers and text (`{example}`, {n_text} cell(s)); it is kept as text. Replace the text by numbers or leave the cells empty to use it as data"
        ));
    }
    ColumnData::Text(values)
}

fn key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn role_of(name: &str) -> Option<ColumnRole> {
    let k = key(name);
    if k.starts_with("time")
        || matches!(
            k.as_str(),
            "t" | "hours" | "hour" | "hr" | "h" | "temps" | "heures" | "heure"
        )
    {
        Some(ColumnRole::Time)
    } else if k.starts_with("conc") || matches!(k.as_str(), "c" | "cp" | "dv" | "cobs") {
        Some(ColumnRole::Concentration)
    } else if matches!(
        k.as_str(),
        "subject"
            | "subj"
            | "id"
            | "subjectid"
            | "patient"
            | "animal"
            | "individual"
            | "sujet"
            | "individu"
            | "volontaire"
    ) {
        Some(ColumnRole::Subject)
    } else if k.starts_with("dose") || matches!(k.as_str(), "amt" | "amount") {
        Some(ColumnRole::Dose)
    } else if matches!(k.as_str(), "route" | "admin" | "administration") {
        Some(ColumnRole::Route)
    } else {
        None
    }
}

fn assign_roles(columns: &mut [Column], notes: &mut Vec<String>) {
    for column in columns.iter_mut() {
        let Some(role) = role_of(&column.name) else {
            continue;
        };
        let numbers = matches!(column.data, ColumnData::Number(_));
        let usable = match role {
            ColumnRole::Time | ColumnRole::Concentration | ColumnRole::Dose => numbers,
            ColumnRole::Route => !numbers,
            ColumnRole::Subject => true,
            ColumnRole::Other => false,
        };
        if !usable {
            notes.push(format!(
                "column `{}` looks like the {} column but its content does not fit (text where numbers are needed, or the reverse); it was not given that role",
                column.name,
                role.id()
            ));
            continue;
        }
        column.role = role;
    }
    // At most one column per role: the first keeps it.
    let mut seen: Vec<ColumnRole> = Vec::new();
    for column in columns.iter_mut() {
        if column.role == ColumnRole::Other {
            continue;
        }
        if seen.contains(&column.role) {
            notes.push(format!(
                "column `{}` also looks like the {} column; it was left as `other`",
                column.name,
                column.role.id()
            ));
            column.role = ColumnRole::Other;
        } else {
            seen.push(column.role);
        }
    }
    for (role, what) in [
        (ColumnRole::Time, "time"),
        (ColumnRole::Concentration, "concentration"),
    ] {
        if !columns.iter().any(|c| c.role == role) {
            notes.push(format!(
                "no {what} column was recognised; set the role of the right column before analysing"
            ));
        }
    }
}

// ---- readings: the ways a file can be read, with plausibility checks (UX-IMP-01) ------------

/// A problem with one way of reading a file, found by looking at what it gives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingCheck {
    /// `time_not_increasing` or `duplicate_times`.
    pub code: String,
    /// What was found and what to do.
    pub message: String,
}

/// One admissible way of reading a CSV: a delimiter and a decimal mark, the table they give, and
/// what looks wrong in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// The table this reading gives (its `delimiter` and `decimal_comma` say how).
    pub table: ImportedTable,
    /// Problems of the result; a reading with none is plausible.
    pub checks: Vec<ReadingCheck>,
}

fn numbers_of(column: &Column) -> Option<&Vec<Option<f64>>> {
    match &column.data {
        ColumnData::Number(v) => Some(v),
        ColumnData::Text(_) => None,
    }
}

/// The checks on one table: within each subject the times must not go back, and must not repeat.
pub(crate) fn check_table(table: &ImportedTable) -> Vec<ReadingCheck> {
    let mut out = Vec::new();
    let Some(time) = table
        .columns
        .iter()
        .find(|c| c.role == ColumnRole::Time)
        .and_then(numbers_of)
    else {
        return out;
    };
    let subject = table.columns.iter().find(|c| c.role == ColumnRole::Subject);
    let label = |row: usize| subject.map_or_else(String::new, |s| s.data.text_at(row));
    // Rows by subject: a map from the label to its place, and the groups in order of appearance.
    let mut place: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut groups: Vec<(String, Vec<(usize, f64)>)> = Vec::new();
    for (row, t) in time.iter().enumerate() {
        let Some(t) = t else { continue };
        let key = label(row);
        match place.get(&key).and_then(|i| groups.get_mut(*i)) {
            Some((_, rows)) => rows.push((row, *t)),
            None => {
                place.insert(key.clone(), groups.len());
                groups.push((key, vec![(row, *t)]));
            }
        }
    }
    let of = |key: &str| {
        if key.is_empty() {
            String::new()
        } else {
            format!(" of subject {key}")
        }
    };
    let mut backwards = None;
    let mut repeated = None;
    for (key, rows) in &groups {
        for pair in rows.windows(2) {
            if let [(_, a), (row, b)] = pair {
                if b < a && backwards.is_none() {
                    backwards = Some(format!(
                        "time goes back at row {}{} ({b} after {a}); a profile is listed in time order",
                        row + 1,
                        of(key)
                    ));
                }
                if b == a && repeated.is_none() {
                    repeated = Some(format!(
                        "time {b} appears twice at row {}{}; duplicated times are an error for an NCA (keep one row, or give them to different subjects)",
                        row + 1,
                        of(key)
                    ));
                }
            }
        }
    }
    if let Some(message) = backwards {
        out.push(ReadingCheck {
            code: "time_not_increasing".to_owned(),
            message,
        });
    }
    if let Some(message) = repeated {
        out.push(ReadingCheck {
            code: "duplicate_times".to_owned(),
            message,
        });
    }
    out
}

/// Every admissible way of reading `bytes`: each delimiter (comma, semicolon, tab) with each
/// decimal mark (a decimal comma only with another delimiter) that gives a table with a numeric
/// time column and a numeric concentration column. Identical tables are listed once. The
/// readings without a failed check come first, the default reading (what [`ImportedTable::from_csv`]
/// does without options) before the others. Options that fix the delimiter or the decimal mark
/// restrict the list.
pub fn readings(bytes: &[u8], options: &CsvOptions) -> Result<Vec<Reading>> {
    let default = ImportedTable::from_csv(bytes, options);
    let mut tables: Vec<ImportedTable> = Vec::new();
    let mut first_error = None;
    match default {
        Ok(t) => tables.push(t),
        Err(e) => first_error = Some(e),
    }
    let delimiters = options.delimiter.map_or(vec![',', ';', '\t'], |d| vec![d]);
    for d in delimiters {
        let decimals = options.decimal_comma.map_or(
            if d == ',' {
                vec![false]
            } else {
                vec![false, true]
            },
            |c| vec![c],
        );
        for decimal_comma in decimals {
            let candidate = CsvOptions {
                delimiter: Some(d),
                decimal_comma: Some(decimal_comma),
            };
            if let Ok(t) = ImportedTable::from_csv(bytes, &candidate) {
                if !tables.iter().any(|x| x.columns == t.columns) {
                    tables.push(t);
                }
            }
        }
    }
    let usable = |t: &ImportedTable| {
        let numeric = |role| {
            t.columns
                .iter()
                .any(|c| c.role == role && numbers_of(c).is_some())
        };
        numeric(ColumnRole::Time) && numeric(ColumnRole::Concentration)
    };
    let mut out: Vec<Reading> = tables
        .into_iter()
        .filter(usable)
        .map(|table| {
            let checks = check_table(&table);
            Reading { table, checks }
        })
        .collect();
    if out.is_empty() {
        return Err(first_error.unwrap_or_else(|| {
            ProjectError::new(
                "csv_no_reading",
                "no reading of the file gives a numeric time column and a numeric concentration column; check the header names, the separator and the decimal mark",
            )
        }));
    }
    // Stable: plausible readings first, each group in the order found (the default first).
    out.sort_by_key(|r| !r.checks.is_empty());
    Ok(out)
}
