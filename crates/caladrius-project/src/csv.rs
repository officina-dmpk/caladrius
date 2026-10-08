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
    if k.starts_with("time") || matches!(k.as_str(), "t" | "hours" | "hour" | "hr" | "h") {
        Some(ColumnRole::Time)
    } else if k.starts_with("conc") || matches!(k.as_str(), "c" | "cp" | "dv" | "cobs") {
        Some(ColumnRole::Concentration)
    } else if matches!(
        k.as_str(),
        "subject" | "subj" | "id" | "subjectid" | "patient" | "animal" | "individual"
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
