//! `cargo xtask conformance`: runs `caladrius-nca` on every oracle case of `oracle/expected/` and
//! writes `docs/conformance.md`, the validated parameters per case (AGENTS.md section 5).
//!
//! A value is validated when it is within `Tolerance::NCA_VS_PKNCA` of the expected one, or when
//! it is expected as not available and the engine computes the parameter and reports it as not
//! available (a parameter the engine does not compute never matches). The file records a floor per
//! parameter per case: the number of validated values and the number of expected rows. A run
//! fails, and leaves the file unchanged, when a parameter validates fewer values, loses expected
//! rows, gains unvalidated rows, or has disappeared; when a floor line is malformed; or when a
//! non-empty previous file holds no floor at all. So floors only go up (golden rule 2). The file
//! is written to a temporary file then renamed.
//!
//! A value covered by a documented difference (`documented_differences` in the case's options file,
//! `specs/differences.md`) is neither validated nor a failure: it is counted in its own column and
//! left out of the validated and expected totals. A difference cannot hide a regression: moving a
//! row from validated to documented lowers the validated floor and fails the run.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use caladrius_nca::{
    AucMethod, BlqAction, BlqPolicy, LambdaZOptions, MissingPolicy, NcaInput, NcaOptions,
    NcaResult, NegativePolicy, Route, StartPolicy, TmaxTie, run as run_nca,
};
use caladrius_testkit::oracle::{BlqRule, NaRule};
use caladrius_testkit::{OracleCase, Tolerance, list_cases, load_case};

use crate::console;
use crate::error::{Result, XtaskError};
use crate::workspace;

/// Output file, relative to the workspace root.
const OUTPUT: &str = "docs/conformance.md";
/// Marker of a floor line in the output file:
/// `<!-- floor <case> <parameter> <validated> <expected> <documented> -->` (the documented count is
/// optional when reading, for files written before documented differences existed).
const FLOOR_MARK: &str = "<!-- floor ";
/// Closing of a floor line.
const FLOOR_END: &str = "-->";

/// Validated, expected and documented counts of one parameter. `expected` leaves out the rows
/// covered by a documented difference, which are counted in `documented` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Count {
    validated: usize,
    expected: usize,
    documented: usize,
}

/// The outcome of one oracle case.
#[derive(Debug, Clone, PartialEq)]
struct CaseReport {
    name: String,
    route: String,
    auc_method: String,
    /// Per parameter, in the order of the case's parameter list.
    parameters: Vec<(String, Count)>,
    /// Subjects the engine refused, with the error message.
    errors: Vec<String>,
}

impl CaseReport {
    fn total(&self) -> Count {
        self.parameters
            .iter()
            .fold(Count::default(), |acc, (_, c)| Count {
                validated: acc.validated + c.validated,
                expected: acc.expected + c.expected,
                documented: acc.documented + c.documented,
            })
    }
}

fn percent(count: Count) -> f64 {
    if count.expected == 0 {
        100.0
    } else {
        100.0 * count.validated as f64 / count.expected as f64
    }
}

fn blq_action(rule: BlqRule) -> BlqAction {
    match rule {
        BlqRule::Keep => BlqAction::Keep,
        BlqRule::Drop => BlqAction::Drop,
        BlqRule::Set(x) => BlqAction::Set(x),
    }
}

