#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Fitting oracle (task T-009): `caladrius-fit` against reference weighted least-squares fits
//! (`oracle/expected/fit/`, produced by `oracle/scripts/fit_wls.R` with `stats::nls` and
//! `minpack.lm::nlsLM`, statistics from the formulas of `specs/fit.md` section 7), on Theoph
//! (oral, first-order absorption, 12 subjects), Indometh (IV bolus, 6 subjects) and the five-point
//! example of `specs/fit.md` section 11, for the five weighting schemes.
//!
//! Written before the engine exists (task T-009): the file does not compile until `caladrius-fit`
//! provides the API below, and its tests are not ignored.
//!
//! # API the engine must provide (all in the crate root, `caladrius_fit`)
//!
//! Close to the NCA style: one input value, one `run`, results by name. The engine agent may add
//! fields, variants and methods, but must keep these names and meanings or change this file in
//! agreement with the orchestrator (the tolerances are never to be touched). Inputs and results are
//! data and should derive `Serialize`/`Deserialize` (golden rule 4).
//!
//! ```ignore
//! /// FIT-WGT-01. `from_id` / `id`: "uniform", "inv_y", "inv_y2", "inv_yhat", "inv_yhat2".
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum Weighting { Uniform, InvY, InvY2, InvYhat, InvYhat2 }
//! impl Weighting { pub fn from_id(id: &str) -> Option<Weighting>; pub fn id(&self) -> &'static str; }
//! /// FIT-JAC-01, FIT-JAC-02.
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum Derivatives { ForwardDifference, Analytic }
//! /// FIT-CNV-01.
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum Criterion { RelativeDecrease, RelativeOffset }
//! pub struct FitOptions {
//!     pub derivatives: Derivatives,   // default ForwardDifference
//!     pub increment: f64,             // default 0.001 (relative, FIT-JAC-01)
//!     pub criterion: Criterion,       // default RelativeDecrease
//!     pub convergence: f64,           // default 0.0001
//!     pub max_iterations: usize,      // default 50
//!     pub confidence_level: f64,      // default 0.95
//!     pub n_curve: usize,             // default 1000
//! }
//! impl Default for FitOptions { /* the defaults above (AGENTS.md section 6) */ }
//! pub struct FitInput {
//!     pub model: caladrius_models::ModelId,   // IvBolus (v, k) and Oral1 (v, k, ka) are exercised here
//!     pub dose: f64,
//!     pub time: Vec<f64>,                     // observations to fit (the caller has already left out
//!     pub conc: Vec<f64>,                     // the ones it does not want fitted)
//!     pub weighting: Weighting,
//!     /// Initial estimates by parameter name; the fitted parameters are exactly the ones named.
//!     /// The fit uses the (v, k[, ka]) parameterisation of the oracle.
//!     pub initial: BTreeMap<String, f64>,
//!     pub options: FitOptions,
//! }
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum FitStatus { Converged, MaxIterations, NoDecrease, Singular, NonFinite, AtBound } // FIT-CNV-03
//! #[derive(Debug, Clone, PartialEq)]
//! pub struct TraceRow { pub iteration: usize, pub wrss: f64, pub estimates: Vec<f64> } // FIT-OUT-11, parameter order of the model
//! pub struct FitResult { /* private */ }
//! impl FitResult {
//!     pub fn status(&self) -> FitStatus;
//!     /// Results by name, `None` when not calculated (no degrees of freedom, singular...). Names:
//!     /// n, p, df; estimate.<par>, se.<par>, cv_percent.<par>, ci_lo.<par>, ci_hi.<par>,
//!     /// planar_lo.<par>, planar_hi.<par> for each parameter (v, k, ka); covariance.<a>.<b> (a <= b
//!     /// in parameter order), correlation.<a>.<b> (a < b), eigenvalue.<i> (decreasing, from 1),
//!     /// condition_number, kappa_jacobian; wrss, s, ss_weighted, ss_corrected, corr_obs_pred, aic,
//!     /// sbc; secondary parameters estimate./se./cv_percent./ci_lo./ci_hi. of cl, half_life, auc_inf.
//!     /// All as defined in specs/fit.md section 7 and in `oracle/expected/fit/*.options.json`.
//!     pub fn get(&self, name: &str) -> Option<f64>;
//!     /// One row per iteration including iteration 0 (FIT-OUT-11).
//!     pub fn trace(&self) -> &[TraceRow];
//! }
//! pub struct FitError { /* Display, std::error::Error; says what to fix */ }
//! pub fn run(input: &FitInput) -> Result<FitResult, FitError>;
//! ```
//!
//! What the reference runs use (the options file of each case says so): analytic derivatives,
//! criterion `relative_decrease` with threshold 1e-10 and at most 200 iterations (FIT-CNV-04),
//! 95 % intervals, the initial estimates of the options file, and the observations after time 0 for
//! Theoph. Predicted-value weights are the iteratively reweighted fixed point of FIT-WGT-03.
//!
//! Tolerances: estimates and statistics `FIT_PARAMETERS` (1e-4 relative), the residual sum of
//! squares `FIT_WEIGHTED_SS` (1e-6). Three quantities need a derived comparison, stated here and
//! not a looser constant: confidence limits can lie close to zero while the estimate does not
//! (`estimate - c * SE`), so their allowed error is the allowed error of the estimate plus that of
//! `c * SE`; AIC and SBC are checked as `N ln(WRSS) + k` from the engine's own WRSS (formula) and
//! within `N * 1e-6` of the oracle (the consequence of the WRSS tolerance); both are explained at
//! their tests.

