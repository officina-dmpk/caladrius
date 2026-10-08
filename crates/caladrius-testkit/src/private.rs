//! Private-oracle support (`AGENTS.md` section 5, source 2): reading the tables exported by the
//! reference software into `private/exports/<td>/`, the coursework data in `private/coursework/`,
//! and comparing values "at the precision displayed in the export".
//!
//! Nothing here contains, and nothing here prints, a number, a file name or a column name taken
//! from `private/`: errors and reports speak of counts, positions and canonical parameter names
//! only (the canonical names are the project's own PKNCA-style vocabulary). The format of the
//! exports was not known when this module was written (task T-019); the reader is tolerant to
//! what a spreadsheet export can look like:
//!
//! - delimiter `,`, `;` or tab, double-quoted cells, a byte-order mark, blank lines, title lines;
//! - decimal point or decimal comma, exponent notation, a unit suffix or space inside a number
//!   is NOT guessed at (such a cell is reported as unreadable, not as a mismatch);
//! - a long layout (one row per parameter: a name column and a value column, an optional unit
//!   column) or a wide layout (a header row of parameter names, then a row per profile or per
//!   summary statistic, possibly with a units row under the header);
//! - for summary tables of a single profile, the row labelled mean (else median, minimum).
//!
//! Besides text tables the reader handles what the reference software actually writes (first
//! export, task T-019): `.xls` worksheets (OLE/BIFF, read with the `calamine` crate; their numbers
//! are stored at full double precision, so they are compared with a relative tolerance and not at
//! a displayed precision), and `.rtf` text reports whose "Final Parameters" block is read as
//! displayed (four decimals). A worksheet of the profile with its cumulative areas, predictions and
//! residuals (the summary) is compared row by row; a settings file is read for the conventions it
//! records.
//!
//! The mapping from the export's parameter names to the project's names ([`canonical_name`]) is an
//! `assumed` table, to be corrected against the first real export; names it does not know are
//! counted as unmapped, never as failures.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::oracle::oracle_dir;
use crate::tolerance::Tolerance;

/// Error while reading private material. Messages never contain content from the files.
#[derive(Debug, Clone, PartialEq)]
pub enum PrivateError {
    /// A file or folder could not be read (`what` names the role, not the path).
    Io { what: String, kind: String },
    /// A table does not have a shape the reader understands (`what` names the role).
    Format { what: String, why: String },
}

impl fmt::Display for PrivateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrivateError::Io { what, kind } => write!(f, "cannot read {what}: {kind}"),
            PrivateError::Format { what, why } => write!(f, "{what}: {why}"),
        }
    }
}

impl std::error::Error for PrivateError {}

fn io_error(what: &str, e: &std::io::Error) -> PrivateError {
    PrivateError::Io {
        what: what.to_string(),
        kind: format!("{:?}", e.kind()),
    }
}

fn format_error(what: &str, why: &str) -> PrivateError {
    PrivateError::Format {
        what: what.to_string(),
        why: why.to_string(),
    }
}

// ---------------------------------------------------------------- locations

/// The `private/` directory of this repository.
pub fn private_dir() -> PathBuf {
    oracle_dir()
        .parent()
        .map_or_else(|| PathBuf::from("private"), |root| root.join("private"))
}

/// `private/exports/<td>/`.
pub fn exports_dir(td: &str) -> PathBuf {
    private_dir().join("exports").join(td)
}

/// `private/coursework/`.
pub fn coursework_dir() -> PathBuf {
    private_dir().join("coursework")
}

// ---------------------------------------------------------------- cases

/// A private case: coursework data and the settings the exercise was run with. The numbers of the
/// data are read at run time from `private/`; only the dose and the route, stated in the task
/// card, are here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrivateCase {
    /// The label of the exercise (`td1`), also the folder under `private/exports/` and the prefix
    /// of the coursework file name.
    pub id: &'static str,
    /// Dose, in the unit of the exercise.
    pub dose: f64,
    /// `"extravascular"`, `"iv_bolus"` or `"iv_infusion"`.
    pub route: &'static str,
}

/// The private cases known to the project.
pub const PRIVATE_CASES: &[PrivateCase] = &[PrivateCase {
    id: "td1",
    // 10 mg expressed in micrograms: the exports state concentrations in micrograms per millilitre
    // and the dose in micrograms, so clearance and volumes come out in the units of the export.
    dose: 10_000.0,
    route: "extravascular",
}];

/// Time and concentration of a coursework profile.
#[derive(Debug, Clone, PartialEq)]
pub struct CourseworkProfile {
    pub time: Vec<f64>,
    pub conc: Vec<f64>,
}

/// Reads the coursework profile of `case`: the first CSV file of `private/coursework/` whose name
/// starts with the case id, with a header and two columns (time, concentration). `Ok(None)` when
/// the folder or the file does not exist.
pub fn load_coursework_profile(
    case: &PrivateCase,
) -> Result<Option<CourseworkProfile>, PrivateError> {
    let dir = coursework_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(None);
    };
    let mut names: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            name.starts_with(case.id) && name.ends_with(".csv")
        })
        .collect();
    names.sort();
    let Some(path) = names.first() else {
        return Ok(None);
    };
    let text = fs::read_to_string(path).map_err(|e| io_error("the coursework data", &e))?;
    let table = split_table(&text, "the coursework data")?;
    let mut time = Vec::new();
    let mut conc = Vec::new();
    for row in table.rows.iter().skip(1) {
        let (Some(t), Some(c)) = (row.first(), row.get(1)) else {
            return Err(format_error(
                "the coursework data",
                "a row has fewer than two cells",
            ));
        };
        let (Some(t), Some(c)) = (parse_displayed(t), parse_displayed(c)) else {
            return Err(format_error(
                "the coursework data",
                "a time or concentration is not a number",
            ));
        };
        time.push(t.value);
        conc.push(c.value);
    }
    if time.is_empty() {
        return Err(format_error("the coursework data", "no data rows"));
    }
    Ok(Some(CourseworkProfile { time, conc }))
}

// ---------------------------------------------------------------- reading a table