/// The engine options matching the PKNCA options recorded with the case, and the engine hints the
/// case file carries for what PKNCA has no option for (start and negative-value policies).
fn options_of(case: &OracleCase) -> Result<NcaOptions> {
    let pk = &case.options.pknca_options;
    let unknown = |what: &str, value: &str| {
        XtaskError::new(format!(
            "{}: unknown {what} {value:?} in the oracle options",
            case.name
        ))
    };
    let incomplete = || {
        XtaskError::new(format!(
            "{}: the BLQ rule of the oracle options names neither first/middle/last nor before/after Tmax completely",
            case.name
        ))
    };
    let auc_method = match pk.auc_method.as_str() {
        "linear" => AucMethod::Linear,
        "lin up/log down" => AucMethod::LinUpLogDown,
        other => return Err(unknown("AUC method", other)),
    };
    let missing = match pk.conc_na {
        NaRule::Drop => MissingPolicy::Drop,
        NaRule::Replace(x) => MissingPolicy::Replace(x),
    };
    let rule = &pk.conc_blq;
    let blq = match (
        rule.first,
        rule.middle,
        rule.last,
        rule.before_tmax,
        rule.after_tmax,
    ) {
        (Some(first), Some(middle), Some(last), None, None) => BlqPolicy::Position {
            first: blq_action(first),
            middle: blq_action(middle),
            last: blq_action(last),
        },
        (None, None, None, Some(before), Some(after)) => BlqPolicy::Tmax {
            before: blq_action(before),
            after: blq_action(after),
        },
        _ => return Err(incomplete()),
    };
    let hints = &case.options.engine;
    let start = match hints.start.as_deref() {
        None => StartPolicy::default(),
        Some("none") => StartPolicy::None,
        Some("zero") => StartPolicy::Zero,
        Some("c0") => StartPolicy::C0,
        Some(other) => return Err(unknown("start policy", other)),
    };
    let negative = match hints.negative.as_deref() {
        None => NegativePolicy::default(),
        Some("error") => NegativePolicy::Error,
        Some("allow") => NegativePolicy::Allow,
        Some("set_zero") => NegativePolicy::SetZero,
        Some(other) => return Err(unknown("negative-value policy", other)),
    };
    Ok(NcaOptions {
        auc_method,
        missing,
        blq,
        start,
        negative,
        tmax_tie: if pk.first_tmax {
            TmaxTie::First
        } else {
            TmaxTie::Last
        },
        lambda_z: LambdaZOptions {
            min_points: usize::try_from(pk.min_hl_points).unwrap_or(usize::MAX),
            allow_tmax: pk.allow_tmax_in_half_life,
            adj_r_squared_factor: pk.adj_r_squared_factor,
        },
        ..NcaOptions::default()
    })
}

fn route_of(case: &OracleCase) -> Result<Route> {
    match (case.options.route.as_str(), case.options.infusion_duration) {
        ("extravascular", _) => Ok(Route::Extravascular),
        ("iv_bolus", _) => Ok(Route::IvBolus),
        ("iv_infusion", Some(duration)) => Ok(Route::IvInfusion { duration }),
        ("iv_infusion", None) => Err(XtaskError::new(format!(
            "{}: route iv_infusion without an infusion_duration in the oracle options",
            case.name
        ))),
        (other, _) => Err(XtaskError::new(format!(
            "{}: unknown route {other:?} in the oracle options",
            case.name
        ))),
    }
}

/// Whether one expected row is reproduced. `result` is the engine's result for the subject, `None`
/// when the engine refused the subject. A value expected as not available only matches when the
/// engine computes the parameter (`parameter(name)` is `Some`) and reports it as not available.
fn row_validated(expected: Option<f64>, result: Option<&NcaResult>, name: &str) -> bool {
    let Some(result) = result else {
        return false;
    };
    match (expected, result.parameter(name)) {
        (_, None) => false,
        (Some(e), Some(p)) => p
            .value()
            .is_some_and(|a| Tolerance::NCA_VS_PKNCA.accepts(a, e)),
        (None, Some(p)) => p.value().is_none(),
    }
}

/// Runs the engine on every subject of `case` and counts the validated values per parameter.
fn evaluate(case: &OracleCase) -> Result<CaseReport> {
    let options = options_of(case)?;
    let route = route_of(case)?;
    let mut results = BTreeMap::new();
    let mut errors = Vec::new();
    for profile in &case.dataset.profiles {
        let input = NcaInput {
            time: profile.time.clone(),
            conc: profile.conc.clone(),
            dose: profile.dose,
            route,
            options: options.clone(),
        };
        match run_nca(&input) {
            Ok(result) => {
                results.insert(profile.subject.to_string(), result);
            }
            Err(e) => errors.push(format!("subject {}: {e}", profile.subject)),
        }
    }
    let mut counts: BTreeMap<&str, Count> = BTreeMap::new();
    for (subject, name, expected) in case.expected.iter() {
        let count = counts.entry(name).or_default();
        if case
            .options
            .documented_differences
            .iter()
            .any(|d| d.covers(subject, name))
        {
            count.documented += 1;
            continue;
        }
        let ok = row_validated(expected, results.get(subject), name);
        count.expected += 1;
        count.validated += usize::from(ok);
    }
    let mut seen = std::collections::BTreeSet::new();
    let parameters = case
        .options
        .parameters
        .iter()
        .filter(|p| seen.insert(p.as_str()))
        .map(|p| {
            (
                p.clone(),
                counts.get(p.as_str()).copied().unwrap_or_default(),
            )
        })
        .collect();
    Ok(CaseReport {
        name: case.name.clone(),
        route: case.options.route.clone(),
        auc_method: case.options.pknca_options.auc_method.clone(),
        parameters,
        errors,
    })
}

