//! `cargo xtask conformance`: runs `caladrius-nca` on every oracle case of `oracle/expected/` and
//! writes `docs/conformance.md`, the validated parameters per case (AGENTS.md section 5).
//!
//! A value is validated when it is within `Tolerance::NCA_VS_PKNCA` of the expected one, or when
//! both are not available. The file records a floor per case (the number of validated values); a
//! run that validates fewer values than the floor fails and leaves the file unchanged, so floors
//! only go up (golden rule 2).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use caladrius_nca::{
    AucMethod, BlqAction, BlqPolicy, LambdaZOptions, MissingPolicy, NcaInput, NcaOptions, Route,
    TmaxTie, run as run_nca,
};
use caladrius_testkit::{OracleCase, Tolerance, list_cases, load_case};

use crate::console;
use crate::error::{Result, XtaskError};
use crate::workspace;

/// Output file, relative to the workspace root.
const OUTPUT: &str = "docs/conformance.md";
/// Marker of a floor line in the output file: `<!-- floor <case> <validated> -->`.
const FLOOR_MARK: &str = "<!-- floor ";

/// Validated and expected counts of one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Count {
    validated: usize,
    expected: usize,
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

fn blq_action(text: &str) -> Result<BlqAction> {
    match text {
        "keep" => Ok(BlqAction::Keep),
        "drop" => Ok(BlqAction::Drop),
        other => Err(XtaskError::new(format!(
            "unknown BLQ action {other:?} in the oracle options"
        ))),
    }
}