/// A table of text cells.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawTable {
    pub rows: Vec<Vec<String>>,
    /// The numbers come from a worksheet: stored at full double precision, not as displayed.
    pub exact: bool,
}

fn split_cells(line: &str, delimiter: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == delimiter {
            cells.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(c);
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

fn count_outside_quotes(line: &str, delimiter: char) -> usize {
    let mut quoted = false;
    let mut n = 0;
    for c in line.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c == delimiter && !quoted {
            n += 1;
        }
    }
    n
}

/// Splits text into rows of cells. The delimiter is the one among tab, `;` and `,` that occurs
/// most often in the first line with a delimiter (ties go to tab, then `;`). Blank lines are
/// dropped. `what` names the table in errors.
pub fn split_table(text: &str, what: &str) -> Result<RawTable, PrivateError> {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return Err(format_error(what, "the table is empty"));
    }
    let mut best = (',', 0usize);
    for delimiter in ['\t', ';', ','] {
        let n = lines
            .iter()
            .take(10)
            .map(|l| count_outside_quotes(l, delimiter))
            .max()
            .unwrap_or(0);
        if n > best.1 {
            best = (delimiter, n);
        }
    }
    Ok(RawTable {
        rows: lines.iter().map(|l| split_cells(l, best.0)).collect(),
        exact: false,
    })
}

// ---------------------------------------------------------------- numbers as displayed

/// A number as it is written in an export, with the half unit of its last displayed digit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Displayed {
    pub value: f64,
    /// Half a unit in the last displayed place: two values that round to the same displayed text
    /// differ by at most this.
    pub half_unit: f64,
    /// Decimal places shown in the mantissa.
    pub decimals: u32,
}

/// Reads a displayed number: optional sign, digits with a decimal point or a decimal comma, an
/// optional exponent (`e` or `E`). Anything else (units, `NC`, blank, a thousands separator) is
/// `None`.
pub fn parse_displayed(text: &str) -> Option<Displayed> {
    let t = text.trim().replace('\u{a0}', "");
    if t.is_empty() {
        return None;
    }
    let (mantissa, exponent) = match t.find(['e', 'E']) {
        Some(i) => (&t[..i], Some(&t[i + 1..])),
        None => (&t[..], None),
    };
    let exponent: i32 = match exponent {
        Some(e) => e.trim().trim_start_matches('+').parse().ok()?,
        None => 0,
    };
    let normalised = if mantissa.contains('.') {
        if mantissa.contains(',') {
            return None;
        }
        mantissa.to_string()
    } else {
        if mantissa.matches(',').count() > 1 {
            return None;
        }
        mantissa.replace(',', ".")
    };
    let digits = normalised.trim_start_matches(['+', '-']);
    if digits.is_empty()
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        || digits.matches('.').count() > 1
    {
        return None;
    }
    let decimals = u32::try_from(digits.split_once('.').map_or(0, |(_, frac)| frac.len())).ok()?;
    let mantissa_value: f64 = normalised.parse().ok()?;
    let value = mantissa_value * 10f64.powi(exponent);
    if !value.is_finite() {
        return None;
    }
    let half_unit = 0.5 * 10f64.powi(exponent - i32::try_from(decimals).ok()?);
    Some(Displayed {
        value,
        half_unit,
        decimals,
    })
}

/// How a value compares with a displayed number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Equal at the displayed precision.
    Match,
    /// Different at the displayed precision.
    Mismatch,
    /// The text is not a number (blank, `NC`, units...).
    Unreadable,
}

/// Equality at the precision displayed in the export (`AGENTS.md` section 5): the value, rounded
/// to the places of the text, equals the text, that is `|actual - shown| <= half a unit of the
/// last displayed place` (with a margin of 1e-9 of that unit for the binary rounding of the
/// decimal text).
pub fn compare_displayed(text: &str, actual: f64) -> Outcome {
    match parse_displayed(text) {
        None => Outcome::Unreadable,
        Some(shown) => {
            if actual.is_finite() && (actual - shown.value).abs() <= shown.half_unit * (1.0 + 1e-9)
            {
                Outcome::Match
            } else {
                Outcome::Mismatch
            }
        }
    }
}

// ---------------------------------------------------------------- parameter names

fn normalise_name(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase()
}

/// The project's name (PKNCA spelling, see `oracle/expected/*.options.json`) of a parameter as the
/// reference software names it in an export. An `assumed` table, to be corrected against the first
/// real export; `None` for names it does not know.
pub fn canonical_name(raw: &str) -> Option<&'static str> {
    Some(match normalise_name(raw).as_str() {
        "cmax" => "cmax",
        "tmax" => "tmax",
        "tlag" => "tlag",
        "tlast" => "tlast",
        "clast" => "clast.obs",
        "clastpred" => "clast.pred",
        "lambdaz" => "lambda.z",
        "lambdazlower" => "lambda.z.time.first",
        "lambdazupper" => "lambda.z.time.last",
        "lambdazintercept" => "lambda.z.intercept",
        "corrxy" => "corr.xy",
        "nsamples" => "n.samples",
        "dose" => "dose",
        "cmaxd" => "cmax.dn",
        "auclastd" => "auclast.dn",
        "aucalld" => "aucall.dn",
        "aucinfdobs" => "aucinf.obs.dn",
        "aucinfdpred" => "aucinf.pred.dn",
        "aumcextrapobs" | "aumcpercentextrapobs" => "aumcpext.obs",
        "aumcextrappred" | "aumcpercentextrappred" => "aumcpext.pred",
        "nopointslambdaz" | "nopointslambdaz1" | "npointslambdaz" => "lambda.z.n.points",
        "rsq" => "r.squared",
        "rsqadjusted" | "rsqadj" | "adjrsq" => "adj.r.squared",
        "hllambdaz" | "halflife" => "half.life",
        "span" => "span.ratio",
        "auclast" => "auclast",
        "aucall" => "aucall",
        "aucinfobs" => "aucinf.obs",
        "aucinfpred" => "aucinf.pred",
        "aucextrapobs" | "aucpercentextrapobs" => "aucpext.obs",
        "aucextrappred" | "aucpercentextrappred" => "aucpext.pred",
        "aumclast" => "aumclast",
        "aumcinfobs" => "aumcinf.obs",
        "aumcinfpred" => "aumcinf.pred",
        "mrtlast" => "mrt.last",
        "mrtinfobs" => "mrt.obs",
        "mrtinfpred" => "mrt.pred",
        "clfobs" => "cl.obs",
        "clfpred" => "cl.pred",
        "vzfobs" => "vz.obs",
        "vzfpred" => "vz.pred",
        _ => return None,
    })
}

