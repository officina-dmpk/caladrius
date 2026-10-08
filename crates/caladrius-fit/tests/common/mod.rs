#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
#![allow(dead_code)] // each test file uses a part of these helpers

//! Helpers shared by the oracle test files added by task T-029 (`oracle_fit_models.rs`,
//! `oracle_fit_tight.rs`). The comparison rules are those of `oracle_fit.rs` (written by T-009);
//! they are repeated here, not loosened, and the tolerances come from `caladrius-testkit`.

use caladrius_fit::{
    Criterion, Derivatives, FitError, FitInput, FitOptions, FitResult, FitStatus, Weighting, run,
};
use caladrius_models::ModelId;
use caladrius_testkit::{FitCase, Tolerance, load_fit_case};

/// The input of one subject of a reference fit: the model, the observations that were fitted, the
/// initial estimates and the fixed parameters of the options file, and the reference run's
/// convergence settings. Only the way the partial derivatives are formed differs between callers.
pub fn input_of(case: &FitCase, subject: &str, derivatives: Derivatives) -> FitInput {
    let o = &case.options;
    assert_eq!(o.criterion, "relative_decrease", "{}", case.name);
    let profile = case
        .profile(subject)
        .unwrap_or_else(|| panic!("{}: no subject {subject}", case.name));
    let (time, conc) = case.fitted_observations(profile);
    FitInput {
        model: ModelId::from_id(&case.model)
            .unwrap_or_else(|| panic!("{}: unknown model id {:?}", case.name, case.model)),
        dose: profile.dose,
        time,
        conc,
        weighting: Weighting::from_id(&case.weighting)
            .unwrap_or_else(|| panic!("{}: unknown weighting {:?}", case.name, case.weighting)),
        initial: case.initial_estimates[subject].clone(),
        options: FitOptions {
            derivatives,
            criterion: Criterion::RelativeDecrease,
            convergence: o.convergence,
            max_iterations: o.max_iterations,
            confidence_level: o.confidence_level,
            fixed: case.fixed.clone(),
            ..FitOptions::default()
        },
    }
}

pub fn expected(case: &FitCase, subject: &str, name: &str) -> f64 {
    case.expected
        .get(subject, name)
        .flatten()
        .unwrap_or_else(|| panic!("{}: no expected {name} for subject {subject}", case.name))
}

/// How a fit is run: the engine's own `run`, or a variant of it (see `oracle_fit_models.rs`).
pub type Runner = fn(&FitInput) -> Result<FitResult, FitError>;

/// The engine as shipped.
pub fn engine(input: &FitInput) -> Result<FitResult, FitError> {
    run(input)
}

pub fn fit(case: &FitCase, subject: &str, derivatives: Derivatives, runner: Runner) -> FitResult {
    runner(&input_of(case, subject, derivatives)).unwrap_or_else(|e| {
        panic!(
            "{} subject {subject}: the engine refuses a valid fit ({derivatives:?} derivatives): {e}",
            case.name
        )
    })
}

/// Which test group an expected quantity belongs to (the groups of `oracle_fit.rs`).
pub fn group_of(name: &str) -> &'static str {
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

/// A confidence limit is `estimate -/+ c * SE`: its allowed error is that of the estimate plus that
/// of `c * SE` (the limit itself may be near zero when the interval is wide).
pub fn interval_ok(
    case: &FitCase,
    subject: &str,
    name: &str,
    actual: f64,
    value: f64,
    tolerance: Tolerance,
) -> bool {
    let parameter = name.split_once('.').map_or(name, |(_, p)| p);
    let estimate = expected(case, subject, &format!("estimate.{parameter}"));
    let allowed =
        tolerance.allowed_difference(estimate) + tolerance.allowed_difference(value - estimate);
    actual.is_finite() && (actual - value).abs() <= allowed
}

/// AIC = N ln(WRSS) + 2 P and SBC = N ln(WRSS) + P ln N: the engine's value must follow the
/// formula from its own WRSS (to rounding) and may differ from the oracle by N times the relative
/// tolerance allowed on WRSS (the consequence of the WRSS tolerance; see `oracle_fit.rs`).
pub fn information_criterion_ok(
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

/// Compares one group of expected quantities of every subject of a case with the engine's, using
/// `derivatives` to form the partial derivatives. Fails with every value outside tolerance listed.
pub fn check_group(case_name: &str, group: &str, derivatives: Derivatives, runner: Runner) {
    check_group_with(
        case_name,
        group,
        derivatives,
        runner,
        Tolerance::FIT_PARAMETERS,
    );
}

/// As [`check_group`], with the tolerance for the estimates, the statistics, the secondary
/// parameters and the confidence limits given (the weighted sum of squares keeps
/// `FIT_WEIGHTED_SS`, the information criteria their derived rule).
pub fn check_group_with(
    case_name: &str,
    group: &str,
    derivatives: Derivatives,
    runner: Runner,
    parameters: Tolerance,
) {
    let case = load_fit_case(case_name).expect("the fit oracle case loads");
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for subject in &case.subjects {
        let result = fit(&case, subject, derivatives, runner);
        for (s, name, value) in case.expected.iter() {
            if s != subject || group_of(name) != group {
                continue;
            }
            let value = value.expect("the oracle has no NA fit value");
            checked += 1;
            let Some(actual) = result.get(name) else {
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
                    interval_ok(&case, subject, name, actual, value, parameters),
                    "limit tolerance",
                ),
                "information_criteria" => (
                    information_criterion_ok(&case, subject, &result, name, actual, value),
                    "N * 1e-6",
                ),
                _ => (parameters.accepts(actual, value), "parameter tolerance"),
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

/// Every subject of the case converges (FIT-CNV-03).
pub fn check_status(case_name: &str, derivatives: Derivatives, runner: Runner) {
    let case = load_fit_case(case_name).expect("the fit oracle case loads");
    let mut problems = Vec::new();
    for subject in &case.subjects {
        let status = fit(&case, subject, derivatives, runner).status();
        if status != FitStatus::Converged {
            problems.push(format!("subject {subject}: status {status:?}"));
        }
    }
    assert!(problems.is_empty(), "{case_name}: {}", problems.join("; "));
}
