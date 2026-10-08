//! Loaders for the step-3 oracle: closed-form model values and weighted least-squares fits.
//!
//! Layout (see `oracle/README.md`):
//!
//! - `oracle/expected/models/model_<case>.csv` and `.options.json`: exact values of one model on a
//!   time grid, and its secondary parameters;
//! - `oracle/expected/fit/fit_<dataset>_<weighting>.csv` and `.options.json`: reference fits of
//!   one dataset, one weighting, every subject.
//!
//! Both use the long format `subject,parameter,value` of the NCA cases; the "subject" of a model
//! case is `t=<time>` for a grid value and `scalar` for a secondary parameter.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::compare::Table;
use crate::oracle::{Dataset, OracleError, Profile, load_dataset, oracle_dir, parse_expected};

fn read(path: &Path) -> Result<String, OracleError> {
    fs::read_to_string(path).map_err(|e| OracleError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

fn json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, OracleError> {
    let text = read(path)?;
    serde_json::from_str(&text).map_err(|e| OracleError::Json {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

fn names_in(dir: &Path) -> Result<Vec<String>, OracleError> {
    let entries = fs::read_dir(dir).map_err(|e| OracleError::Io {
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

// ---------------------------------------------------------------- models

#[derive(Debug, Deserialize)]
struct ModelOptions {
    case: String,
    model: String,
    dose: f64,
    parameters: BTreeMap<String, f64>,
    times: Vec<f64>,
    #[serde(default)]
    note: String,
    n_values: usize,
}

/// One model case: a model id, a dose, named parameters (`v`, `cl` or `k`, `ka`, `tlag`, `dur`), a
/// time grid, and the exact values.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCase {
    pub name: String,
    /// Model id of `specs/models.md` MOD-GEN-05, for example `pk1.oral_1_lag`.
    pub model: String,
    pub dose: f64,
    /// Parameters by name (MOD-VOC-01). Exactly one of `cl` and `k` is present.
    pub parameters: BTreeMap<String, f64>,
    /// Times to evaluate, in the order of the file (may include a time before the dose).
    pub times: Vec<f64>,
    /// For each time, the row key in `expected` (`t=<text>`), in the same order as `times`.
    pub time_keys: Vec<String>,
    /// Grid values (`conc`, `auc`) keyed by the time key; secondary parameters keyed by `scalar`.
    pub expected: Table,
    pub note: String,
}

/// Names of the model cases in `oracle/expected/models/`, sorted.
pub fn list_model_cases() -> Result<Vec<String>, OracleError> {
    names_in(&oracle_dir().join("expected").join("models"))
}

/// Loads one model case and checks that its files agree.
pub fn load_model_case(name: &str) -> Result<ModelCase, OracleError> {
    let dir = oracle_dir().join("expected").join("models");
    let options_path = dir.join(format!("{name}.options.json"));
    let options: ModelOptions = json(&options_path)?;
    let here = options_path.display().to_string();
    let inconsistent = |message: String| OracleError::Inconsistent {
        path: here.clone(),
        message,
    };
    if options.case != name {
        return Err(inconsistent(format!(
            "case field is {:?}, file name says {name:?}",
            options.case
        )));
    }
    let expected_path = dir.join(format!("{name}.csv"));
    let expected = parse_expected(&expected_path.display().to_string(), &read(&expected_path)?)?;
    if expected.len() != options.n_values {
        return Err(inconsistent(format!(
            "options say {} values, the expected table has {}",
            options.n_values,
            expected.len()
        )));
    }
    // Row keys: the group `t=<text>` whose number equals the time.
    let mut keys: BTreeMap<u64, String> = BTreeMap::new();
    for (group, _, _) in expected.iter() {
        if let Some(text) = group.strip_prefix("t=") {
            if let Ok(t) = text.parse::<f64>() {
                keys.insert(t.to_bits(), group.to_string());
            }
        }
    }
    let mut time_keys = Vec::new();
    for t in &options.times {
        match keys.get(&t.to_bits()) {
            Some(key) => time_keys.push(key.clone()),
            None => {
                return Err(inconsistent(format!(
                    "time {t} of the options has no rows in the expected table"
                )));
            }
        }
    }
    Ok(ModelCase {
        name: name.to_string(),
        model: options.model,
        dose: options.dose,
        parameters: options.parameters,
        times: options.times,
        time_keys,
        expected,
        note: options.note,
    })
}

// ---------------------------------------------------------------- fits

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationsOptions {
    /// `"all"` or `"time_greater_than"`.
    #[serde(rename = "use")]
    rule: String,
    #[serde(default)]
    time: Option<f64>,
}

/// Options the reference fits were computed with, to be given to the fitting engine.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FitCaseOptions {
    /// `"analytic"` (the statistics of the oracle use the analytic Jacobian).
    pub derivatives: String,
    /// `"relative_decrease"`.
    pub criterion: String,
    /// The convergence threshold of the criterion (tight: 1e-10, `specs/fit.md` FIT-CNV-04).
    pub convergence: f64,
    pub max_iterations: usize,
    /// Level of the confidence intervals (0.95).
    pub confidence_level: f64,
}

#[derive(Debug, Deserialize)]
struct FitOptions {
    case: String,
    dataset: String,
    data_file: String,
    model: String,
    weighting: String,
    #[serde(default)]
    note: String,
    observations: ObservationsOptions,
    initial_estimates: BTreeMap<String, BTreeMap<String, f64>>,
    fit_options: FitCaseOptions,
    subjects: Vec<String>,
    #[serde(default)]
    no_fixed_point: Vec<String>,
    /// Parameters held at a given value, not fitted (for example `dur`).
    #[serde(default)]
    fixed: BTreeMap<String, f64>,
    n_values: usize,
}

/// Which observations of a profile are fitted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Observations {
    /// Every sample.
    All,
    /// Samples strictly after this time (the sample at the dose time is not fitted).
    TimeGreaterThan(f64),
}

/// One fit case: a dataset, a model, a weighting, start values and options per subject, and the
/// reference results of every fitted subject.
#[derive(Debug, Clone, PartialEq)]
pub struct FitCase {
    pub name: String,
    pub dataset: Dataset,
    /// Model id of `specs/models.md`, for example `pk1.oral_1`.
    pub model: String,
    /// Weighting id of `specs/fit.md` FIT-WGT-01, for example `inv_yhat2`.
    pub weighting: String,
    pub observations: Observations,
    /// Initial estimates by subject, then by parameter name (`v`, `k`, `ka`).
    pub initial_estimates: BTreeMap<String, BTreeMap<String, f64>>,
    pub options: FitCaseOptions,
    /// Subjects that have reference results (their codes, as in `expected`).
    pub subjects: Vec<String>,
    /// Subjects left out because the iteratively reweighted scheme has no fixed point for them.
    pub no_fixed_point: Vec<String>,
    /// Parameters held at a given value in the reference fits and not fitted (the duration `dur`
    /// of an infusion or of a zero-order input); the engine must be given the same values.
    pub fixed: BTreeMap<String, f64>,
    /// `(subject, name)` to value, for the subjects in `subjects`.
    pub expected: Table,
    pub note: String,
}

impl FitCase {
    /// The profile of `subject`, if it is in the dataset.
    pub fn profile(&self, subject: &str) -> Option<&Profile> {
        self.dataset
            .profiles
            .iter()
            .find(|p| p.subject.to_string() == subject)
    }

    /// The `(time, concentration)` pairs of `profile` that are fitted under [`Self::observations`].
    pub fn fitted_observations(&self, profile: &Profile) -> (Vec<f64>, Vec<f64>) {
        profile
            .time
            .iter()
            .zip(&profile.conc)
            .filter(|&(&t, _)| match self.observations {
                Observations::All => true,
                Observations::TimeGreaterThan(limit) => t > limit,
            })
            .map(|(&t, &c)| (t, c))
            .unzip()
    }
}

/// Names of the fit cases in `oracle/expected/fit/`, sorted.
pub fn list_fit_cases() -> Result<Vec<String>, OracleError> {
    names_in(&oracle_dir().join("expected").join("fit"))
}

/// Loads one fit case and checks that its files agree.
pub fn load_fit_case(name: &str) -> Result<FitCase, OracleError> {
    let root = oracle_dir();
    let dir = root.join("expected").join("fit");
    let options_path = dir.join(format!("{name}.options.json"));
    let options: FitOptions = json(&options_path)?;
    let here = options_path.display().to_string();
    let inconsistent = |message: String| OracleError::Inconsistent {
        path: here.clone(),
        message,
    };
    if options.case != name {
        return Err(inconsistent(format!(
            "case field is {:?}, file name says {name:?}",
            options.case
        )));
    }
    let observations = match (
        options.observations.rule.as_str(),
        options.observations.time,
    ) {
        ("all", _) => Observations::All,
        ("time_greater_than", Some(t)) => Observations::TimeGreaterThan(t),
        (other, _) => {
            return Err(inconsistent(format!("unknown observations rule {other:?}")));
        }
    };
    let dataset = load_dataset(&options.dataset, &root.join(&options.data_file))?;
    let expected_path = dir.join(format!("{name}.csv"));
    let expected = parse_expected(&expected_path.display().to_string(), &read(&expected_path)?)?;
    if expected.len() != options.n_values {
        return Err(inconsistent(format!(
            "options say {} values, the expected table has {}",
            options.n_values,
            expected.len()
        )));
    }
    for subject in options.subjects.iter().chain(&options.no_fixed_point) {
        if !dataset
            .profiles
            .iter()
            .any(|p| p.subject.to_string() == *subject)
        {
            return Err(inconsistent(format!(
                "subject {subject} is not in the dataset"
            )));
        }
        if !options.initial_estimates.contains_key(subject) {
            return Err(inconsistent(format!(
                "subject {subject} has no initial estimates"
            )));
        }
    }
    for subject in &options.subjects {
        if expected.get(subject, "wrss").is_none() {
            return Err(inconsistent(format!("no results for subject {subject}")));
        }
    }
    Ok(FitCase {
        name: name.to_string(),
        dataset,
        model: options.model,
        weighting: options.weighting,
        observations,
        initial_estimates: options.initial_estimates,
        options: options.fit_options,
        subjects: options.subjects,
        no_fixed_point: options.no_fixed_point,
        fixed: options.fixed,
        expected,
        note: options.note,
    })
}

/// One iterate of the Gauss-Newton trace of worked example F1 (`specs/fit.md` section 11).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GaussNewtonStep {
    pub iteration: usize,
    pub wrss: f64,
    pub v: f64,
    pub k: f64,
}

/// The iterates 0 to 5 of worked example F1 to full double precision (uniform weights, analytic
/// derivatives, full steps from V = 12, k = 0.15), from `oracle/expected/fit/gauss_newton_f1.csv`.
pub fn load_gauss_newton_f1() -> Result<Vec<GaussNewtonStep>, OracleError> {
    let path = oracle_dir()
        .join("expected")
        .join("fit")
        .join("gauss_newton_f1.csv");
    let shown = path.display().to_string();
    let text = read(&path)?;
    let bad = |line: usize, message: &str| OracleError::Csv {
        path: shown.clone(),
        line,
        message: message.to_string(),
    };
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, "iteration,wrss,v,k")) => {}
        _ => return Err(bad(1, "expected header iteration,wrss,v,k")),
    }
    let mut steps = Vec::new();
    for (i, line) in lines {
        let f: Vec<&str> = line.split(',').collect();
        let [it, wrss, v, k] = f.as_slice() else {
            return Err(bad(i + 1, "expected 4 fields"));
        };
        let num = |x: &str| {
            x.trim()
                .parse::<f64>()
                .map_err(|_| bad(i + 1, "not a number"))
        };
        steps.push(GaussNewtonStep {
            iteration: it
                .trim()
                .parse()
                .map_err(|_| bad(i + 1, "not an iteration number"))?,
            wrss: num(wrss)?,
            v: num(v)?,
            k: num(k)?,
        });
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_case_loads_with_its_grid() {
        let names = list_model_cases().unwrap();
        assert!(names.len() >= 20, "{names:?}");
        for name in names {
            let case = load_model_case(&name).unwrap();
            assert_eq!(case.times.len(), case.time_keys.len());
            for key in &case.time_keys {
                assert!(case.expected.get(key, "conc").is_some(), "{name} {key}");
                assert!(case.expected.get(key, "auc").is_some(), "{name} {key}");
            }
            assert!(case.expected.get("scalar", "auc_inf").is_some(), "{name}");
            assert!(
                case.parameters.contains_key("cl") ^ case.parameters.contains_key("k"),
                "{name}: exactly one of cl and k"
            );
        }
    }

    #[test]
    fn every_fit_case_loads_and_is_complete() {
        let names = list_fit_cases().unwrap();
        // 15 cases of T-009/T-018 (Theoph, Indometh, the five-point example) and 15 of T-029
        // (synthetic infusion, zero-order and lag profiles), five weightings each.
        assert_eq!(names.len(), 30, "{names:?}");
        for name in names {
            let case = load_fit_case(&name).unwrap();
            assert!(!case.subjects.is_empty(), "{name}");
            for s in &case.subjects {
                for key in ["wrss", "aic", "estimate.v", "se.v", "estimate.k"] {
                    assert!(case.expected.get(s, key).is_some(), "{name} {s} {key}");
                }
                let profile = case.profile(s).unwrap();
                let (t, y) = case.fitted_observations(profile);
                assert_eq!(t.len(), y.len());
                assert!(t.len() >= 5, "{name} {s}");
                let p = case.expected.get(s, "p").flatten().unwrap();
                assert_eq!(case.initial_estimates[s].len() as f64, p, "{name} {s}");
            }
        }
    }

    #[test]
    fn the_spec_example_reproduces_the_hand_computed_numbers() {
        // specs/fit.md F2 (uniform) and F3 (inv_y2): independent hand/script values to 7 digits.
        let f2 = load_fit_case("fit_spec_uniform").unwrap();
        let get = |c: &FitCase, n: &str| c.expected.get("1", n).flatten().unwrap();
        for (name, value) in [
            ("estimate.v", 9.9186407),
            ("estimate.k", 0.20405768),
            ("wrss", 0.19559014),
            ("se.v", 0.24785299),
            ("se.k", 0.01280633),
            ("aic", -4.1586697),
            ("sbc", -4.9397939),
            ("corr_obs_pred", 0.9970964),
            ("kappa_jacobian", 2.4971343),
            ("ci_lo.v", 9.1298619),
            ("planar_hi.k", 0.26003204),
            ("estimate.cl", 2.0239749),
            ("se.half_life", 0.2131790),
        ] {
            let x = get(&f2, name);
            assert!(
                (x - value).abs() <= 1e-7 * value.abs().max(1.0),
                "F2 {name}: {x} against {value}"
            );
        }
        let f3 = load_fit_case("fit_spec_inv_y2").unwrap();
        for (name, value) in [
            ("estimate.v", 10.10481932),
            ("estimate.k", 0.19590905),
            ("wrss", 0.005450726),
            ("aic", -22.0600324),
            ("ss_corrected", 1.3984729),
            ("estimate.cl", 1.9796255),
        ] {
            let x = get(&f3, name);
            assert!(
                (x - value).abs() <= 1e-7 * value.abs().max(1.0),
                "F3 {name}: {x} against {value}"
            );
        }
    }

    #[test]
    fn the_gauss_newton_reference_agrees_with_the_printed_iterations_of_f1() {
        // specs/fit.md F1 prints WRSS to 7 decimals and the parameters to 7 decimals: the exact
        // iterates must round to them (half a unit of the last printed place).
        let steps = load_gauss_newton_f1().unwrap();
        assert_eq!(steps.len(), 6);
        let printed = [
            (3.7383091, 12.0, 0.15),
            (0.4285909, 9.6163537, 0.2032666),
            (0.1957952, 9.9096786, 0.2040143),
            (0.1955901388, 9.9186464, 0.2040567),
            (0.1955901382, 9.9186411, 0.2040577),
        ];
        for (step, (wrss, v, k)) in steps.iter().zip(printed) {
            let half_unit = |x: f64| {
                if x == 0.1955901388 || x == 0.1955901382 {
                    0.5e-10
                } else {
                    0.5e-7
                }
            };
            assert!(
                (step.wrss - wrss).abs() <= half_unit(wrss) * 1.0001,
                "{step:?} against {wrss}"
            );
            assert!(
                (step.v - v).abs() <= 0.5e-7 * 1.0001,
                "{step:?} against {v}"
            );
            assert!(
                (step.k - k).abs() <= 0.5e-7 * 1.0001,
                "{step:?} against {k}"
            );
        }
        assert_eq!(steps[0].iteration, 0);
        assert_eq!(steps[5].iteration, 5);
    }

    #[test]
    fn the_model_cases_reproduce_the_hand_computed_numbers_of_the_spec() {
        let m2 = load_model_case("model_oral_1").unwrap();
        let get = |c: &ModelCase, g: &str, n: &str| c.expected.get(g, n).flatten().unwrap();
        for (key, name, value) in [
            ("t=0.5", "conc", 3.7288344790),
            ("t=1", "conc", 5.6356413988),
            ("t=2", "conc", 6.6873095350),
            ("t=6", "conc", 3.7339432467),
            ("t=12", "conc", 1.1338976135),
            ("t=6", "auc", 31.2063461577),
            ("scalar", "tmax_pred", 2.0117973905),
            ("scalar", "cmax_pred", 6.6874030498),
            ("scalar", "mrt", 6.0),
        ] {
            let x = get(&m2, key, name);
            assert!((x - value).abs() <= 1e-9 * value, "M2 {key} {name}: {x}");
        }
        // M5: the limit ka = k, and the neighbour at ka - k = 1e-6 (the spec's value for 1e-9,
        // 3.6787944006, is the cancellation-damaged one; the exact value is 3.6787944209).
        let eq = load_model_case("model_oral_1_ka_eq_k").unwrap();
        assert!((get(&eq, "t=5", "conc") - 3.6787944117).abs() < 1e-9);
        let near = load_model_case("model_oral_1_ka_near_k_1e6").unwrap();
        assert!((get(&near, "t=5", "conc") - 3.6788036087).abs() < 1e-9);
        let near9 = load_model_case("model_oral_1_ka_near_k_1e9").unwrap();
        assert!((get(&near9, "t=5", "conc") - 3.6787944209).abs() < 1e-9);
    }
}