// ---------------------------------------------------------------- entries of an export table

/// One value of an export table: the parameter, as named in the export, and the text of its value.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportEntry {
    pub raw_name: String,
    pub canonical: Option<&'static str>,
    pub text: String,
    /// Full double precision (from a worksheet): compared with a relative tolerance.
    pub exact: bool,
}

fn is_number(cell: &str) -> bool {
    parse_displayed(cell).is_some()
}

fn header_has(header: &[String], words: &[&str]) -> Option<usize> {
    header.iter().position(|h| {
        let n = normalise_name(h);
        words.iter().any(|w| n == *w || n.starts_with(w))
    })
}

fn statistic_rank(label: &str) -> Option<usize> {
    match normalise_name(label).as_str() {
        "mean" | "arithmeticmean" => Some(0),
        "median" => Some(1),
        "min" | "minimum" => Some(2),
        "max" | "maximum" => Some(3),
        _ => None,
    }
}

/// Extracts the parameter values of an export table, long or wide layout, as described in the
/// module documentation. An error says what shape was not understood, without quoting the table.
pub fn extract_entries(table: &RawTable, what: &str) -> Result<Vec<ExportEntry>, PrivateError> {
    // The header: the first row of at least two cells that holds a non-numeric cell and is
    // followed by a row of at least two cells (title lines have one filled cell).
    let filled = |row: &Vec<String>| row.iter().filter(|c| !c.is_empty()).count();
    let header_index = table
        .rows
        .iter()
        .position(|r| filled(r) >= 2 && r.iter().any(|c| !c.is_empty() && !is_number(c)))
        .ok_or_else(|| format_error(what, "no header row with at least two cells"))?;
    let header = &table.rows[header_index];
    let body = &table.rows[header_index + 1..];
    if body.is_empty() {
        return Err(format_error(what, "a header and no data row"));
    }
    let exact = table.exact;
    let entry = |name: &str, text: &str| ExportEntry {
        raw_name: name.to_string(),
        canonical: canonical_name(name),
        text: text.to_string(),
        exact,
    };

    // Long layout: a column of parameter names and a column of values.
    let name_col = header_has(
        header,
        &["parameter", "param", "name", "variable", "pkparameter"],
    );
    let value_col = header_has(header, &["value", "estimate", "result", "final"]);
    if let (Some(n), Some(v)) = (name_col, value_col) {
        if n != v {
            let entries: Vec<ExportEntry> = body
                .iter()
                .filter_map(|row| {
                    let name = row.get(n)?;
                    let value = row.get(v)?;
                    (!name.is_empty()).then(|| entry(name, value))
                })
                .collect();
            if entries.is_empty() {
                return Err(format_error(
                    what,
                    "a long-layout table without parameter rows",
                ));
            }
            return Ok(entries);
        }
    }

    // Wide layout: parameter names in the header, a profile or a statistic per row.
    let numeric_cells = |row: &Vec<String>| {
        row.iter()
            .enumerate()
            .skip(1)
            .filter(|(_, c)| is_number(c))
            .count()
    };
    let data_rows: Vec<&Vec<String>> = body.iter().filter(|r| numeric_cells(r) >= 1).collect();
    if data_rows.is_empty() {
        return Err(format_error(
            what,
            "no row with numeric values under the header",
        ));
    }
    // Several statistic rows: prefer the mean, then the median, minimum, maximum.
    let chosen = data_rows
        .iter()
        .filter_map(|r| r.first().and_then(|l| statistic_rank(l)).map(|k| (k, *r)))
        .min_by_key(|(k, _)| *k)
        .map(|(_, r)| r)
        .unwrap_or(data_rows[0]);
    let mut entries = Vec::new();
    for (j, name) in header.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let Some(text) = chosen.get(j) else { continue };
        if j == 0 && canonical_name(name).is_none() {
            continue; // the row label column (profile or statistic)
        }
        entries.push(entry(name, text));
    }
    Ok(entries)
}

// ---------------------------------------------------------------- comparing

/// Counts of an evaluation. Only counts: nothing here carries a value from an export.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrivateCounts {
    /// Equal to the engine value at the displayed precision.
    pub validated: usize,
    /// Different at the displayed precision (or beyond the relative tolerance of a full-precision
    /// cell): the canonical parameter name and the size of the difference.
    pub mismatched: Vec<Difference>,
    /// A known parameter that the engine does not compute, or reports as not calculated.
    pub engine_missing: Vec<&'static str>,
    /// Entries whose name the project does not know.
    pub unmapped: usize,
    /// Entries whose value text is not a number (blank, not calculated...).
    pub unreadable: usize,
}

/// A value that differs from the export: how, never by what value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Difference {
    pub name: &'static str,
    /// Decimals shown in the export (0 for a full-precision cell).
    pub decimals: u32,
    /// The difference in units of the last displayed place (for a displayed value), or as a
    /// relative error (for a full-precision cell).
    pub size: f64,
    /// `size` is a relative error of a full-precision cell.
    pub relative: bool,
}

impl Difference {
    /// Text for a report: the size only, no value.
    pub fn describe(&self) -> String {
        if self.relative {
            format!(
                "relative difference {:.1e} (full-precision cell)",
                self.size
            )
        } else {
            format!(
                "{:.1} units of the last displayed place ({} decimals)",
                self.size, self.decimals
            )
        }
    }
}

impl PrivateCounts {
    /// Entries that were compared or could have been: validated, mismatched and engine-missing.
    pub fn expected(&self) -> usize {
        self.validated + self.mismatched.len() + self.engine_missing.len()
    }

