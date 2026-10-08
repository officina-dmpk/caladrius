//! Loaders for the versioned public oracle in `oracle/` (datasets, expected values, options).
//!
//! Layout (see `oracle/README.md`):
//!
//! - `oracle/data/<dataset>.csv`: `subject,time,conc,dose`
//! - `oracle/expected/<case>.csv`: `subject,parameter,value` (empty value means NA)
//! - `oracle/expected/<case>.options.json`: options, versions, preprocessing

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::compare::Table;

/// Error while locating, reading or interpreting an oracle file. Always carries the file.
#[derive(Debug, Clone, PartialEq)]
pub enum OracleError {
    /// The file could not be read.
    Io { path: String, message: String },
    /// A line of a CSV file is malformed (`line` counts from 1, the header being line 1).
    Csv {
        path: String,
        line: usize,
        message: String,
    },
    /// The JSON options file is malformed.
    Json { path: String, message: String },
    /// Files that must agree do not (for example a profile with unsorted times).
    Inconsistent { path: String, message: String },
}

impl fmt::Display for OracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OracleError::Io { path, message } => write!(f, "cannot read {path}: {message}"),
            OracleError::Csv {
                path,
                line,
                message,
            } => write!(f, "{path}, line {line}: {message}"),
            OracleError::Json { path, message } => write!(f, "{path}: invalid JSON: {message}"),
            OracleError::Inconsistent { path, message } => write!(f, "{path}: {message}"),
        }
    }
}

impl std::error::Error for OracleError {}

/// The `oracle/` directory of this repository.
pub fn oracle_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `<repo>/crates/caladrius-testkit` at compile time.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map_or_else(|| PathBuf::from("oracle"), |root| root.join("oracle"))
}

/// One subject's concentration-time profile and dose.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub subject: u32,
    /// Sampling times, in the order of the file (non-decreasing).
    pub time: Vec<f64>,
    pub conc: Vec<f64>,
    /// Absolute dose given at time 0.
    pub dose: f64,
}

/// A dataset: profiles in subject order.
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    pub name: String,
    pub profiles: Vec<Profile>,
}

/// PKNCA options used to produce a case, as written explicitly in `oracle/scripts/nca_pknca.R`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PknaOptions {
    /// `"lin up/log down"` or `"linear"`.
    #[serde(rename = "auc.method")]
    pub auc_method: String,
    #[serde(rename = "conc.na")]
    pub conc_na: String,
    #[serde(rename = "conc.blq")]
    pub conc_blq: BlqHandling,
    #[serde(rename = "first.tmax")]
    pub first_tmax: bool,
    #[serde(rename = "allow.tmax.in.half.life")]
    pub allow_tmax_in_half_life: bool,
    #[serde(rename = "min.hl.points")]
    pub min_hl_points: u32,
    #[serde(rename = "adj.r.squared.factor")]
    pub adj_r_squared_factor: f64,
    #[serde(rename = "min.hl.r.squared")]
    pub min_hl_r_squared: f64,
    #[serde(rename = "min.span.ratio")]
    pub min_span_ratio: f64,
    #[serde(rename = "max.aucinf.pext")]
    pub max_aucinf_pext: f64,
    #[serde(rename = "max.missing")]
    pub max_missing: f64,
}

/// Handling of below-limit concentrations (zeros) at the first, middle and last positions.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BlqHandling {
    pub first: String,
    pub middle: String,
    pub last: String,
}

/// Units of a case.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Units {
    pub dose: String,
    pub time: String,
    pub concentration: String,
}

/// Versions of the tools that produced a case.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Versions {
    #[serde(rename = "R")]
    pub r: String,
    #[serde(rename = "PKNCA")]
    pub pknca: String,
}

/// Content of `<case>.options.json` (fields the tests rely on; others are ignored).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CaseOptions {
    pub schema: u32,
    pub case: String,
    pub dataset: String,
    /// Path of the dataset CSV relative to `oracle/`.
    pub data_file: String,
    /// `"extravascular"` or `"iv_bolus"`.
    pub route: String,
    pub units: Units,
    pub versions: Versions,
    pub pknca_options: PknaOptions,
    /// Parameter names (PKNCA spelling) in the expected table, per subject.
    pub parameters: Vec<String>,
    pub n_subjects: usize,
    pub n_values: usize,
    pub n_na: usize,
}

