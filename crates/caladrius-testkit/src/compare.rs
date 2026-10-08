//! Table comparison: every value outside tolerance is reported, never only the first.

use std::collections::BTreeMap;
use std::fmt;

use crate::tolerance::Tolerance;

/// Values indexed by `(group, name)`, for example `("3", "auclast")` for subject 3. A value of
/// `None` means "not available" (PKNCA returns NA, an engine returns `None` or NaN).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    entries: BTreeMap<(String, String), Option<f64>>,
}

impl Table {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a value. NaN is stored as `None`.
    pub fn insert(
        &mut self,
        group: impl Into<String>,
        name: impl Into<String>,
        value: Option<f64>,
    ) {
        let value = value.filter(|v| !v.is_nan());
        self.entries.insert((group.into(), name.into()), value);
    }

    /// The stored value: `None` if the key is absent, `Some(None)` if present but not available.
    pub fn get(&self, group: &str, name: &str) -> Option<Option<f64>> {
        self.entries
            .get(&(group.to_string(), name.to_string()))
            .copied()
    }

    /// Number of entries (available or not).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the table holds no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, Option<f64>)> {
        self.entries
            .iter()
            .map(|((g, n), v)| (g.as_str(), n.as_str(), *v))
    }
}

/// Why a value was reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchKind {
    /// Both values exist but differ by more than the tolerance (this includes an infinite actual value).
    OutOfTolerance,
    /// An expected value exists, the actual table has no such entry.
    MissingEntry,
    /// An expected value exists, the actual entry is not available (`None` or NaN).
    MissingValue,
    /// The expected value is not available (NA), the actual one is a number.
    UnexpectedValue,
}

/// One reported value.
#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    pub group: String,
    pub name: String,
    pub expected: Option<f64>,
    pub actual: Option<f64>,
    /// Relative (or absolute, see [`Tolerance::error`]) error when both values are numbers.
    pub error: Option<f64>,
    pub kind: MismatchKind,
}

/// Result of comparing an actual table with an expected one.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub tolerance: Tolerance,
    /// Number of expected entries examined.
    pub compared: usize,
    /// Every entry outside tolerance, in key order.
    pub mismatches: Vec<Mismatch>,
}

impl Report {
    /// True when no value is outside tolerance.
    pub fn is_ok(&self) -> bool {
        self.mismatches.is_empty()
    }