use std::collections::BTreeMap;

use caladrius_fit::{
    Criterion, Derivatives, FitInput, FitOptions, FitResult, FitStatus, Weighting, run,
};
use caladrius_models::ModelId;
use caladrius_testkit::{FitCase, Tolerance, load_fit_case, load_gauss_newton_f1};

fn model_of(case: &FitCase) -> ModelId {
    ModelId::from_id(&case.model)
        .unwrap_or_else(|| panic!("{}: unknown model id {:?}", case.name, case.model))
}

fn options_of(case: &FitCase) -> FitOptions {
    let o = &case.options;
    assert_eq!(o.derivatives, "analytic", "{}", case.name);
    assert_eq!(o.criterion, "relative_decrease", "{}", case.name);
    FitOptions {
        derivatives: Derivatives::Analytic,
        criterion: Criterion::RelativeDecrease,
        convergence: o.convergence,
        max_iterations: o.max_iterations,
        confidence_level: o.confidence_level,
        ..FitOptions::default()
    }
}

fn input_of(case: &FitCase, subject: &str) -> FitInput {
    let profile = case
        .profile(subject)
        .unwrap_or_else(|| panic!("{}: no subject {subject}", case.name));
    let (time, conc) = case.fitted_observations(profile);
    FitInput {
        model: model_of(case),
        dose: profile.dose,
        time,
        conc,
        weighting: Weighting::from_id(&case.weighting)
            .unwrap_or_else(|| panic!("{}: unknown weighting {:?}", case.name, case.weighting)),
        initial: case.initial_estimates[subject].clone(),
        options: options_of(case),
    }
}

fn expected(case: &FitCase, subject: &str, name: &str) -> f64 {
    case.expected
        .get(subject, name)
        .flatten()
        .unwrap_or_else(|| panic!("{}: no expected {name} for subject {subject}", case.name))
}

fn fit(case: &FitCase, subject: &str) -> FitResult {
    run(&input_of(case, subject)).unwrap_or_else(|e| {
        panic!(
            "{} subject {subject}: the engine refuses a valid fit: {e}",
            case.name
        )
    })
}