/// A complete oracle case: input profiles, options, expected values.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleCase {
    pub name: String,
    pub options: CaseOptions,
    pub dataset: Dataset,
    /// Expected values keyed by `(subject, parameter)`; `None` is NA.
    pub expected: Table,
}

fn read(path: &Path) -> Result<String, OracleError> {
    fs::read_to_string(path).map_err(|e| OracleError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

fn csv_err(path: &str, line: usize, message: impl Into<String>) -> OracleError {
    OracleError::Csv {
        path: path.to_string(),
        line,
        message: message.into(),
    }
}

fn number(path: &str, line: usize, field: &str, what: &str) -> Result<f64, OracleError> {
    let value: f64 = field
        .trim()
        .parse()
        .map_err(|_| csv_err(path, line, format!("{what} is not a number: {field:?}")))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(csv_err(path, line, format!("{what} is not finite")))
    }
}

fn data_lines<'a>(
    path: &str,
    text: &'a str,
    header: &str,
) -> Result<impl Iterator<Item = (usize, &'a str)>, OracleError> {
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, first)) if first.trim() == header => {}
        _ => return Err(csv_err(path, 1, format!("expected header {header:?}"))),
    }
    Ok(lines
        .map(|(i, l)| (i + 1, l))
        .filter(|(_, l)| !l.trim().is_empty()))
}

/// Parses the text of a dataset CSV (`subject,time,conc,dose`). `path` is only used in messages.
pub fn parse_dataset(name: &str, path: &str, text: &str) -> Result<Dataset, OracleError> {
    let mut profiles: Vec<Profile> = Vec::new();
    for (line, row) in data_lines(path, text, "subject,time,conc,dose")? {
        let mut fields = row.split(',');
        let (Some(s), Some(t), Some(c), Some(d), None) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return Err(csv_err(
                path,
                line,
                "expected 4 fields: subject,time,conc,dose",
            ));
        };
        let subject: u32 = s
            .trim()
            .parse()
            .map_err(|_| csv_err(path, line, format!("subject is not an integer: {s:?}")))?;
        let time = number(path, line, t, "time")?;
        let conc = number(path, line, c, "conc")?;
        let dose = number(path, line, d, "dose")?;
        match profiles.last_mut() {
            Some(p) if p.subject == subject => {
                if p.time.last().is_some_and(|&last| time < last) {
                    return Err(csv_err(
                        path,
                        line,
                        "times are not sorted within the subject",
                    ));
                }
                if p.dose != dose {
                    return Err(csv_err(path, line, "dose differs within the subject"));
                }
                p.time.push(time);
                p.conc.push(conc);
            }
            _ => {
                if profiles.iter().any(|p| p.subject == subject) {
                    return Err(csv_err(path, line, "rows of a subject are not contiguous"));
                }
                profiles.push(Profile {
                    subject,
                    time: vec![time],
                    conc: vec![conc],
                    dose,
                });
            }
        }
    }
    if profiles.is_empty() {
        return Err(OracleError::Inconsistent {
            path: path.to_string(),
            message: "no data rows".to_string(),
        });
    }
    Ok(Dataset {
        name: name.to_string(),
        profiles,
    })
}

/// Parses the text of an expected-values CSV (`subject,parameter,value`; empty value is NA).
pub fn parse_expected(path: &str, text: &str) -> Result<Table, OracleError> {
    let mut table = Table::new();
    for (line, row) in data_lines(path, text, "subject,parameter,value")? {
        let mut fields = row.split(',');
        let (Some(s), Some(p), Some(v), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(csv_err(
                path,
                line,
                "expected 3 fields: subject,parameter,value",
            ));
        };
        if s.trim().is_empty() || p.trim().is_empty() {
            return Err(csv_err(path, line, "empty subject or parameter"));
        }
        let value = if v.trim().is_empty() {
            None
        } else {
            Some(number(path, line, v, "value")?)
        };
        if table.get(s.trim(), p.trim()).is_some() {
            return Err(csv_err(
                path,
                line,
                format!("duplicate entry for subject {s} parameter {p}"),
            ));
        }
        table.insert(s.trim(), p.trim(), value);
    }
    Ok(table)
}

