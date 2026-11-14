//! The subjects of a population run (POP-DAT-01): from the inline `subjects` of the request or from
//! a table in the event layout (`ID`, `TIME`, `DV`, `AMT`, `EVID`, optionally `MDV`, `ADDL`, `SS`,
//! `DUR`), then the exclusions of `exclude_subjects`, then the refusals of the dataset that belong to
//! a subject (POP-INN-03 items 6 and 7), then the observations that enter the fit: y present and a
//! time at or after the dose (an observation before the dose is left out, `before_dose`).
//!
//! The library reads no file (layer L0): the host reads the table named by `dataset_file` and puts
//! its text in [`PopRequest::dataset_text`].

use super::{PopError, PopErrorCode, PopRequest, invalid};

/// A subject as the fit uses it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Subject {
    pub id: String,
    pub dose: f64,
    /// The duration of a zero-order input, when the subject carries one.
    pub duration: Option<f64>,
    /// Times since the dose and observations, for the observations used only.
    pub times: Vec<f64>,
    pub y: Vec<f64>,
}

/// A subject as given, before the checks.
#[derive(Debug, Clone, PartialEq)]
struct Raw {
    id: String,
    /// (time, amount, duration) of each dose row.
    doses: Vec<(f64, f64, Option<f64>)>,
    /// (time, observation) of each observation row; the time is absolute (the dose row's time is
    /// subtracted later).
    observations: Vec<(f64, Option<f64>)>,
    /// (row of the subject from 1, column) of each row that asks for additional doses or a steady
    /// state.
    history: Vec<(usize, String)>,
}

/// The subjects used by the run, in dataset order, after the exclusions and the dataset refusals.
pub(crate) fn subjects(request: &PopRequest) -> Result<Vec<Subject>, PopError> {
    let raw = match (&request.subjects, &request.dataset_file) {
        (Some(_), Some(_)) => {
            return Err(invalid(
                "dataset_file",
                "the request gives both `subjects` and `dataset_file`; give one of them",
            ));
        }
        (Some(list), None) => inline(list)?,
        (None, Some(path)) => match &request.dataset_text {
            Some(text) => table(text)?,
            None => {
                return Err(invalid(
                    "dataset_file",
                    format!(
                        "the table `{path}` was not read by the host; read it into `dataset_text`, or give `subjects`"
                    ),
                ));
            }
        },
        (None, None) => {
            return Err(invalid(
                "subjects",
                "no subject is given; give `subjects`, or a table with `dataset_file`",
            ));
        }
    };
    let raw = exclude(raw, &request.options.exclude_subjects)?;
    refuse_history(&raw)?;
    refuse_dose_rows(&raw)?;
    Ok(raw.into_iter().filter_map(used).collect())
}

fn inline(list: &[super::SubjectInput]) -> Result<Vec<Raw>, PopError> {
    let mut out: Vec<Raw> = Vec::with_capacity(list.len());
    for s in list {
        if out.iter().any(|r| r.id == s.id) {
            return Err(invalid(
                "subjects",
                format!(
                    "the label `{}` is given to two subjects; give each subject its own label",
                    s.id
                ),
            ));
        }
        if s.time.len() != s.y.len() {
            return Err(invalid(
                "subjects",
                format!(
                    "subject {} has {} times but {} observations; give one observation (or null) per time",
                    s.id,
                    s.time.len(),
                    s.y.len()
                ),
            ));
        }
        if !(s.dose.is_finite() && s.dose > 0.0) {
            return Err(invalid(
                "subjects",
                format!(
                    "the dose of subject {} is {}; give a finite dose greater than 0",
                    s.id, s.dose
                ),
            ));
        }
        let finite =
            s.time.iter().all(|t| t.is_finite()) && s.y.iter().flatten().all(|y| y.is_finite());
        if !finite {
            return Err(invalid(
                "subjects",
                format!(
                    "subject {} has a time or an observation that is not a finite number; give finite numbers (null for a missing observation)",
                    s.id
                ),
            ));
        }
        out.push(Raw {
            id: s.id.clone(),
            doses: vec![(0.0, s.dose, s.duration)],
            observations: s.time.iter().copied().zip(s.y.iter().copied()).collect(),
            history: Vec::new(),
        });
    }
    Ok(out)
}

/// One cell of the table: `None` when empty or `.`.
fn cell<'a>(fields: &[&'a str], column: Option<usize>) -> Option<&'a str> {
    let text = fields.get(column?)?.trim();
    (!text.is_empty() && text != ".").then_some(text)
}

fn number(text: &str, line: usize, name: &str) -> Result<f64, PopError> {
    match text.parse::<f64>() {
        Ok(x) if x.is_finite() => Ok(x),
        _ => Err(invalid(
            "dataset_file",
            format!(
                "line {line} of the table: `{name}` is `{text}`, not a finite number; give a number"
            ),
        )),
    }
}