/// Which test group an expected quantity belongs to.
fn group_of(name: &str) -> &'static str {
    let base = name.split('.').next().unwrap_or(name);
    let tail = name.rsplit('.').next().unwrap_or(name);
    if matches!(tail, "cl" | "half_life" | "auc_inf") {
        return "secondary";
    }
    match base {
        "estimate" => "estimates",
        "wrss" => "wrss",
        "ci_lo" | "ci_hi" | "planar_lo" | "planar_hi" => "intervals",
        "aic" | "sbc" => "information_criteria",
        _ => "statistics",
    }
}

fn check_group(case_name: &str, group: &str) {
    let case = load_fit_case(case_name).expect("the fit oracle case loads");
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for subject in &case.subjects {
        let result = fit(&case, subject);
        for (s, name, value) in case.expected.iter() {
            if s != subject || group_of(name) != group {
                continue;
            }
            let value = value.expect("the oracle has no NA fit value");
            checked += 1;
            let actual = result.get(name);
            let Some(actual) = actual else {
                problems.push(format!(
                    "[{subject}] {name}: expected {value:e}, not calculated"
                ));
                continue;
            };
            let (ok, tolerance) = match group {
                "wrss" => (
                    Tolerance::FIT_WEIGHTED_SS.accepts(actual, value),
                    "1e-6 relative",
                ),
                "intervals" => (
                    interval_ok(&case, subject, name, actual, value),
                    "limit tolerance",
                ),
                "information_criteria" => (
                    information_criterion_ok(&case, subject, &result, name, actual, value),
                    "N * 1e-6",
                ),
                _ => (
                    Tolerance::FIT_PARAMETERS.accepts(actual, value),
                    "1e-4 relative",
                ),
            };
            if !ok {
                problems.push(format!(
                    "[{subject}] {name}: expected {value:e}, got {actual:e} ({tolerance})"
                ));
            }
        }
    }
    assert!(
        checked > 0,
        "{case_name}: no expected value in group {group}"
    );
    assert!(
        problems.is_empty(),
        "{case_name} {group}: {} of {checked} values outside tolerance\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}

/// A confidence limit is `estimate -/+ c * SE`: its allowed error is that of the estimate plus that
/// of `c * SE` (the limit itself may be near zero when the interval is wide).
fn interval_ok(case: &FitCase, subject: &str, name: &str, actual: f64, value: f64) -> bool {
    let parameter = name.split_once('.').map_or(name, |(_, p)| p);
    let estimate = expected(case, subject, &format!("estimate.{parameter}"));
    let allowed = Tolerance::FIT_PARAMETERS.allowed_difference(estimate)
        + Tolerance::FIT_PARAMETERS.allowed_difference(value - estimate);
    actual.is_finite() && (actual - value).abs() <= allowed
}

/// AIC = N ln(WRSS) + 2 P and SBC = N ln(WRSS) + P ln N. The value is a logarithm of the residual
/// sum of squares and can be close to zero, where a relative tolerance means nothing. So: the
/// engine's value must follow the formula from its own WRSS (to rounding), and may differ from the
/// oracle by N times the relative tolerance allowed on WRSS, which is what a WRSS error of
/// `FIT_WEIGHTED_SS` does to the logarithm.
fn information_criterion_ok(
    case: &FitCase,
    subject: &str,
    result: &FitResult,
    name: &str,
    actual: f64,
    value: f64,
) -> bool {
    let n = expected(case, subject, "n");
    let p = expected(case, subject, "p");
    let Some(wrss) = result.get("wrss") else {
        return false;
    };
    let penalty = if name == "aic" { 2.0 * p } else { p * n.ln() };
    let formula = n * wrss.ln() + penalty;
    let follows_formula = (actual - formula).abs() <= 1e-9 * formula.abs().max(1.0);
    let rel = match Tolerance::FIT_WEIGHTED_SS {
        Tolerance::Relative { rel, .. } => rel,
        Tolerance::DisplayedDecimals { .. } => 0.0,
    };
    follows_formula && (actual - value).abs() <= n * rel + 1e-9
}

macro_rules! oracle_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::{check_group, check_status};
                const NAME: &str = concat!("fit_", stringify!($case));
                #[test]
                fn converges() { check_status(NAME); }
                #[test]
                fn estimates() { check_group(NAME, "estimates"); }
                #[test]
                fn residual_sum_of_squares() { check_group(NAME, "wrss"); }
                #[test]
                fn statistics() { check_group(NAME, "statistics"); }
                #[test]
                fn confidence_limits() { check_group(NAME, "intervals"); }
                #[test]
                fn information_criteria() { check_group(NAME, "information_criteria"); }
                #[test]
                fn secondary_parameters() { check_group(NAME, "secondary"); }
            }
        )*
    };
}