/// Loads a dataset CSV from a file.
pub fn load_dataset(name: &str, path: &Path) -> Result<Dataset, OracleError> {
    let text = read(path)?;
    parse_dataset(name, &path.display().to_string(), &text)
}

/// Names of the cases found in `oracle/expected/` (files `<case>.options.json`), sorted.
pub fn list_cases() -> Result<Vec<String>, OracleError> {
    let dir = oracle_dir().join("expected");
    let entries = fs::read_dir(&dir).map_err(|e| OracleError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter_map(|n| n.strip_suffix(".options.json").map(str::to_owned))
        .collect();
    names.sort();
    Ok(names)
}

/// Loads one case: options, the dataset it names, and the expected values. Checks that the three
/// agree (subject counts, number of values, parameter list).
pub fn load_case(name: &str) -> Result<OracleCase, OracleError> {
    let root = oracle_dir();
    let options_path = root.join("expected").join(format!("{name}.options.json"));
    let options_text = read(&options_path)?;
    let options: CaseOptions =
        serde_json::from_str(&options_text).map_err(|e| OracleError::Json {
            path: options_path.display().to_string(),
            message: e.to_string(),
        })?;
    let here = options_path.display().to_string();
    if options.case != name {
        return Err(OracleError::Inconsistent {
            path: here,
            message: format!("case field is {:?}, file name says {name:?}", options.case),
        });
    }
    let dataset = load_dataset(&options.dataset, &root.join(&options.data_file))?;

    let expected_path = root.join("expected").join(format!("{name}.csv"));
    let expected_text = read(&expected_path)?;
    let expected = parse_expected(&expected_path.display().to_string(), &expected_text)?;

    let inconsistent = |message: String| OracleError::Inconsistent {
        path: here.clone(),
        message,
    };
    if dataset.profiles.len() != options.n_subjects {
        return Err(inconsistent(format!(
            "options say {} subjects, the dataset has {}",
            options.n_subjects,
            dataset.profiles.len()
        )));
    }
    if expected.len() != options.n_values
        || options.n_values != options.n_subjects * options.parameters.len()
    {
        return Err(inconsistent(format!(
            "options say {} values ({} subjects x {} parameters), the expected table has {}",
            options.n_values,
            options.n_subjects,
            options.parameters.len(),
            expected.len()
        )));
    }
    for profile in &dataset.profiles {
        let subject = profile.subject.to_string();
        for parameter in &options.parameters {
            if expected.get(&subject, parameter).is_none() {
                return Err(inconsistent(format!(
                    "expected table lacks subject {subject}, parameter {parameter}"
                )));
            }
        }
    }
    let na = expected.iter().filter(|(_, _, v)| v.is_none()).count();
    if na != options.n_na {
        return Err(inconsistent(format!(
            "options say {} NA values, the expected table has {na}",
            options.n_na
        )));
    }
    Ok(OracleCase {
        name: name.to_string(),
        options,
        dataset,
        expected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = "subject,time,conc,dose\n1,0,0.5,10\n1,1,2.5,10\n2,0,0,12\n2,2,1.25,12\n";

    #[test]
    fn parses_a_dataset() {
        let d = parse_dataset("t", "mem", DATA).unwrap();
        assert_eq!(d.profiles.len(), 2);
        let p = d.profiles.first().unwrap();
        assert_eq!((p.subject, p.dose), (1, 10.0));
        assert_eq!(p.time, vec![0.0, 1.0]);
        assert_eq!(p.conc, vec![0.5, 2.5]);
    }

    #[test]
    fn rejects_a_wrong_header_with_the_line() {
        let err = parse_dataset("t", "mem", "a,b,c\n1,2,3\n").unwrap_err();
        assert!(matches!(err, OracleError::Csv { line: 1, .. }), "{err}");
    }

    #[test]
    fn rejects_malformed_rows() {
        for (bad, line) in [
            ("subject,time,conc,dose\n1,0,0.5\n", 2),
            ("subject,time,conc,dose\n1,0,abc,10\n", 2),
            ("subject,time,conc,dose\nx,0,1,10\n", 2),
            ("subject,time,conc,dose\n1,0,1,10\n1,0.5,1,10,9\n", 3),
            ("subject,time,conc,dose\n1,2,1,10\n1,1,1,10\n", 3),
            ("subject,time,conc,dose\n1,0,1,10\n1,1,1,11\n", 3),
            ("subject,time,conc,dose\n1,0,1,10\n2,0,1,10\n1,1,1,10\n", 4),
            ("subject,time,conc,dose\n1,0,NaN,10\n", 2),
        ] {
            let err = parse_dataset("t", "mem", bad).unwrap_err();
            assert!(
                matches!(err, OracleError::Csv { line: l, .. } if l == line),
                "{bad:?}: {err}"
            );
        }
    }

    #[test]
    fn rejects_an_empty_dataset() {
        assert!(matches!(
            parse_dataset("t", "mem", "subject,time,conc,dose\n"),
            Err(OracleError::Inconsistent { .. })
        ));
    }

    #[test]
    fn parses_expected_values_with_na() {
        let t =
            parse_expected("mem", "subject,parameter,value\n1,a,1.5\n1,b,\n2,a,-3e-2\n").unwrap();
        assert_eq!(t.get("1", "a"), Some(Some(1.5)));
        assert_eq!(t.get("1", "b"), Some(None));
        assert_eq!(t.get("2", "a"), Some(Some(-0.03)));
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn rejects_duplicate_and_malformed_expected_rows() {
        assert!(parse_expected("mem", "subject,parameter,value\n1,a,1\n1,a,2\n").is_err());
        assert!(parse_expected("mem", "subject,parameter,value\n1,a\n").is_err());
        assert!(parse_expected("mem", "subject,parameter,value\n,a,1\n").is_err());
        assert!(parse_expected("mem", "subject,parameter,value\n1,a,x\n").is_err());
        assert!(parse_expected("mem", "x\n").is_err());
    }

    #[test]
    fn a_missing_file_is_a_readable_error() {
        let err = load_dataset("t", Path::new("definitely/not/here.csv")).unwrap_err();
        assert!(err.to_string().contains("definitely"), "{err}");
        assert!(load_case("no_such_case").is_err());
    }

    #[test]
    fn lists_the_public_cases() {
        let cases = list_cases().unwrap();
        for expected in ["indometh", "indometh_linear", "theoph", "theoph_linear"] {
            assert!(
                cases.iter().any(|c| c == expected),
                "{expected} missing from {cases:?}"
            );
        }
    }

    #[test]
    fn loads_every_public_case_consistently() {
        for name in list_cases().unwrap() {
            let case = load_case(&name).unwrap();
            assert_eq!(case.name, name);
            assert_eq!(case.expected.len(), case.options.n_values);
            assert_eq!(case.options.schema, 1);
            assert_eq!(case.options.units.time, "h");
            assert!(!case.options.versions.pknca.is_empty());
            for p in &case.dataset.profiles {
                assert_eq!(p.time.len(), p.conc.len());
                assert!(p.dose > 0.0);
            }
        }
    }

    #[test]
    fn the_public_cases_have_the_documented_shape() {
        let theoph = load_case("theoph").unwrap();
        assert_eq!(theoph.dataset.profiles.len(), 12);
        assert!(
            theoph
                .dataset
                .profiles
                .iter()
                .all(|p| p.time.len() == 11 && p.time.first() == Some(&0.0))
        );
        assert_eq!(theoph.options.route, "extravascular");
        assert_eq!(theoph.options.pknca_options.auc_method, "lin up/log down");
        assert_eq!(theoph.options.pknca_options.min_hl_points, 3);
        assert!(!theoph.options.pknca_options.allow_tmax_in_half_life);
        assert_eq!(theoph.options.pknca_options.conc_blq.middle, "drop");
        let indo = load_case("indometh_linear").unwrap();
        assert_eq!(indo.dataset.profiles.len(), 6);
        assert!(
            indo.dataset
                .profiles
                .iter()
                .all(|p| p.time.len() == 11 && p.time.first() == Some(&0.25))
        );
        assert_eq!(indo.options.route, "iv_bolus");
        assert_eq!(indo.options.pknca_options.auc_method, "linear");
    }
}