/// Reads a table in the event layout.
fn table(text: &str) -> Result<Vec<Raw>, PopError> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let Some((_, header)) = lines.next() else {
        return Err(invalid(
            "dataset_file",
            "the table is empty; give a header line and one row per event",
        ));
    };
    let names: Vec<String> = header
        .split(',')
        .map(|h| h.trim().trim_matches('"').to_ascii_uppercase())
        .collect();
    let column = |name: &str| names.iter().position(|n| n == name);
    let (Some(id_col), Some(time_col)) = (column("ID"), column("TIME")) else {
        return Err(invalid(
            "dataset_file",
            "the table has no `ID` or no `TIME` column; give the event layout (ID, TIME, DV, AMT, EVID)",
        ));
    };
    let (dv_col, amt_col, evid_col, mdv_col, dur_col) = (
        column("DV"),
        column("AMT"),
        column("EVID"),
        column("MDV"),
        column("DUR"),
    );
    let history_cols: Vec<(usize, String)> = ["ADDL", "SS"]
        .iter()
        .filter_map(|n| column(n).map(|c| (c, n.to_ascii_lowercase())))
        .collect();
    let mut out: Vec<Raw> = Vec::new();
    for (index, line) in lines {
        let line_no = index + 1;
        let fields: Vec<&str> = line
            .split(',')
            .map(|f| f.trim().trim_matches('"'))
            .collect();
        let Some(id) = cell(&fields, Some(id_col)) else {
            return Err(invalid(
                "dataset_file",
                format!("line {line_no} of the table has no `ID`; give the subject of every row"),
            ));
        };
        let time = match cell(&fields, Some(time_col)) {
            Some(t) => number(t, line_no, "TIME")?,
            None => {
                return Err(invalid(
                    "dataset_file",
                    format!(
                        "line {line_no} of the table has no `TIME`; give the time of every row"
                    ),
                ));
            }
        };
        let amount = cell(&fields, amt_col)
            .map(|a| number(a, line_no, "AMT"))
            .transpose()?;
        let evid = cell(&fields, evid_col)
            .map(|e| number(e, line_no, "EVID"))
            .transpose()?;
        let is_dose = match evid {
            Some(1.0) => true,
            Some(0.0) => false,
            Some(e) => {
                return Err(invalid(
                    "dataset_file",
                    format!(
                        "line {line_no} of the table has `EVID` {e}; give 0 (an observation) or 1 (a dose)"
                    ),
                ));
            }
            None => amount.is_some_and(|a| a != 0.0),
        };
        let position = match out.iter().position(|r| r.id == id) {
            Some(p) => p,
            None => {
                out.push(Raw {
                    id: id.to_string(),
                    doses: Vec::new(),
                    observations: Vec::new(),
                    history: Vec::new(),
                });
                out.len() - 1
            }
        };
        let Some(subject) = out.get_mut(position) else {
            continue;
        };
        let row = subject.doses.len() + subject.observations.len() + 1;
        for (c, name) in &history_cols {
            if let Some(text) = cell(&fields, Some(*c)) {
                if number(text, line_no, name)? != 0.0 {
                    subject.history.push((row, name.clone()));
                }
            }
        }
        if is_dose {
            let Some(amount) = amount.filter(|a| *a > 0.0) else {
                return Err(invalid(
                    "dataset_file",
                    format!(
                        "line {line_no} of the table is a dose row with no positive `AMT`; give the amount"
                    ),
                ));
            };
            let duration = cell(&fields, dur_col)
                .map(|d| number(d, line_no, "DUR"))
                .transpose()?;
            subject.doses.push((time, amount, duration));
        } else {
            let missing = cell(&fields, mdv_col)
                .map(|m| number(m, line_no, "MDV"))
                .transpose()?
                .is_some_and(|m| m != 0.0);
            let y = if missing {
                None
            } else {
                cell(&fields, dv_col)
                    .map(|d| number(d, line_no, "DV"))
                    .transpose()?
            };
            subject.observations.push((time, y));
        }
    }
    Ok(out)
}

fn exclude(raw: Vec<Raw>, labels: &[String]) -> Result<Vec<Raw>, PopError> {
    let unknown: Vec<&String> = labels
        .iter()
        .filter(|l| !raw.iter().any(|r| &r.id == *l))
        .collect();
    if !unknown.is_empty() {
        let list = unknown
            .iter()
            .map(|l| format!("`{l}`"))
            .collect::<Vec<_>>()
            .join(" or ");
        return Err(invalid(
            "exclude_subjects",
            format!("no subject is labelled {list} in this dataset; give labels that exist"),
        ));
    }
    Ok(raw
        .into_iter()
        .filter(|r| !labels.contains(&r.id))
        .collect())
}

/// POP-INN-03 item 6: additional doses or a steady state.
fn refuse_history(raw: &[Raw]) -> Result<(), PopError> {
    let sentences: Vec<String> = raw
        .iter()
        .flat_map(|r| {
            r.history.iter().map(move |(row, col)| {
                format!(
                    "row {row} of subject {} asks for additional doses or a steady state (`{col}`), which are not supported in this version; give each dose on its own row.",
                    r.id
                )
            })
        })
        .collect();
    if sentences.is_empty() {
        return Ok(());
    }
    Err(PopError {
        code: PopErrorCode::UnsupportedHistory,
        message: sentences.join(" "),
    })
}

/// POP-INN-03 item 7: one dose row per subject.
fn refuse_dose_rows(raw: &[Raw]) -> Result<(), PopError> {
    let sentences: Vec<String> = raw
        .iter()
        .filter(|r| r.doses.len() != 1)
        .map(|r| {
            format!(
                "subject {} has {} dose rows; the population fit of this version analyses a single dose per subject. Give one dose row per subject, or exclude the subject with `exclude_subjects`.",
                r.id,
                r.doses.len()
            )
        })
        .collect();
    if sentences.is_empty() {
        return Ok(());
    }
    Err(PopError {
        code: PopErrorCode::UnsupportedHistory,
        message: sentences.join(" "),
    })
}

/// The observations that enter the fit (y present, at or after the dose), on the time since the dose.
fn used(r: Raw) -> Option<Subject> {
    let &(dose_time, dose, duration) = r.doses.first()?;
    let (times, y) = r
        .observations
        .iter()
        .filter_map(|&(t, y)| {
            let s = t - dose_time;
            (s >= 0.0).then_some((s, y?))
        })
        .unzip();
    Some(Subject {
        id: r.id,
        dose,
        duration,
        times,
        y,
    })
}