/// Floors recorded in a previous output.
#[derive(Debug, Default, PartialEq, Eq)]
struct Floors {
    /// Per case and parameter: the validated and expected counts.
    parameters: BTreeMap<(String, String), Count>,
    /// Per case: the total validated count, from the first file format (one total per case). Still
    /// enforced, then replaced by per-parameter floors when the file is rewritten.
    legacy_totals: BTreeMap<String, usize>,
}

impl Floors {
    fn is_empty(&self) -> bool {
        self.parameters.is_empty() && self.legacy_totals.is_empty()
    }
}

/// Reads the floors of a previous output. A line that starts a floor but does not parse is an
/// error, and so is a non-empty text without any floor: a floor must never vanish silently.
fn parse_floors(text: &str) -> Result<Floors> {
    let mut floors = Floors::default();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if !line.starts_with(FLOOR_MARK) {
            continue;
        }
        let bad = |why: &str| {
            XtaskError::new(format!(
                "{OUTPUT} line {}: malformed floor line ({why}): {line:?}; expected \
`{FLOOR_MARK}<case> <parameter> <validated> <expected> <documented> {FLOOR_END}`. Fix or restore the file \
from version control; floors are not dropped silently",
                index + 1
            ))
        };
        let body = line
            .strip_prefix(FLOOR_MARK)
            .and_then(|rest| rest.strip_suffix(FLOOR_END))
            .ok_or_else(|| bad("missing closing marker"))?;
        let words: Vec<&str> = body.split_whitespace().collect();
        let number = |word: &str| {
            word.parse::<usize>()
                .map_err(|_| bad("count is not a whole number"))
        };
        match words.as_slice() {
            [case, parameter, validated, expected] | [case, parameter, validated, expected, _] => {
                let count = Count {
                    validated: number(validated)?,
                    expected: number(expected)?,
                    documented: match words.get(4) {
                        Some(word) => number(word)?,
                        None => 0,
                    },
                };
                if count.validated > count.expected {
                    return Err(bad("validated is larger than expected"));
                }
                let key = ((*case).to_owned(), (*parameter).to_owned());
                if floors.parameters.insert(key, count).is_some() {
                    return Err(bad("duplicated floor"));
                }
            }
            [case, validated] => {
                if floors
                    .legacy_totals
                    .insert((*case).to_owned(), number(validated)?)
                    .is_some()
                {
                    return Err(bad("duplicated floor"));
                }
            }
            _ => return Err(bad("wrong number of fields")),
        }
    }
    if floors.is_empty() && !text.trim().is_empty() {
        return Err(XtaskError::new(format!(
            "{OUTPUT} is not empty but holds no floor line; refusing to regenerate it without \
floors. Restore the file from version control"
        )));
    }
    Ok(floors)
}

/// Every floor must still be met: the case and the parameter exist, the parameter validates at
/// least as many values, keeps at least as many expected rows, and has no more unvalidated rows.
fn check_floors(floors: &Floors, reports: &[CaseReport]) -> Result<()> {
    let mut drops = Vec::new();
    for ((case, parameter), floor) in &floors.parameters {
        let Some(report) = reports.iter().find(|r| &r.name == case) else {
            drops.push(format!("case {case} is no longer in oracle/"));
            continue;
        };
        let Some((_, count)) = report.parameters.iter().find(|(name, _)| name == parameter) else {
            drops.push(format!(
                "{case}: parameter {parameter} is no longer checked"
            ));
            continue;
        };
        if count.validated < floor.validated {
            drops.push(format!(
                "{case}: {parameter} validates {} values, floor is {}",
                count.validated, floor.validated
            ));
        }
        // Rows documented as differences are still rows of the oracle.
        if count.expected + count.documented < floor.expected + floor.documented {
            drops.push(format!(
                "{case}: {parameter} has {} expected rows, floor is {}",
                count.expected + count.documented,
                floor.expected + floor.documented
            ));
        }
        let unvalidated = count.expected.saturating_sub(count.validated);
        let floor_unvalidated = floor.expected.saturating_sub(floor.validated);
        if unvalidated > floor_unvalidated {
            drops.push(format!(
                "{case}: {parameter} has {unvalidated} unvalidated rows, floor allows {floor_unvalidated}"
            ));
        }
    }
    for (case, &floor) in &floors.legacy_totals {
        match reports.iter().find(|r| &r.name == case) {
            None => drops.push(format!("case {case} is no longer in oracle/")),
            Some(report) => {
                let validated = report.total().validated;
                if validated < floor {
                    drops.push(format!(
                        "{case}: {validated} values validated, floor is {floor}"
                    ));
                }
            }
        }
    }
    drops.sort();
    drops.dedup();
    if drops.is_empty() {
        Ok(())
    } else {
        Err(XtaskError::new(format!(
            "conformance went down; {OUTPUT} is left unchanged:\n  {}",
            drops.join("\n  ")
        )))
    }
}