fn check_status(case_name: &str) {
    let case = load_fit_case(case_name).expect("the fit oracle case loads");
    let mut problems = Vec::new();
    for subject in &case.subjects {
        let status = fit(&case, subject).status();
        if status != FitStatus::Converged {
            problems.push(format!("subject {subject}: status {status:?}"));
        }
    }
    assert!(problems.is_empty(), "{case_name}: {}", problems.join("; "));
    // Subjects for which the iteratively reweighted scheme has no fixed point (the iteration
    // cycles): the engine must not report a converged fit for them.
    for subject in &case.no_fixed_point {
        match run(&input_of(&case, subject)) {
            Err(_) => {}
            Ok(result) => assert_ne!(
                result.status(),
                FitStatus::Converged,
                "{case_name} subject {subject}: no fixed point exists, yet the fit reports convergence"
            ),
        }
    }
}

oracle_tests! {
    theoph_uniform, theoph_inv_y, theoph_inv_y2, theoph_inv_yhat, theoph_inv_yhat2,
    indometh_uniform, indometh_inv_y, indometh_inv_y2, indometh_inv_yhat, indometh_inv_yhat2,
    spec_uniform, spec_inv_y, spec_inv_y2, spec_inv_yhat, spec_inv_yhat2,
}

/// Guards the guard: every fit case of the oracle is run by a test above.
#[test]
fn every_fit_case_has_tests() {
    let covered = [
        "theoph_uniform",
        "theoph_inv_y",
        "theoph_inv_y2",
        "theoph_inv_yhat",
        "theoph_inv_yhat2",
        "indometh_uniform",
        "indometh_inv_y",
        "indometh_inv_y2",
        "indometh_inv_yhat",
        "indometh_inv_yhat2",
        "spec_uniform",
        "spec_inv_y",
        "spec_inv_y2",
        "spec_inv_yhat",
        "spec_inv_yhat2",
    ];
    // The synthetic infusion, zero-order and lag cases of task T-029 are run by
    // `oracle_fit_models.rs`, which has its own guard.
    let names: Vec<String> = caladrius_testkit::list_fit_cases()
        .unwrap()
        .into_iter()
        .filter(|n| {
            !["fit_infusion_", "fit_zero_order_", "fit_lag_"]
                .iter()
                .any(|p| n.starts_with(p))
        })
        .collect();
    assert_eq!(names.len(), covered.len(), "{names:?}");
    for name in names {
        let short = name.strip_prefix("fit_").unwrap();
        assert!(covered.contains(&short), "{name} has no test");
    }
}

// ---------------------------------------------------------------- specs/fit.md section 11

fn spec_input(weighting: Weighting) -> FitInput {
    FitInput {
        model: ModelId::IvBolus,
        dose: 100.0,
        time: vec![0.5, 1.0, 2.0, 4.0, 8.0],
        conc: vec![9.31, 7.92, 6.85, 4.31, 2.11],
        weighting,
        initial: BTreeMap::from([("v".to_string(), 12.0), ("k".to_string(), 0.15)]),
        // The worked examples are stated with the assumed reference defaults, which are the
        // preset `reference_conventions` since T-040 (Q-014 option 2, D-04).
        options: FitOptions::reference_conventions(),
    }
}