    /// Adds another count.
    pub fn add(&mut self, other: &PrivateCounts) {
        self.validated += other.validated;
        self.mismatched.extend(other.mismatched.iter().copied());
        self.engine_missing
            .extend(other.engine_missing.iter().copied());
        self.unmapped += other.unmapped;
        self.unreadable += other.unreadable;
    }
}

/// Compares the entries of an export with the engine's values (by canonical name; `None` for a
/// parameter the engine does not compute or reports as not calculated).
pub fn evaluate(entries: &[ExportEntry], engine: &BTreeMap<String, Option<f64>>) -> PrivateCounts {
    let mut counts = PrivateCounts::default();
    for e in entries {
        counts_one(&mut counts, e.canonical, &e.text, e.exact, |name| {
            engine.get(name).copied().flatten()
        });
    }
    counts
}

/// Relative tolerance for a cell stored at full double precision.
const FULL_PRECISION: Tolerance = Tolerance::Relative {
    rel: 1e-9,
    abs: 1e-12,
};

/// Compares one value with the text of an export and records the outcome in `counts`.
fn counts_one(
    counts: &mut PrivateCounts,
    canonical: Option<&'static str>,
    text: &str,
    exact: bool,
    engine: impl Fn(&str) -> Option<f64>,
) {
    let Some(name) = canonical else {
        counts.unmapped += 1;
        return;
    };
    let Some(shown) = parse_displayed(text) else {
        counts.unreadable += 1;
        return;
    };
    let Some(actual) = engine(name) else {
        counts.engine_missing.push(name);
        return;
    };
    let (ok, size) = if exact {
        let difference = (actual - shown.value).abs();
        let size = if shown.value != 0.0 {
            difference / shown.value.abs()
        } else {
            difference
        };
        (FULL_PRECISION.accepts(actual, shown.value), size)
    } else {
        (
            compare_displayed(text, actual) == Outcome::Match,
            (actual - shown.value).abs() / (2.0 * shown.half_unit),
        )
    };
    if ok {
        counts.validated += 1;
    } else {
        counts.mismatched.push(Difference {
            name,
            decimals: shown.decimals,
            size,
            relative: exact,
        });
    }
}

// ---------------------------------------------------------------- finding the exports

/// What a table is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExportKind {
    /// The "Final Parameters" worksheet.
    FinalParameters,
    /// The worksheet of the profile: cumulative areas, predictions, residuals, which points were
    /// used for the terminal phase.
    Summary,
    /// The text report of the analysis engine (RTF); its "Final Parameters" block is read.
    CoreOutput,
    /// The settings the analysis was run with (RTF); read for the conventions it records.
    Settings,
}

/// Which AUC method a table was run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MethodHint {
    /// Linear trapezoid with linear interpolation.
    Linear,
    /// Linear-up/log-down.
    LinUpLogDown,
}

impl MethodHint {
    /// Label for reports.
    pub fn label(&self) -> &'static str {
        match self {
            MethodHint::Linear => "linear",
            MethodHint::LinUpLogDown => "lin_up_log_down",
        }
    }
}

impl ExportKind {
    /// Label for reports.
    pub fn label(&self) -> &'static str {
        match self {
            ExportKind::FinalParameters => "final_parameters",
            ExportKind::Summary => "summary",
            ExportKind::CoreOutput => "core_output",
            ExportKind::Settings => "settings",
        }
    }
}

/// An export file found for an exercise, classified.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportFile {
    /// Position among the files found (for messages: names are private).
    pub index: usize,
    pub path: PathBuf,
    pub kind: ExportKind,
    pub method: MethodHint,
}

/// Result of looking for the exports of an exercise.
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    /// Files that were classified.
    pub files: Vec<ExportFile>,
    /// Number of files found that could not be classified (and no manifest said what they are).
    pub unclassified: usize,
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    file: String,
    /// `"linear"` or `"lin_up_log_down"`.
    method: String,
    /// `"final"` or `"summary"`.
    table: String,
}

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if depth < 2 {
                collect_files(&p, depth + 1, out);
            }
        } else {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if matches!(ext.as_str(), "csv" | "txt" | "tsv" | "xls" | "xlsx" | "rtf") {
                out.push(p);
            }
        }
    }
}

fn classify(path_text: &str) -> (Option<ExportKind>, Option<MethodHint>) {
    let t = path_text.to_lowercase();
    let kind = if t.contains("final") {
        Some(ExportKind::FinalParameters)
    } else if t.contains("summary") {
        Some(ExportKind::Summary)
    } else if t.contains("core") {
        Some(ExportKind::CoreOutput)
    } else if t.contains("setting") {
        Some(ExportKind::Settings)
    } else {
        None
    };
    let compact: String = t.chars().filter(char::is_ascii_alphanumeric).collect();
    let method = if (compact.contains("up") && compact.contains("down"))
        || compact.contains("ludl")
        || compact.contains("luld")
    {
        Some(MethodHint::LinUpLogDown)
    } else if compact.contains("linlog") || compact.contains("loglin") {
        None // the linear-log trapezoid is a different method: not classified
    } else if compact.contains("lin") {
        Some(MethodHint::Linear)
    } else {
        None
    };
    (kind, method)
}

/// Finds and classifies the exports of an exercise under `private/exports/<id>/` (files with the
/// extension csv, txt or tsv, up to two folders deep). Classification uses the path text (a
/// "final" or "summary" table; an "up"/"down" or "lin" method) or, if present, a `manifest.json`
/// in the folder: a list of `{"file": relative path, "method": "linear" | "lin_up_log_down",
/// "table": "final" | "summary"}`. Returns `Ok(None)` when there are no export files at all.
pub fn discover_exports(id: &str) -> Result<Option<Discovery>, PrivateError> {
    discover_in(&exports_dir(id))
}