/// The engine options matching the PKNCA options recorded with the case.
fn options_of(case: &OracleCase) -> Result<NcaOptions> {
    let pk = &case.options.pknca_options;
    let unknown = |what: &str, value: &str| {
        XtaskError::new(format!(
            "{}: unknown {what} {value:?} in the oracle options",
            case.name
        ))
    };
    let auc_method = match pk.auc_method.as_str() {
        "linear" => AucMethod::Linear,
        "lin up/log down" => AucMethod::LinUpLogDown,
        other => return Err(unknown("AUC method", other)),
    };
    let missing = match pk.conc_na.as_str() {
        "drop" => MissingPolicy::Drop,
        other => return Err(unknown("missing-value rule", other)),
    };
    Ok(NcaOptions {
        auc_method,
        missing,
        blq: BlqPolicy::Position {
            first: blq_action(&pk.conc_blq.first)?,
            middle: blq_action(&pk.conc_blq.middle)?,
            last: blq_action(&pk.conc_blq.last)?,
        },
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
    match case.options.route.as_str() {
        "extravascular" => Ok(Route::Extravascular),
        "iv_bolus" => Ok(Route::IvBolus),
        other => Err(XtaskError::new(format!(
            "{}: unknown route {other:?} in the oracle options",
            case.name
        ))),
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
        let actual = results.get(subject).and_then(|r| r.get(name));
        let ok = match (expected, actual) {
            (Some(e), Some(a)) => Tolerance::NCA_VS_PKNCA.accepts(a, e),
            (None, None) => true,
            _ => false,
        };
        let count = counts.entry(name).or_default();
        count.expected += 1;
        count.validated += usize::from(ok);
    }
    let parameters = case
        .options
        .parameters
        .iter()
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

/// Floors recorded in a previous output: case name to validated count.
fn parse_floors(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(FLOOR_MARK))
        .filter_map(|rest| rest.strip_suffix("-->"))
        .filter_map(|rest| {
            let mut words = rest.split_whitespace();
            let case = words.next()?;
            let validated = words.next()?.parse().ok()?;
            Some((case.to_owned(), validated))
        })
        .collect()
}

/// Every case of the previous floors must still exist and validate at least as many values.
fn check_floors(floors: &BTreeMap<String, usize>, reports: &[CaseReport]) -> Result<()> {
    let mut drops = Vec::new();
    for (case, &floor) in floors {
        match reports.iter().find(|r| &r.name == case) {
            None => drops.push(format!(
                "case {case} (floor {floor}) is no longer in oracle/"
            )),
            Some(report) => {
                let validated = report.total().validated;
                if validated < floor {
                    drops.push(format!(
                        "case {case}: {validated} values validated, floor is {floor}"
                    ));
                }
            }
        }
    }
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
        }
    });
    out.push_str("# Conformance\n\n");
    out.push_str(
        "Generated by `cargo xtask conformance`; never edit by hand. Each value of `oracle/expected/` is \
compared with `caladrius-nca`, within a relative error of 1e-6 (`Tolerance::NCA_VS_PKNCA`); a value \
expected as not available must be not available. The floor of each case (last lines) can only go up.\n\n",
    );
    out.push_str(&format!(
        "**Overall: {} of {} values validated ({:.1} %).**\n\n",
        all.validated,
        all.expected,
        percent(all)
    ));
    out.push_str(
        "| case | route | AUC method | parameters fully validated | values validated | % |\n",
    );
    out.push_str("|---|---|---|---|---|---|\n");
    for r in reports {
        let t = r.total();
        let full = r
            .parameters
            .iter()
            .filter(|(_, c)| c.validated == c.expected)
            .count();
        out.push_str(&format!(
            "| {} | {} | {} | {} / {} | {} / {} | {:.1} |\n",
            r.name,
            r.route,
            r.auc_method,
            full,
            r.parameters.len(),
            t.validated,
            t.expected,
            percent(t)
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
        out.push_str("| parameter | validated | status |\n|---|---|---|\n");
        for (name, c) in &r.parameters {
            let status = if c.validated == c.expected {
                "ok"
            } else {
                "FAILS"
            };
            out.push_str(&format!(
                "| `{name}` | {} / {} | {status} |\n",
                c.validated, c.expected
            ));
        }
    }
    out.push('\n');
    for r in reports {
        out.push_str(&format!(
            "{FLOOR_MARK}{} {} -->\n",
            r.name,
            r.total().validated
        ));
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
    let path = root.join(OUTPUT);
    let previous = read_previous(&path)?;
    check_floors(&parse_floors(&previous), &reports)?;
    let text = render(&reports);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| XtaskError::io("create", dir, &e))?;
    }
    fs::write(&path, &text).map_err(|e| XtaskError::io("write", &path, &e))?;
    for r in &reports {
        let t = r.total();
        console::out(&format!(
            "{:<20} {:>4} / {:<4} values validated ({:.1} %)",
            r.name,
            t.validated,
            t.expected,
            percent(t)
        ));
    }
    console::out(&format!("conformance: wrote {OUTPUT}"));
    Ok(())
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

    fn report(name: &str, validated: usize, expected: usize) -> CaseReport {
        CaseReport {
            name: name.to_owned(),
            route: "extravascular".to_owned(),
            auc_method: "linear".to_owned(),
            parameters: vec![(
                "cmax".to_owned(),
                Count {
                    validated,
                    expected,
                },
            )],
            errors: Vec::new(),
        }
    }

    #[test]
    fn rendered_floors_are_read_back() {
        let text = render(&[report("a", 3, 4), report("b", 2, 2)]);
        let floors = parse_floors(&text);
        assert_eq!(floors.get("a"), Some(&3));
        assert_eq!(floors.get("b"), Some(&2));
        assert!(text.contains("**Overall: 5 of 6 values validated (83.3 %).**"));
        assert!(text.contains("| `cmax` | 3 / 4 | FAILS |"));
    }

    #[test]
    fn floors_only_go_up() {
        let floors = parse_floors(&render(&[report("a", 3, 4)]));
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
    fn every_public_case_is_fully_validated() {
        for name in list_cases().unwrap() {
            let report = evaluate(&load_case(&name).unwrap()).unwrap();
            let total = report.total();
            assert!(report.errors.is_empty(), "{name}: {:?}", report.errors);
            assert_eq!(total.validated, total.expected, "{name}");
        }
    }
}