/// The assumed defaults of AGENTS.md section 6 (the preset `reference_conventions` since T-040)
/// and FIT-VOC-01.
#[test]
fn default_options_are_the_documented_ones() {
    let o = FitOptions::reference_conventions();
    assert_eq!(o.derivatives, Derivatives::ForwardDifference);
    assert_eq!(o.increment, 0.001);
    assert_eq!(o.criterion, Criterion::RelativeDecrease);
    assert_eq!(o.convergence, 0.0001);
    assert_eq!(o.max_iterations, 50);
    assert_eq!(o.confidence_level, 0.95);
    assert_eq!(o.n_curve, 1000);
    assert_eq!(Weighting::from_id("uniform"), Some(Weighting::Uniform));
    assert_eq!(
        Weighting::from_id("inv_yhat2").map(|w| w.id()),
        Some("inv_yhat2")
    );
    assert_eq!(Weighting::from_id("1/y"), None);
}

/// Worked example F1: pure Gauss-Newton iterations (full steps, no halving) with analytic
/// derivatives from (12, 0.15), stopped by the default criterion 1e-4 after iteration 4. The
/// iterates are compared with the double-precision reference of the oracle (task T-018).
#[test]
fn gauss_newton_trace_of_worked_example_f1() {
    let mut input = spec_input(Weighting::Uniform);
    input.options.derivatives = Derivatives::Analytic;
    let result = run(&input).unwrap();
    assert_eq!(result.status(), FitStatus::Converged);
    let trace = result.trace();
    assert_eq!(trace.len(), 5, "iterations 0 to 4: {trace:?}");
    // The reference iterates are those of `oracle/expected/fit/gauss_newton_f1.csv`, computed in
    // double precision by the oracle script; the specification prints them to 7 decimals, and
    // rounding to 7 decimals alone can move a value by 5e-8 relative, which a printed value cannot
    // be held to. Two implementations of the same recurrence in double precision agree to
    // rounding, far below 1e-9 relative; 1e-9 is what is demanded.
    let reference = load_gauss_newton_f1().unwrap();
    for (i, (row, expected)) in trace.iter().zip(&reference).enumerate() {
        assert_eq!(row.iteration, i);
        assert_eq!(expected.iteration, i);
        assert!(
            (row.wrss - expected.wrss).abs() <= 1e-9 * expected.wrss,
            "iteration {i}: WRSS {} against {}",
            row.wrss,
            expected.wrss
        );
        for (a, e) in row.estimates.iter().zip([expected.v, expected.k]) {
            assert!((a - e).abs() <= 1e-9 * e, "iteration {i}: {a} against {e}");
        }
    }
}

/// Worked example F2 with the default forward-difference derivatives (increment 0.001 relative):
/// the standard errors carry the relative error of the increment (FIT-JAC-03), so they differ from
/// the analytic ones by about 1e-3, and the reader's values for that Jacobian are
/// SE(V) = 0.24814908 and SE(k) = 0.01281461 (re-computed independently for this oracle).
#[test]
fn forward_difference_standard_errors_of_worked_example_f2() {
    let mut input = spec_input(Weighting::Uniform);
    input.options.convergence = 1e-10;
    input.options.max_iterations = 200;
    let result = run(&input).unwrap();
    assert_eq!(result.status(), FitStatus::Converged);
    for (name, value) in [
        ("estimate.v", 9.91864072695836),
        ("estimate.k", 0.204057684345212),
        ("se.v", 0.24814908),
        ("se.k", 0.01281461),
    ] {
        let x = result.get(name).unwrap();
        assert!(
            Tolerance::FIT_PARAMETERS.accepts(x, value),
            "{name}: {x} against {value}"
        );
    }
}

// ---------------------------------------------------------------- errors (FIT-ERR-*, golden rule 6)