/// [`discover_exports`] on an explicit folder.
pub fn discover_in(dir: &Path) -> Result<Option<Discovery>, PrivateError> {
    let dir = dir.to_path_buf();
    let mut found = Vec::new();
    collect_files(&dir, 0, &mut found);
    if found.is_empty() {
        return Ok(None);
    }
    let manifest: Vec<ManifestEntry> = match fs::read_to_string(dir.join("manifest.json")) {
        Ok(text) => serde_json::from_str(&text).map_err(|_| {
            format_error("the export manifest", "not a list of {file, method, table}")
        })?,
        Err(_) => Vec::new(),
    };
    let mut files = Vec::new();
    let mut unclassified = 0;
    for (index, path) in found.iter().enumerate() {
        let relative = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative.eq_ignore_ascii_case("manifest.json") {
            continue;
        }
        let from_manifest = manifest
            .iter()
            .find(|m| m.file.replace('\\', "/") == relative);
        let (kind, method) = match from_manifest {
            Some(m) => (
                match m.table.as_str() {
                    "final" => Some(ExportKind::FinalParameters),
                    "summary" => Some(ExportKind::Summary),
                    "core" => Some(ExportKind::CoreOutput),
                    "settings" => Some(ExportKind::Settings),
                    _ => None,
                },
                match m.method.as_str() {
                    "linear" => Some(MethodHint::Linear),
                    "lin_up_log_down" => Some(MethodHint::LinUpLogDown),
                    _ => None,
                },
            ),
            None => classify(&relative),
        };
        match (kind, method) {
            (Some(kind), Some(method)) => files.push(ExportFile {
                index,
                path: path.clone(),
                kind,
                method,
            }),
            _ => unclassified += 1,
        }
    }
    Ok(Some(Discovery {
        files,
        unclassified,
    }))
}

// ---------------------------------------------------------------- worksheets, reports, settings

/// Reads the first sheet of a spreadsheet file (`.xls` or `.xlsx`) as a table of text cells whose
/// numbers are marked as full precision.
pub fn read_worksheet(path: &Path, what: &str) -> Result<RawTable, PrivateError> {
    use calamine::{Data, Reader, open_workbook_auto};
    let mut workbook =
        open_workbook_auto(path).map_err(|_| format_error(what, "not a readable spreadsheet"))?;
    let name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| format_error(what, "a spreadsheet without sheets"))?;
    let range = workbook
        .worksheet_range(&name)
        .map_err(|_| format_error(what, "the first sheet cannot be read"))?;
    let rows = range
        .rows()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Data::Empty => String::new(),
                    Data::String(t) => t.trim().to_string(),
                    Data::Float(x) => format!("{x}"),
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    other => format!("{other}"),
                })
                .collect::<Vec<String>>()
        })
        .collect();
    Ok(RawTable { rows, exact: true })
}

/// Turns the text of an RTF report into plain text: paragraphs and tabs kept, control words,
/// groups marked `\*` and the font table dropped, `\'hh` escapes decoded as Latin-1.
pub fn rtf_to_text(rtf: &str) -> String {
    let chars: Vec<char> = rtf.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // Depth at which a destination to be skipped started (`{\*...}` or the font and colour tables).
    let mut depth = 0usize;
    let mut skip_from: Option<usize> = None;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => {
                depth += 1;
                i += 1;
                if chars.get(i) == Some(&'\\') {
                    let rest: String = chars[i + 1..chars.len().min(i + 12)].iter().collect();
                    if (rest.starts_with('*')
                        || rest.starts_with("fonttbl")
                        || rest.starts_with("colortbl"))
                        && skip_from.is_none()
                    {
                        skip_from = Some(depth);
                    }
                }
            }
            '}' => {
                if skip_from == Some(depth) {
                    skip_from = None;
                }
                depth = depth.saturating_sub(1);
                i += 1;
            }
            '\\' => {
                i += 1;
                let Some(&next) = chars.get(i) else { break };
                if next == '\'' {
                    let hex: String = chars[i + 1..chars.len().min(i + 3)].iter().collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        if skip_from.is_none() {
                            out.push(byte as char);
                        }
                    }
                    i += 3;
                } else if next.is_ascii_alphabetic() {
                    let start = i;
                    while i < chars.len() && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    if chars.get(i) == Some(&'-') {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if chars.get(i) == Some(&' ') {
                        i += 1;
                    }
                    if skip_from.is_none() {
                        match word.as_str() {
                            "par" | "line" => out.push('\n'),
                            "tab" => out.push('\t'),
                            _ => {}
                        }
                    }
                } else {
                    // An escaped symbol: \\ \{ \} are literal; others are ignored.
                    if matches!(next, '\\' | '{' | '}') && skip_from.is_none() {
                        out.push(next);
                    }
                    i += 1;
                }
            }
            '\r' | '\n' => i += 1, // line breaks in the RTF source are not paragraphs
            _ => {
                if skip_from.is_none() {
                    out.push(c);
                }
                i += 1;
            }
        }
    }
    out
}

/// The entries of the "Final Parameters" block of a text report: after a line reading "Final
/// Parameters" and its underline, lines of a name and a number, until a line that is not one.
pub fn report_final_parameters(text: &str) -> Vec<ExportEntry> {
    let mut entries = Vec::new();
    let mut lines = text
        .lines()
        .map(str::trim)
        .skip_while(|l| !l.eq_ignore_ascii_case("Final Parameters"));
    if lines.next().is_none() {
        return entries;
    }
    for line in lines {
        if line.is_empty() || line.chars().all(|c| c == '-') {
            continue; // the report puts blank lines between the rows
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        match tokens.as_slice() {
            [name, value] if parse_displayed(value).is_some() => entries.push(ExportEntry {
                raw_name: (*name).to_string(),
                canonical: canonical_name(name),
                text: (*value).to_string(),
                exact: false,
            }),
            _ if entries.is_empty() => {}
            _ => break,
        }
    }
    entries
}

/// One row of the profile worksheet (the "summary"): the sample and what was computed on it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRow {
    pub time: String,
    /// The sample was used for the terminal phase.
    pub included: bool,
    pub conc: String,
    pub predicted: Option<String>,
    pub residual: Option<String>,
    pub auc: Option<String>,
    pub aumc: Option<String>,
    /// The weight of the sample in the terminal regression.
    pub weight: Option<String>,
    pub exact: bool,
}