    /// `Ok` when everything is within tolerance, otherwise the full report as text.
    pub fn into_result(self) -> Result<(), String> {
        if self.is_ok() {
            Ok(())
        } else {
            Err(self.to_string())
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} of {} values outside tolerance ({})",
            self.mismatches.len(),
            self.compared,
            self.tolerance
        )?;
        for m in &self.mismatches {
            let expected = show(m.expected);
            let actual = show(m.actual);
            match m.kind {
                MismatchKind::OutOfTolerance => {
                    let err = m
                        .error
                        .map_or_else(|| "n/a".to_string(), |e| format!("{e:.3e}"));
                    writeln!(
                        f,
                        "  [{}] {}: expected {expected}, got {actual}, error {err}",
                        m.group, m.name
                    )?;
                }
                MismatchKind::MissingEntry => {
                    writeln!(
                        f,
                        "  [{}] {}: expected {expected}, entry missing from the result",
                        m.group, m.name
                    )?;
                }
                MismatchKind::MissingValue => {
                    writeln!(
                        f,
                        "  [{}] {}: expected {expected}, result is not available",
                        m.group, m.name
                    )?;
                }
                MismatchKind::UnexpectedValue => {
                    writeln!(
                        f,
                        "  [{}] {}: expected not available (NA), got {actual}",
                        m.group, m.name
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn show(v: Option<f64>) -> String {
    v.map_or_else(|| "NA".to_string(), |x| format!("{x:e}"))
}

/// Compares `actual` with `expected` under `tolerance`.
///
/// Every entry of `expected` is examined; all those outside tolerance are returned. Entries that
/// exist only in `actual` are not an error (an engine may compute more than the oracle covers).
pub fn compare_tables(expected: &Table, actual: &Table, tolerance: Tolerance) -> Report {
    let mut mismatches = Vec::new();
    let mut compared = 0usize;
    for (group, name, exp) in expected.iter() {
        compared += 1;
        let act_entry = actual.get(group, name);
        let found = |kind: MismatchKind, actual: Option<f64>, error: Option<f64>| Mismatch {
            group: group.to_string(),
            name: name.to_string(),
            expected: exp,
            actual,
            error,
            kind,
        };
        match (exp, act_entry) {
            (Some(_), None) => mismatches.push(found(MismatchKind::MissingEntry, None, None)),
            (Some(_), Some(None)) => mismatches.push(found(MismatchKind::MissingValue, None, None)),
            (Some(e), Some(Some(a))) => {
                if !tolerance.accepts(a, e) {
                    let error = a.is_finite().then(|| tolerance.error(a, e));
                    mismatches.push(found(MismatchKind::OutOfTolerance, Some(a), error));
                }
            }
            (None, Some(Some(a))) if a.is_finite() => {
                mismatches.push(found(MismatchKind::UnexpectedValue, Some(a), None));
            }
            (None, _) => {}
        }
    }
    Report {
        tolerance,
        compared,
        mismatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[(&str, &str, Option<f64>)]) -> Table {
        let mut t = Table::new();
        for (g, n, v) in rows {
            t.insert(*g, *n, *v);
        }
        t
    }

    #[test]
    fn identical_tables_pass() {
        let e = table(&[
            ("1", "a", Some(1.0)),
            ("1", "b", Some(2.5)),
            ("2", "a", None),
        ]);
        let r = compare_tables(&e, &e.clone(), Tolerance::NCA_VS_PKNCA);
        assert!(r.is_ok());
        assert_eq!(r.compared, 3);
        assert_eq!(r.into_result(), Ok(()));
    }

    #[test]
    fn every_out_of_tolerance_value_is_reported() {
        let e = table(&[
            ("1", "a", Some(1.0)),
            ("1", "b", Some(2.0)),
            ("2", "a", Some(3.0)),
            ("2", "b", Some(4.0)),
        ]);
        let a = table(&[
            ("1", "a", Some(1.1)),
            ("1", "b", Some(2.0)),
            ("2", "a", Some(3.0)),
            ("2", "b", Some(4.4)),
        ]);
        let r = compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA);
        assert_eq!(r.compared, 4);
        assert_eq!(r.mismatches.len(), 2);
        let keys: Vec<_> = r
            .mismatches
            .iter()
            .map(|m| (m.group.as_str(), m.name.as_str()))
            .collect();
        assert_eq!(keys, vec![("1", "a"), ("2", "b")]);
        let text = r.to_string();
        assert!(text.contains("2 of 4 values outside tolerance"), "{text}");
        assert!(text.contains("[1] a") && text.contains("[2] b"), "{text}");
    }

    #[test]
    fn missing_entries_and_values_are_distinguished() {
        let e = table(&[
            ("1", "a", Some(1.0)),
            ("1", "b", Some(2.0)),
            ("1", "c", Some(3.0)),
        ]);
        let a = table(&[("1", "b", None), ("1", "c", Some(f64::NAN))]);
        let r = compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA);
        let kinds: Vec<_> = r.mismatches.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MismatchKind::MissingEntry,
                MismatchKind::MissingValue,
                MismatchKind::MissingValue
            ]
        );
    }

    #[test]
    fn expected_na_requires_unavailable_actual() {
        let e = table(&[("1", "a", None), ("1", "b", None), ("1", "c", None)]);
        let a = table(&[("1", "a", None), ("1", "b", Some(5.0))]);
        let r = compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA);
        assert_eq!(r.mismatches.len(), 1);
        assert_eq!(
            r.mismatches.first().map(|m| m.kind),
            Some(MismatchKind::UnexpectedValue)
        );
    }

    #[test]
    fn infinite_actual_is_reported_without_an_error_figure() {
        let e = table(&[("1", "a", Some(1.0))]);
        let a = table(&[("1", "a", Some(f64::INFINITY))]);
        let r = compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA);
        assert_eq!(r.mismatches.len(), 1);
        assert_eq!(r.mismatches.first().and_then(|m| m.error), None);
    }

    #[test]
    fn extra_actual_entries_are_ignored() {
        let e = table(&[("1", "a", Some(1.0))]);
        let a = table(&[("1", "a", Some(1.0)), ("1", "extra", Some(9.0))]);
        assert!(compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA).is_ok());
    }

    #[test]
    fn the_tolerance_is_the_one_given() {
        let e = table(&[("1", "a", Some(100.0))]);
        let a = table(&[("1", "a", Some(100.004))]);
        assert!(
            compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA)
                .mismatches
                .len()
                == 1
        );
        assert!(compare_tables(&e, &a, Tolerance::FIT_PARAMETERS).is_ok());
        assert!(compare_tables(&e, &a, Tolerance::displayed(1)).is_ok());
        assert!(
            compare_tables(&e, &a, Tolerance::displayed(3))
                .mismatches
                .len()
                == 1
        );
    }

    #[test]
    fn nan_inserted_is_stored_as_unavailable() {
        let mut t = Table::new();
        t.insert("1", "a", Some(f64::NAN));
        assert_eq!(t.get("1", "a"), Some(None));
        assert_eq!(t.get("1", "zzz"), None);
        assert_eq!(t.len(), 1);
        assert!(!t.is_empty());
    }

    #[test]
    fn report_into_result_carries_the_text() {
        let e = table(&[("1", "a", Some(1.0))]);
        let a = table(&[("1", "a", Some(2.0))]);
        let err = compare_tables(&e, &a, Tolerance::NCA_VS_PKNCA)
            .into_result()
            .unwrap_err();
        assert!(err.contains("1 of 1"), "{err}");
    }
}