fn error_text(input: FitInput) -> String {
    match run(&input) {
        Ok(_) => panic!("the engine accepted an invalid fit"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn input_errors_say_what_to_fix() {
    // Different lengths.
    let mut i = spec_input(Weighting::Uniform);
    i.conc.pop();
    error_text(i);
    // Fewer observations than parameters (FIT-ERR-01).
    let mut i = spec_input(Weighting::Uniform);
    i.time.truncate(1);
    i.conc.truncate(1);
    error_text(i);
    // NaN and infinite values.
    let mut i = spec_input(Weighting::Uniform);
    i.conc[2] = f64::NAN;
    error_text(i);
    let mut i = spec_input(Weighting::Uniform);
    i.time[1] = f64::INFINITY;
    error_text(i);
    // A dose that is not positive.
    let mut i = spec_input(Weighting::Uniform);
    i.dose = 0.0;
    error_text(i);
    // An initial estimate that is not positive, a missing one, an unknown one.
    let mut i = spec_input(Weighting::Uniform);
    i.initial.insert("v".to_string(), -1.0);
    assert!(error_text(i).contains('v'));
    let mut i = spec_input(Weighting::Uniform);
    i.initial.remove("k");
    assert!(error_text(i).contains('k'));
    let mut i = spec_input(Weighting::Uniform);
    i.initial.insert("ka".to_string(), 1.0);
    assert!(error_text(i).contains("ka"));
}

/// FIT-WGT-02: a weight 1/y or 1/y^2 needs y > 0; the error names the observation.
#[test]
fn observed_value_weights_refuse_zero_and_negative_concentrations() {
    for weighting in [Weighting::InvY, Weighting::InvY2] {
        for bad in [0.0, -0.5] {
            let mut i = spec_input(weighting);
            i.conc[3] = bad;
            let text = error_text(i);
            assert!(
                text.contains("4") || text.contains("zero") || text.contains("weight"),
                "the error should name the observation or the weighting: {text}"
            );
        }
    }
    // Uniform weights accept them (the fit may be poor but runs).
    let mut i = spec_input(Weighting::Uniform);
    i.conc[4] = 0.0;
    assert!(run(&i).is_ok());
}

/// FIT-ERR-02: with N = P the fit runs, and everything that needs degrees of freedom is not
/// calculated rather than NaN or infinite.
#[test]
fn no_degrees_of_freedom_gives_no_statistics() {
    let mut i = spec_input(Weighting::Uniform);
    i.time.truncate(2);
    i.conc.truncate(2);
    i.options.derivatives = Derivatives::Analytic;
    i.options.convergence = 1e-10;
    i.options.max_iterations = 200;
    let result = run(&i).unwrap();
    assert_eq!(result.get("df"), Some(0.0));
    assert!(result.get("estimate.v").is_some());
    for name in ["s", "se.v", "se.k", "ci_lo.v", "aic", "cv_percent.k"] {
        assert_eq!(result.get(name), None, "{name} needs degrees of freedom");
    }
    assert!(result.get("wrss").unwrap() < 1e-12);
}

/// FIT-CNV-02/03: reaching the iteration limit is reported as such, with the best iterate and a
/// trace, never as convergence and never as a panic.
#[test]
fn the_iteration_limit_is_reported() {
    let mut i = spec_input(Weighting::Uniform);
    i.options.derivatives = Derivatives::Analytic;
    i.options.max_iterations = 1;
    i.options.convergence = 1e-12;
    match run(&i) {
        Ok(result) => {
            assert_eq!(result.status(), FitStatus::MaxIterations);
            assert!(!result.trace().is_empty());
        }
        Err(e) => panic!("a fit that hits the iteration limit returns its best iterate: {e}"),
    }
}

/// FIT-ALG-05: the same input gives the same output, bit for bit.
#[test]
fn a_fit_is_deterministic() {
    let i = spec_input(Weighting::InvYhat);
    let a = run(&i).unwrap();
    let b = run(&i).unwrap();
    for name in ["estimate.v", "estimate.k", "wrss", "se.v", "aic"] {
        assert_eq!(a.get(name).map(f64::to_bits), b.get(name).map(f64::to_bits));
    }
    assert_eq!(a.trace().len(), b.trace().len());
}