/// Reads the profile worksheet: a header with time, concentration, predicted, residual, AUC, AUMC,
/// weighting and a column marking the samples used for the terminal phase, then an optional units
/// row, then one row per sample. `None` when the table does not look like one.
pub fn extract_profile_table(table: &RawTable) -> Option<Vec<ProfileRow>> {
    let header = table.rows.first()?;
    let col = |words: &[&str]| header_has(header, words);
    let time = col(&["time", "temps", "t"])?;
    let conc = col(&["conc", "concentration"])?;
    let auc = col(&["auc"])?;
    let included = col(&["lambdazincl", "incl", "included"]);
    let predicted = col(&["pred"]);
    let residual = col(&["resid"]);
    let aumc = col(&["aumc"]);
    let weight = col(&["weight"]);
    let get = |row: &Vec<String>, c: Option<usize>| {
        c.and_then(|c| row.get(c))
            .filter(|t| !t.is_empty())
            .cloned()
    };
    let rows: Vec<ProfileRow> = table
        .rows
        .iter()
        .skip(1)
        .filter(|row| row.get(time).is_some_and(|t| is_number(t)))
        .map(|row| ProfileRow {
            time: row[time].clone(),
            included: get(row, included).is_some_and(|t| !t.is_empty() && t != "0"),
            conc: get(row, Some(conc)).unwrap_or_default(),
            predicted: get(row, predicted),
            residual: get(row, residual),
            auc: get(row, Some(auc)),
            aumc: get(row, aumc),
            weight: get(row, weight),
            exact: table.exact,
        })
        .collect();
    (!rows.is_empty()).then_some(rows)
}

/// What the engine says about one sample of the profile (the cumulative areas up to that sample,
/// the prediction of the terminal regression, and whether the sample took part in it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileEngine {
    pub auc: Option<f64>,
    pub aumc: Option<f64>,
    pub predicted: Option<f64>,
    pub included: bool,
}

/// Compares the profile worksheet with the engine, row by row: cumulative AUC and AUMC, prediction,
/// residual (observed minus predicted, the observed concentration being the one of the row), the
/// inclusion mark, and the weight (1 for an included sample, 0 otherwise). Only counts.
pub fn evaluate_profile(rows: &[ProfileRow], engine: &[ProfileEngine]) -> PrivateCounts {
    let mut counts = PrivateCounts::default();
    if rows.len() != engine.len() {
        counts.unreadable += rows.len().max(engine.len());
        return counts;
    }
    for (row, e) in rows.iter().zip(engine) {
        let observed = parse_displayed(&row.conc).map(|d| d.value);
        let mut value = |name: &'static str, text: &Option<String>, actual: Option<f64>| {
            if let Some(text) = text {
                counts_one(&mut counts, Some(name), text, row.exact, |_| actual);
            }
        };
        value("auc.cumulative", &row.auc, e.auc);
        value("aumc.cumulative", &row.aumc, e.aumc);
        value("predicted", &row.predicted, e.predicted);
        let residual = match (observed, e.predicted) {
            (Some(o), Some(p)) => Some(o - p),
            _ => None,
        };
        value("residual", &row.residual, residual);
        value(
            "weight",
            &row.weight,
            Some(if e.included { 1.0 } else { 0.0 }),
        );
        // The inclusion mark is a flag: agree or not.
        counts_flag(&mut counts, row.included == e.included);
    }
    counts
}

fn counts_flag(counts: &mut PrivateCounts, agree: bool) {
    if agree {
        counts.validated += 1;
    } else {
        counts.mismatched.push(Difference {
            name: "lambda.z.included",
            decimals: 0,
            size: 1.0,
            relative: false,
        });
    }
}

/// Conventions recorded in a settings file, as lowercase text with all but letters and digits
/// removed (the wording of the reference software is not reproduced; the tests look for words).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettingsFacts {
    /// The AUC calculation method.
    pub calculation_method: String,
    /// The weighting of the terminal regression.
    pub weighting: String,
    /// How the terminal phase was chosen (fit method and selection).
    pub slope_selection: String,
}

/// Reads the settings recorded in an exported settings report.
pub fn settings_facts(text: &str) -> SettingsFacts {
    let compact = |t: &str| -> String {
        t.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_lowercase()
    };
    let mut facts = SettingsFacts::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Calculation Method") {
            facts.calculation_method = compact(v);
        } else if let Some(v) = line.strip_prefix("Weighting") {
            facts.weighting = compact(v);
        } else if line.contains("Fit Method") {
            facts.slope_selection = compact(line);
        }
    }
    facts
}

/// Reads and parses one export file into scalar entries: a "Final Parameters" worksheet, or the
/// final-parameters block of a core-output report.
pub fn read_export(file: &ExportFile) -> Result<Vec<ExportEntry>, PrivateError> {
    let what = format!("export file {} ({})", file.index + 1, file.kind.label());
    match file.kind {
        ExportKind::CoreOutput => {
            let entries = report_final_parameters(&read_text(file, &what)?);
            if entries.is_empty() {
                Err(format_error(
                    &what,
                    "no final-parameters block found in the report",
                ))
            } else {
                Ok(entries)
            }
        }
        ExportKind::FinalParameters => {
            let table = read_table(file, &what)?;
            extract_entries(&table, &what)
        }
        ExportKind::Summary | ExportKind::Settings => Err(format_error(
            &what,
            "this kind of file has no scalar parameters",
        )),
    }
}

/// Reads a profile worksheet (kind [`ExportKind::Summary`]).
pub fn read_profile_rows(file: &ExportFile) -> Result<Vec<ProfileRow>, PrivateError> {
    let what = format!("export file {} ({})", file.index + 1, file.kind.label());
    let table = read_table(file, &what)?;
    extract_profile_table(&table).ok_or_else(|| format_error(&what, "not a profile worksheet"))
}