/// The Markdown document. Deterministic: no date, cases and parameters in a fixed order.
fn render(reports: &[CaseReport]) -> String {
    let mut out = String::new();
    let all = reports.iter().fold(Count::default(), |acc, r| {
        let t = r.total();
        Count {
            validated: acc.validated + t.validated,
            expected: acc.expected + t.expected,
            documented: acc.documented + t.documented,
        }
    });
    out.push_str("# Conformance\n\n");
    out.push_str(
        "Generated by `cargo xtask conformance`; never edit by hand. Each value of `oracle/expected/` is \
compared with `caladrius-nca`, within a relative error of 1e-6 (`Tolerance::NCA_VS_PKNCA`); a value \
expected as not available must be not available. A value covered by a documented difference (`specs/differences.md`) is neither validated nor a failure: it has its own column and is left out of the totals. The floors (last lines: validated, expected and documented rows per parameter per case) can only go up.\n\n",
    );
    out.push_str(&format!(
        "**Overall: {} of {} values validated ({:.1} %).**",
        all.validated,
        all.expected,
        percent(all)
    ));
    if all.documented > 0 {
        out.push_str(&format!(
            " {} further values are documented differences, neither validated nor failures.",
            all.documented
        ));
    }
    out.push_str("\n\n");
    out.push_str(
        "| case | route | AUC method | parameters fully validated | values validated | % | documented differences |\n",
    );
    out.push_str("|---|---|---|---|---|---|---|\n");
    for r in reports {
        let t = r.total();
        let full = r
            .parameters
            .iter()
            .filter(|(_, c)| c.validated == c.expected)
            .count();
        out.push_str(&format!(
            "| {} | {} | {} | {} / {} | {} / {} | {:.1} | {} |\n",
            r.name,
            r.route,
            r.auc_method,
            full,
            r.parameters.len(),
            t.validated,
            t.expected,
            percent(t),
            t.documented
        ));
    }
    for r in reports {
        out.push_str(&format!("\n## {}\n\n", r.name));
        for e in &r.errors {
            out.push_str(&format!("- engine error: {e}\n"));
        }
        if !r.errors.is_empty() {
            out.push('\n');
        }
        out.push_str(
            "| parameter | validated | documented differences | status |\n|---|---|---|---|\n",
        );
        for (name, c) in &r.parameters {
            let status = if c.validated == c.expected {
                "ok"
            } else {
                "FAILS"
            };
            out.push_str(&format!(
                "| `{name}` | {} / {} | {} | {status} |\n",
                c.validated, c.expected, c.documented
            ));
        }
    }
    out.push('\n');
    for r in reports {
        for (name, c) in &r.parameters {
            out.push_str(&format!(
                "{FLOOR_MARK}{} {name} {} {} {} {FLOOR_END}\n",
                r.name, c.validated, c.expected, c.documented
            ));
        }
    }
    out
}

pub fn run() -> Result<()> {
    let root = workspace::root()?;
    let oracle_error = |e: caladrius_testkit::OracleError| XtaskError::new(e.to_string());
    let mut reports = Vec::new();
    for name in list_cases().map_err(oracle_error)? {
        let case = load_case(&name).map_err(oracle_error)?;
        reports.push(evaluate(&case)?);
    }
    update_file(&root.join(OUTPUT), &reports)?;
    for r in &reports {
        let t = r.total();
        let documented = if t.documented > 0 {
            format!(", {} documented differences", t.documented)
        } else {
            String::new()
        };
        console::out(&format!(
            "{:<20} {:>4} / {:<4} values validated ({:.1} %){documented}",
            r.name,
            t.validated,
            t.expected,
            percent(t)
        ));
    }
    console::out(&format!("conformance: wrote {OUTPUT}"));
    Ok(())
}