/// Reads a settings file (kind [`ExportKind::Settings`]).
pub fn read_settings(file: &ExportFile) -> Result<SettingsFacts, PrivateError> {
    let what = format!("export file {} ({})", file.index + 1, file.kind.label());
    Ok(settings_facts(&read_text(file, &what)?))
}

fn is_spreadsheet(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "xls" | "xlsx"))
}

/// The text of an export file: RTF is converted, bytes are read as UTF-8 or Latin-1.
fn read_text(file: &ExportFile, what: &str) -> Result<String, PrivateError> {
    let bytes = fs::read(&file.path).map_err(|e| io_error(what, &e))?;
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    };
    Ok(if text.trim_start().starts_with("{\\rtf") {
        rtf_to_text(&text)
    } else {
        text
    })
}

fn read_table(file: &ExportFile, what: &str) -> Result<RawTable, PrivateError> {
    if is_spreadsheet(&file.path) {
        read_worksheet(&file.path, what)
    } else {
        split_table(&read_text(file, what)?, what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayed_numbers_know_their_precision() {
        let d = parse_displayed("12.350").unwrap();
        assert_eq!((d.value, d.decimals), (12.35, 3));
        assert!((d.half_unit - 0.0005).abs() < 1e-15);
        assert_eq!(parse_displayed("12,35").unwrap().decimals, 2);
        assert_eq!(parse_displayed("-7").unwrap().decimals, 0);
        let e = parse_displayed("1.5E-3").unwrap();
        assert!((e.value - 0.0015).abs() < 1e-18 && (e.half_unit - 0.5e-4).abs() < 1e-18);
        assert_eq!(parse_displayed("2,5e2").unwrap().value, 250.0);
        for bad in [
            "", "NC", "abc", "1.2.3", "1,234.5", "1,2,3", "12 mg", "--1", ".",
        ] {
            assert_eq!(parse_displayed(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn equality_at_the_displayed_precision() {
        assert_eq!(compare_displayed("12.35", 12.3549), Outcome::Match);
        assert_eq!(compare_displayed("12.35", 12.3451), Outcome::Match);
        assert_eq!(compare_displayed("12.35", 12.3551), Outcome::Mismatch);
        assert_eq!(compare_displayed("12.35", 12.345), Outcome::Match); // exactly half a unit
        assert_eq!(compare_displayed("12,35", 12.3549), Outcome::Match);
        assert_eq!(compare_displayed("7", 7.4), Outcome::Match);
        assert_eq!(compare_displayed("7", 7.6), Outcome::Mismatch);
        assert_eq!(compare_displayed("1.5E-3", 0.00152), Outcome::Match);
        assert_eq!(compare_displayed("1.5E-3", 0.0016), Outcome::Mismatch);
        assert_eq!(compare_displayed("NC", 1.0), Outcome::Unreadable);
        assert_eq!(compare_displayed("1.0", f64::NAN), Outcome::Mismatch);
    }

    #[test]
    fn delimiters_quotes_and_marks_are_tolerated() {
        let semicolon = "\u{feff}Parameter;Value;Units\r\nCmax;12,5;ng/mL\r\n\r\nTmax;2;h\r\n";
        let t = split_table(semicolon, "t").unwrap();
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[1], vec!["Cmax", "12,5", "ng/mL"]);
        let tab = "Parameter\tValue\nCmax\t12.5\n";
        assert_eq!(split_table(tab, "t").unwrap().rows[1], vec!["Cmax", "12.5"]);
        let quoted = "\"Parameter\",\"Value\"\n\"AUC_%Extrap_obs\",\"1,5\"\n\"a \"\"b\"\"\",3\n";
        let t = split_table(quoted, "t").unwrap();
        assert_eq!(t.rows[1], vec!["AUC_%Extrap_obs", "1,5"]);
        assert_eq!(t.rows[2][0], "a \"b\"");
        assert!(split_table("\n \n", "t").is_err());
    }

    fn entries_of(text: &str) -> Vec<(String, Option<&'static str>, String)> {
        let table = split_table(text, "t").unwrap();
        extract_entries(&table, "t")
            .unwrap()
            .into_iter()
            .map(|e| (e.raw_name, e.canonical, e.text))
            .collect()
    }

    #[test]
    fn long_and_wide_layouts_give_the_same_entries() {
        // Made-up numbers: these tests never see an export.
        let long =
            "Final Parameters\nParameter,Value,Units\nCmax,5.5,u\nLambda_z,0.25,1/h\nWeird,1,x\n";
        let e = entries_of(long);
        assert_eq!(e.len(), 3);
        assert_eq!((e[0].1, e[0].2.as_str()), (Some("cmax"), "5.5"));
        assert_eq!(e[1].1, Some("lambda.z"));
        assert_eq!(e[2].1, None);
        let wide = "Subject;Cmax;Lambda_z;Weird\nUnits;u;1/h;x\n1;5,5;0,25;1\n";
        let w = entries_of(wide);
        let names: Vec<_> = w.iter().map(|x| (x.1, x.2.clone())).collect();
        assert_eq!(
            names,
            vec![
                (Some("cmax"), "5,5".to_string()),
                (Some("lambda.z"), "0,25".to_string()),
                (None, "1".to_string())
            ]
        );
    }

    #[test]
    fn a_summary_table_of_one_profile_uses_the_mean_row() {
        let summary = "Statistic,Cmax,AUClast\nN,1,1\nMean,5.5,20.25\nSD,,\nMedian,5.5,20.25\nMin,5.5,20.25\n";
        let e = entries_of(summary);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].2, "5.5");
        assert_eq!(e[1].2, "20.25");
        // Without statistic labels: the first row of numbers.
        let plain = "ID,Cmax\nA,3\nB,4\n";
        assert_eq!(entries_of(plain)[0].2, "3");
    }

    #[test]
    fn errors_say_what_is_wrong_without_quoting() {
        let table = split_table("only a title\nanother\n", "export file 1").unwrap();
        let err = extract_entries(&table, "export file 1")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("export file 1") && !err.contains("title"),
            "{err}"
        );
        let header_only = split_table("a,b\n", "t").unwrap();
        assert!(extract_entries(&header_only, "t").is_err());
    }

    #[test]
    fn evaluation_counts_and_never_carries_values() {
        let entries = |pairs: &[(&str, &str)]| -> Vec<ExportEntry> {
            pairs
                .iter()
                .map(|(n, t)| ExportEntry {
                    raw_name: n.to_string(),
                    canonical: canonical_name(n),
                    text: t.to_string(),
                    exact: false,
                })
                .collect()
        };
        let engine: BTreeMap<String, Option<f64>> = [
            ("cmax".to_string(), Some(5.4999)),
            ("lambda.z".to_string(), Some(0.3)),
            ("tlag".to_string(), None),
        ]
        .into();
        let c = evaluate(
            &entries(&[
                ("Cmax", "5.50"),
                ("Lambda_z", "0.25"),
                ("Tlag", "0.5"),
                ("Tmax", "2"),
                ("Mystery", "1"),
                ("Tlast", "NC"),
            ]),
            &engine,
        );
        assert_eq!(c.validated, 1);
        assert_eq!(
            c.mismatched
                .iter()
                .map(|d| (d.name, d.decimals))
                .collect::<Vec<_>>(),
            vec![("lambda.z", 2)]
        );
        assert_eq!(c.engine_missing, vec!["tlag", "tmax"]);
        assert_eq!((c.unmapped, c.unreadable), (1, 1));
        assert_eq!(c.expected(), 4);
    }

    #[test]
    fn export_files_are_classified_by_path_or_manifest() {
        assert_eq!(
            classify("Final Parameters linear.csv"),
            (Some(ExportKind::FinalParameters), Some(MethodHint::Linear))
        );
        assert_eq!(
            classify("lin up log down/Summary.csv"),
            (Some(ExportKind::Summary), Some(MethodHint::LinUpLogDown))
        );
        assert_eq!(classify("results.csv"), (None, None));
        // The linear-log trapezoid is another method and is not guessed.
        assert_eq!(classify("final_linlog.csv").1, None);
    }

    #[test]
    fn the_name_table_maps_the_usual_parameters() {
        for (raw, canonical) in [
            ("Cmax", "cmax"),
            ("AUClast", "auclast"),
            ("AUC_%Extrap_obs", "aucpext.obs"),
            ("Cl_F_obs", "cl.obs"),
            ("Rsq_adjusted", "adj.r.squared"),
            ("No_points_lambda_z", "lambda.z.n.points"),
            ("HL_Lambda_z", "half.life"),
        ] {
            assert_eq!(canonical_name(raw), Some(canonical), "{raw}");
        }
        assert_eq!(canonical_name("Nonsense"), None);
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("caladrius-private-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The whole path on made-up files in a scratch folder: discovery by name, a manifest for what
    /// the names do not say, reading in two layouts and decimal separators, and the counts.
    #[test]
    fn discovery_reading_and_comparison_on_made_up_exports() {
        let dir = scratch("flow");
        fs::create_dir_all(dir.join("lin")).unwrap();
        fs::write(
            dir.join("lin").join("Final Parameters.csv"),
            "Parameter,Value,Units
Cmax,5.50,u
Lambda_z,0.25,1/h
",
        )
        .unwrap();
        fs::write(
            dir.join("Summary up down.csv"),
            "Statistic;Cmax;Lambda_z
Mean;5,5;0,4
",
        )
        .unwrap();
        fs::write(
            dir.join("a.csv"),
            "Parameter,Value
Cmax,1
",
        )
        .unwrap();
        fs::write(
            dir.join("b.csv"),
            "Parameter,Value
Cmax,1
",
        )
        .unwrap();
        fs::write(
            dir.join("manifest.json"),
            r#"[{"file":"b.csv","method":"lin_up_log_down","table":"final"}]"#,
        )
        .unwrap();
        let d = discover_in(&dir).unwrap().unwrap();
        // Two by name, one by manifest, one left unclassified.
        assert_eq!(d.files.len(), 3);
        assert_eq!(d.unclassified, 1);
        let find = |kind, method| {
            d.files
                .iter()
                .find(|f| f.kind == kind && f.method == method)
        };
        assert!(find(ExportKind::FinalParameters, MethodHint::Linear).is_some());
        assert!(find(ExportKind::Summary, MethodHint::LinUpLogDown).is_some());
        assert!(find(ExportKind::FinalParameters, MethodHint::LinUpLogDown).is_some());
        let engine: BTreeMap<String, Option<f64>> = [
            ("cmax".to_string(), Some(5.5)),
            ("lambda.z".to_string(), Some(0.25)),
        ]
        .into();
        let summary = d
            .files
            .iter()
            .find(|f| f.kind == ExportKind::Summary)
            .unwrap();
        let table = split_table(&fs::read_to_string(&summary.path).unwrap(), "t").unwrap();
        let counts = evaluate(&extract_entries(&table, "t").unwrap(), &engine);
        assert_eq!(counts.validated, 1);
        assert_eq!(
            counts
                .mismatched
                .iter()
                .map(|d| (d.name, d.decimals))
                .collect::<Vec<_>>(),
            vec![("lambda.z", 1)]
        );
        let final_linear = d
            .files
            .iter()
            .find(|f| f.method == MethodHint::Linear)
            .unwrap();
        let counts = evaluate(&read_export(final_linear).unwrap(), &engine);
        assert_eq!((counts.validated, counts.mismatched.len()), (2, 0));
        let _ = fs::remove_dir_all(&dir);
        // An empty folder, and a folder that does not exist, are "no exports".
        let empty = scratch("empty");
        assert_eq!(discover_in(&empty).unwrap(), None);
        assert_eq!(discover_in(&empty.join("missing")).unwrap(), None);
        let _ = fs::remove_dir_all(&empty);
    }

    #[test]
    fn private_locations_are_inside_the_repository() {
        assert!(private_dir().ends_with("private"));
        assert!(exports_dir("td1").ends_with("td1"));
        // No exports folder content is required for the module to work.
        let _ = discover_exports("no_such_exercise").unwrap();
    }
}