/// Checks the floors recorded in `path` against `reports`, then rewrites `path`. Any error leaves
/// the file as it was.
fn update_file(path: &Path, reports: &[CaseReport]) -> Result<()> {
    let previous = read_previous(path)?;
    check_floors(&parse_floors(&previous)?, reports)?;
    write_atomic(path, &render(reports))
}

/// Writes `text` to a temporary file next to `path`, then renames it over `path`, so a crash never
/// leaves a half-written file.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| XtaskError::io("create", dir, &e))?;
    }
    let mut temp_name = path.as_os_str().to_owned();
    temp_name.push(".tmp");
    let temp = std::path::PathBuf::from(temp_name);
    fs::write(&temp, text).map_err(|e| XtaskError::io("write", &temp, &e))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        XtaskError::io("rename onto", path, &e)
    })
}

/// The previous output, or an empty text when there is none yet.
fn read_previous(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(XtaskError::io("read", path, &e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(name: &str, validated: usize, expected: usize) -> (String, Count) {
        (
            name.to_owned(),
            Count {
                validated,
                expected,
                documented: 0,
            },
        )
    }

    fn report_of(name: &str, parameters: Vec<(String, Count)>) -> CaseReport {
        CaseReport {
            name: name.to_owned(),
            route: "extravascular".to_owned(),
            auc_method: "linear".to_owned(),
            parameters,
            errors: Vec::new(),
        }
    }

    fn report(name: &str, validated: usize, expected: usize) -> CaseReport {
        report_of(name, vec![param("cmax", validated, expected)])
    }

    /// A fresh scratch directory, removed by the caller.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "caladrius-xtask-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rendered_floors_are_read_back_per_parameter() {
        let text = render(&[
            report_of("a", vec![param("cmax", 3, 4), param("tmax", 2, 2)]),
            report("b", 2, 2),
        ]);
        let floors = parse_floors(&text).unwrap();
        let get = |case: &str, p: &str| floors.parameters.get(&(case.to_owned(), p.to_owned()));
        assert_eq!(
            get("a", "cmax"),
            Some(&Count {
                validated: 3,
                expected: 4,
                documented: 0
            })
        );
        assert_eq!(
            get("a", "tmax"),
            Some(&Count {
                validated: 2,
                expected: 2,
                documented: 0
            })
        );
        assert!(get("b", "cmax").is_some());
        assert!(floors.legacy_totals.is_empty());
        assert!(text.contains("<!-- floor a cmax 3 4 0 -->"));
        assert!(text.contains("**Overall: 7 of 8 values validated (87.5 %).**"));
        assert!(text.contains("| `cmax` | 3 / 4 | 0 | FAILS |"));
    }

    fn with_documented(
        name: &str,
        validated: usize,
        expected: usize,
        documented: usize,
    ) -> CaseReport {
        report_of(
            name,
            vec![(
                "cmax".to_owned(),
                Count {
                    validated,
                    expected,
                    documented,
                },
            )],
        )
    }

    #[test]
    fn documented_differences_have_their_own_column_and_are_not_counted_elsewhere() {
        let text = render(&[with_documented("a", 3, 3, 2), report("b", 2, 2)]);
        // Neither validated nor expected: 5 of 5 values validated, 2 further documented.
        assert!(
            text.contains("**Overall: 5 of 5 values validated (100.0 %).** 2 further values"),
            "{text}"
        );
        assert!(text.contains("| documented differences |"), "{text}");
        assert!(text.contains("| 1 / 1 | 3 / 3 | 100.0 | 2 |"), "{text}");
        assert!(text.contains("| `cmax` | 3 / 3 | 2 | ok |"), "{text}");
        assert!(text.contains("<!-- floor a cmax 3 3 2 -->"), "{text}");
        let floors = parse_floors(&text).unwrap();
        let floor = floors
            .parameters
            .get(&("a".to_owned(), "cmax".to_owned()))
            .unwrap();
        assert_eq!(
            (floor.validated, floor.expected, floor.documented),
            (3, 3, 2)
        );
    }

    #[test]
    fn a_documented_difference_cannot_hide_a_regression_or_a_lost_row() {
        // Before: 4 of 5 validated, one failure. Documenting the failure is allowed.
        let floors = parse_floors(&render(&[report("a", 4, 5)])).unwrap();
        assert!(check_floors(&floors, &[with_documented("a", 4, 4, 1)]).is_ok());
        // Documenting a validated row lowers the validated count: refused.
        let err = check_floors(&floors, &[with_documented("a", 3, 3, 2)]).unwrap_err();
        assert!(
            err.to_string().contains("validates 3 values, floor is 4"),
            "{err}"
        );
        // Dropping rows from the oracle is still refused, documented or not.
        let err = check_floors(&floors, &[with_documented("a", 4, 4, 0)]).unwrap_err();
        assert!(
            err.to_string().contains("4 expected rows, floor is 5"),
            "{err}"
        );
        // A floor written before the column existed (four fields) still reads.
        let old = "<!-- floor a cmax 4 5 -->\n";
        let floors = parse_floors(old).unwrap();
        assert!(check_floors(&floors, &[with_documented("a", 4, 4, 1)]).is_ok());
    }

    #[test]
    fn floors_only_go_up() {
        let floors = parse_floors(&render(&[report("a", 3, 4)])).unwrap();
        assert!(check_floors(&floors, &[report("a", 3, 4)]).is_ok());
        assert!(check_floors(&floors, &[report("a", 4, 4)]).is_ok());
        let down = check_floors(&floors, &[report("a", 2, 4)]).unwrap_err();
        assert!(down.to_string().contains("floor is 3"), "{down}");
        let gone = check_floors(&floors, &[report("b", 1, 1)]).unwrap_err();
        assert!(gone.to_string().contains("no longer"), "{gone}");
        // A new case has no floor yet.
        assert!(check_floors(&floors, &[report("a", 3, 4), report("c", 0, 1)]).is_ok());
    }

    #[test]
    fn a_lost_parameter_is_not_hidden_by_a_gain_elsewhere() {
        let floors = parse_floors(&render(&[report_of(
            "a",
            vec![param("cmax", 2, 4), param("tmax", 1, 4)],
        )]))
        .unwrap();
        // The case total goes up (3 -> 5) but cmax went down.
        let now = report_of("a", vec![param("cmax", 1, 4), param("tmax", 4, 4)]);
        let err = check_floors(&floors, &[now]).unwrap_err();
        assert!(err.to_string().contains("cmax validates 1"), "{err}");
        // A parameter that is no longer checked is a drop.
        let missing = report_of("a", vec![param("tmax", 4, 4)]);
        let err = check_floors(&floors, &[missing]).unwrap_err();
        assert!(err.to_string().contains("cmax is no longer"), "{err}");
    }

    #[test]
    fn new_unvalidated_or_lost_rows_fail() {
        let floors = parse_floors(&render(&[report("a", 4, 4)])).unwrap();
        // Two oracle rows added that are not reproduced: the validated count still meets the floor.
        let err = check_floors(&floors, &[report("a", 4, 6)]).unwrap_err();
        assert!(err.to_string().contains("2 unvalidated rows"), "{err}");
        // Rows added and validated are fine.
        assert!(check_floors(&floors, &[report("a", 6, 6)]).is_ok());
        // Rows removed from the oracle are a drop.
        let err = check_floors(&floors, &[report("a", 3, 3)]).unwrap_err();
        assert!(err.to_string().contains("3 expected rows"), "{err}");
    }

    #[test]
    fn a_malformed_floor_line_is_an_error() {
        let good = render(&[report("a", 3, 4)]);
        for bad in [
            "<!-- floor a cmax 3 -->",
            "<!-- floor a cmax x 4 -->",
            "<!-- floor a cmax 5 4 -->",
            "<!-- floor a cmax 3 4",
            "<!-- floor a cmax 3 4 5 6 -->",
            "<!-- floor a cmax 3 4 x -->",
            "<!-- floor -->",
        ] {
            let text = format!("{good}{bad}\n");
            let err = parse_floors(&text).unwrap_err();
            assert!(
                err.to_string().contains("malformed floor line"),
                "{bad}: {err}"
            );
        }
        let twice = format!("{good}<!-- floor a cmax 3 4 -->\n");
        assert!(parse_floors(&twice).is_err());
    }

    #[test]
    fn a_non_empty_file_without_floors_is_an_error() {
        let err = parse_floors("# Conformance\n\nno floors here\n").unwrap_err();
        assert!(err.to_string().contains("holds no floor"), "{err}");
        // No file yet, or an empty one, is a first run.
        assert!(parse_floors("").unwrap().is_empty());
        assert!(parse_floors("\n \n").unwrap().is_empty());
    }

    #[test]
    fn first_format_totals_are_still_enforced() {
        let floors = parse_floors("<!-- floor a 3 -->\n").unwrap();
        assert_eq!(floors.legacy_totals.get("a"), Some(&3));
        assert!(check_floors(&floors, &[report("a", 3, 4)]).is_ok());
        assert!(check_floors(&floors, &[report("a", 2, 4)]).is_err());
        assert!(check_floors(&floors, &[report("b", 4, 4)]).is_err());
    }

    fn profile_result() -> NcaResult {
        let input = NcaInput {
            time: vec![0.0, 1.0, 2.0, 4.0, 6.0, 8.0],
            conc: vec![0.0, 8.0, 6.0, 3.0, 1.5, 0.75],
            // A missing dose: the dose-dependent parameters are not calculated (NCA-DAT-10).
            dose: f64::NAN,
            route: Route::Extravascular,
            options: NcaOptions::default(),
        };
        run_nca(&input).unwrap()
    }

    #[test]
    fn not_available_matches_only_a_computed_parameter() {
        let result = profile_result();
        // Computed and not calculated on both sides: validated.
        assert!(
            result
                .parameter("cl.obs")
                .is_some_and(|p| p.value().is_none())
        );
        assert!(row_validated(None, Some(&result), "cl.obs"));
        // The engine does not compute this name at all: never validated.
        assert!(result.parameter("no.such.parameter").is_none());
        assert!(!row_validated(None, Some(&result), "no.such.parameter"));
        assert!(!row_validated(
            Some(1.0),
            Some(&result),
            "no.such.parameter"
        ));
        // A refused subject validates nothing, not even a not-available row.
        assert!(!row_validated(None, None, "cl.obs"));
        // A computed value matches within tolerance only.
        let cmax = result.get("cmax").unwrap();
        assert!(row_validated(Some(cmax), Some(&result), "cmax"));
        assert!(!row_validated(Some(cmax * 1.001), Some(&result), "cmax"));
        // Expected not available but computed: not validated.
        assert!(!row_validated(None, Some(&result), "cmax"));
        // Expected a value but the engine says not calculated: not validated.
        assert!(!row_validated(Some(1.0), Some(&result), "cl.obs"));
    }

    #[test]
    fn the_file_is_written_atomically() {
        let dir = scratch("atomic");
        let path = dir.join("docs").join("conformance.md");
        write_atomic(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert!(!dir.join("docs").join("conformance.md.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failing_gate_leaves_the_file_unchanged() {
        let dir = scratch("gate");
        let path = dir.join("conformance.md");
        update_file(&path, &[report("a", 3, 4)]).unwrap();
        let before = fs::read_to_string(&path).unwrap();
        assert!(update_file(&path, &[report("a", 2, 4)]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        // A file with its floors stripped is refused, not regenerated.
        fs::write(&path, "# Conformance\n").unwrap();
        assert!(update_file(&path, &[report("a", 4, 4)]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Conformance\n");
        // Going up rewrites the file, and a second run is byte-identical.
        fs::write(&path, &before).unwrap();
        update_file(&path, &[report("a", 4, 4)]).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        update_file(&path, &[report("a", 4, 4)]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), after);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_public_case_is_fully_validated_and_every_edge_case_runs() {
        for name in list_cases().unwrap() {
            let report = evaluate(&load_case(&name).unwrap()).unwrap();
            let total = report.total();
            assert!(report.errors.is_empty(), "{name}: {:?}", report.errors);
            // The edge cases (task T-012) record what the engine does not yet reproduce: their
            // floors go up as the engine improves, but they are not required to be complete.
            if !name.starts_with("edge_") {
                assert_eq!(total.validated, total.expected, "{name}");
            }
        }
    }
}
