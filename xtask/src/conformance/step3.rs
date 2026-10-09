//! The model and fit cases of the conformance table (`oracle/expected/models/`, its two-compartment
//! subdirectory `pk2/`, and `oracle/expected/fit/`), compared the way
//! `crates/caladrius-models/tests/oracle_models.rs`, `oracle_models_2c.rs` and
//! `crates/caladrius-fit/tests/oracle_fit.rs` do, with the tolerances of `caladrius-testkit`.
//!
//! A model case counts one row per expected value, grouped by quantity (`conc`, `auc`, `aumc`, then
//! each secondary parameter). A two-compartment derivative case counts one row per expected
//! partial derivative, grouped by parameter (`d_cl`, ...), against the closed-form Jacobian. The
//! two-compartment error suite counts one row per input the engine must refuse, grouped by the
//! oracle's groups: validated when the engine refuses it with a message holding the expected
//! words. A model id the engine does not know is an error line of its case, with nothing
//! validated, not an abort of the run. A fit case counts one row per expected value of every fitted
//! subject, grouped as the oracle tests group them, plus one `status` row per subject: converged
//! for the fitted subjects, not reported as converged for the subjects with no fixed point.

use std::collections::BTreeMap;

use caladrius_fit::{
    Criterion, Derivatives, FitInput, FitOptions, FitResult, FitStatus, Weighting, run as run_fit,
};
use caladrius_models::{
    Derivatives as ModelDerivatives, ModelId, ModelInput, ModelOutput, jacobian, run as run_model,
};
use caladrius_testkit::{
    FitCase, ModelCase, Pk2ErrorCase, Pk2Kind, Tolerance, list_fit_cases, list_model_cases,
    list_pk2_cases, load_fit_case, load_model_case, load_pk2_case, load_pk2_errors,
};

use super::{CaseReport, Count, Kind};
use crate::error::{Result, XtaskError};

fn oracle_error(e: caladrius_testkit::OracleError) -> XtaskError {
    XtaskError::new(e.to_string())
}

// ---------------------------------------------------------------- models

/// The reports of every model case: the one-compartment cases, then the two-compartment value and
/// derivative cases, each in the order of the file names, then the two-compartment error suite.
pub(super) fn model_reports() -> Result<Vec<CaseReport>> {
    let mut out = Vec::new();
    for name in list_model_cases().map_err(oracle_error)? {
        let case = load_model_case(&name).map_err(oracle_error)?;
        out.push(evaluate_model(&case));
    }
    for name in list_pk2_cases().map_err(oracle_error)? {
        let loaded = load_pk2_case(&name).map_err(oracle_error)?;
        out.push(match loaded.kind {
            Pk2Kind::Values => evaluate_model(&loaded.case),
            Pk2Kind::Derivatives => evaluate_derivatives(&loaded.case),
        });
    }
    out.push(evaluate_errors(
        PK2_ERRORS_CASE,
        &load_pk2_errors().map_err(oracle_error)?,
    ));
    Ok(out)
}

/// Name of the two-compartment error suite in the table (its file name).
const PK2_ERRORS_CASE: &str = caladrius_testkit::pk2::PK2_ERRORS;

/// The grid quantities of a model case, reported first and in this order.
const GRID_QUANTITIES: [&str; 3] = ["conc", "auc", "aumc"];

/// The value the engine gives for one expected row of a model case.
fn model_actual(case: &ModelCase, output: &ModelOutput, group: &str, name: &str) -> Option<f64> {
    if group == "scalar" {
        return output.get(name);
    }
    let index = case.time_keys.iter().position(|k| k == group)?;
    match name {
        "conc" => output.conc().get(index).copied(),
        "auc" => output.auc().get(index).copied(),
        "aumc" => output.aumc().get(index).copied(),
        _ => None,
    }
}

/// Counts per quantity: the grid quantities first, then the others in name order.
fn ordered(mut counts: BTreeMap<String, Count>) -> Vec<(String, Count)> {
    let mut parameters: Vec<(String, Count)> = Vec::new();
    for first in GRID_QUANTITIES {
        if let Some(c) = counts.remove(first) {
            parameters.push((first.to_owned(), c));
        }
    }
    parameters.extend(counts);
    parameters
}

/// Every expected row of `case` counted, validated when `ok` says so for its value.
fn count_rows(
    case: &ModelCase,
    mut ok: impl FnMut(&str, &str, Option<f64>) -> bool,
) -> Vec<(String, Count)> {
    let mut counts: BTreeMap<String, Count> = BTreeMap::new();
    for (group, name, expected) in case.expected.iter() {
        let count = counts.entry(name.to_owned()).or_default();
        count.expected += 1;
        count.validated += usize::from(ok(group, name, expected));
    }
    ordered(counts)
}

/// The case as a report with nothing validated and one error line (a model the engine cannot run).
fn not_run(case: &ModelCase, error: String) -> CaseReport {
    CaseReport {
        kind: Kind::Model,
        name: case.name.clone(),
        details: vec![case.model.clone()],
        parameters: count_rows(case, |_, _, _| false),
        errors: vec![error],
    }
}

fn unknown_model(case: &ModelCase) -> String {
    format!(
        "unknown model id {:?}: the engine has no such model",
        case.model
    )
}

fn model_input(case: &ModelCase, model: ModelId) -> ModelInput {
    ModelInput {
        model,
        dose: case.dose,
        params: case.parameters.clone(),
        times: case.times.clone(),
    }
}

fn evaluate_model(case: &ModelCase) -> CaseReport {
    let Some(model) = ModelId::from_id(&case.model) else {
        return not_run(case, unknown_model(case));
    };
    let output = match run_model(&model_input(case, model)) {
        Ok(o) => o,
        Err(e) => return not_run(case, e.to_string()),
    };
    let parameters = count_rows(case, |group, name, expected| {
        let actual = model_actual(case, &output, group, name);
        match (expected, actual) {
            (Some(e), Some(a)) => Tolerance::MODEL_VALUES.accepts(a, e),
            // Expected not available: the model must run and not have the quantity.
            (None, None) => true,
            _ => false,
        }
    });
    CaseReport {
        kind: Kind::Model,
        name: case.name.clone(),
        details: vec![case.model.clone()],
        parameters,
        errors: Vec::new(),
    }
}

/// A two-compartment derivative case: `d_<parameter>` at `t=<time>` against the closed-form
/// Jacobian (`Derivatives::Analytic`, which must not fall back to differences).
fn evaluate_derivatives(case: &ModelCase) -> CaseReport {
    let Some(model) = ModelId::from_id(&case.model) else {
        return not_run(case, unknown_model(case));
    };
    let jac = match jacobian(&model_input(case, model), ModelDerivatives::Analytic) {
        Ok(j) => j,
        Err(e) => return not_run(case, e.to_string()),
    };
    if jac.method != ModelDerivatives::Analytic {
        return not_run(
            case,
            "the engine has no closed-form derivatives for this model (forward differences instead)"
                .to_owned(),
        );
    }
    let parameters = count_rows(case, |group, name, expected| {
        let column = name
            .strip_prefix("d_")
            .and_then(|p| jac.parameters.iter().position(|q| q == p))
            .and_then(|j| jac.columns.get(j));
        let index = case.time_keys.iter().position(|k| k == group);
        let actual = column.zip(index).and_then(|(c, i)| c.get(i)).copied();
        match (expected, actual) {
            (Some(e), Some(a)) => Tolerance::MODEL_DERIVATIVES.accepts(a, e),
            _ => false,
        }
    });
    CaseReport {
        kind: Kind::Model,
        name: case.name.clone(),
        details: vec![case.model.clone()],
        parameters,
        errors: Vec::new(),
    }
}

/// Whether the engine refuses `c` with a message holding, for each list of words, one of them.
/// `Err` holds why not, for the error lines of the report.
fn refused_as_expected(c: &Pk2ErrorCase) -> std::result::Result<(), String> {
    let Some(model) = ModelId::from_id(&c.model) else {
        return Err(format!("{}: unknown model id {:?}", c.id, c.model));
    };
    let outcome = run_model(&ModelInput {
        model,
        dose: c.dose,
        params: c.parameters.clone(),
        times: c.times.clone(),
    });
    let message = match outcome {
        Ok(_) => return Err(format!("{}: accepted ({})", c.id, c.reason)),
        Err(e) => e.to_string().to_lowercase(),
    };
    match c
        .message_contains
        .iter()
        .find(|words| !words.iter().any(|w| message.contains(w.as_str())))
    {
        None => Ok(()),
        Some(words) => Err(format!(
            "{}: the message {message:?} names none of {words:?}",
            c.id
        )),
    }
}

/// The error suite: one row per input to refuse, grouped by the oracle's groups in the order they
/// first appear.
fn evaluate_errors(name: &str, cases: &[Pk2ErrorCase]) -> CaseReport {
    let mut parameters: Vec<(String, Count)> = Vec::new();
    let mut errors = Vec::new();
    for c in cases {
        let ok = match refused_as_expected(c) {
            Ok(()) => true,
            Err(why) => {
                errors.push(why);
                false
            }
        };
        let index = match parameters.iter().position(|(g, _)| g == &c.group) {
            Some(i) => i,
            None => {
                parameters.push((c.group.clone(), Count::default()));
                parameters.len() - 1
            }
        };
        if let Some((_, count)) = parameters.get_mut(index) {
            count.expected += 1;
            count.validated += usize::from(ok);
        }
    }
    CaseReport {
        kind: Kind::Model,
        name: name.to_owned(),
        details: vec!["pk2.* (inputs to refuse)".to_owned()],
        parameters,
        errors,
    }
}

// ---------------------------------------------------------------- fits

/// The groups of an expected fit quantity, in the order of the report; `status` is added.
const FIT_GROUPS: [&str; 7] = [
    "estimates",
    "wrss",
    "statistics",
    "intervals",
    "information_criteria",
    "secondary",
    "status",
];

/// The reports of every fit case, in the order of the file names.
pub(super) fn fit_reports() -> Result<Vec<CaseReport>> {
    let mut out = Vec::new();
    for name in list_fit_cases().map_err(oracle_error)? {
        let case = load_fit_case(&name).map_err(oracle_error)?;
        out.push(evaluate_fit(&case)?);
    }
    Ok(out)
}

/// Which group an expected quantity belongs to (the groups of the fit oracle tests).
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

fn fit_input(case: &FitCase, subject: &str) -> Result<FitInput> {
    let wrong = |what: &str, value: &str| {
        XtaskError::new(format!(
            "{}: {what} {value:?} is not supported by the conformance run",
            case.name
        ))
    };
    let o = &case.options;
    if o.derivatives != "analytic" {
        return Err(wrong("derivatives", &o.derivatives));
    }
    if o.criterion != "relative_decrease" {
        return Err(wrong("criterion", &o.criterion));
    }
    let model = ModelId::from_id(&case.model).ok_or_else(|| wrong("model", &case.model))?;
    let weighting =
        Weighting::from_id(&case.weighting).ok_or_else(|| wrong("weighting", &case.weighting))?;
    let profile = case
        .profile(subject)
        .ok_or_else(|| wrong("subject", subject))?;
    let initial = case
        .initial_estimates
        .get(subject)
        .cloned()
        .ok_or_else(|| wrong("subject without initial estimates", subject))?;
    let (time, conc) = case.fitted_observations(profile);
    Ok(FitInput {
        model,
        dose: profile.dose,
        time,
        conc,
        weighting,
        initial,
        options: FitOptions {
            derivatives: Derivatives::Analytic,
            criterion: Criterion::RelativeDecrease,
            convergence: o.convergence,
            max_iterations: o.max_iterations,
            confidence_level: o.confidence_level,
            fixed: case.fixed.clone(),
            ..FitOptions::default()
        },
    })
}

fn expected_of(case: &FitCase, subject: &str, name: &str) -> Option<f64> {
    case.expected.get(subject, name).flatten()
}

/// A confidence limit is `estimate -/+ c * SE`: its allowed error is that of the estimate plus that
/// of `c * SE` (the limit itself may be near zero when the interval is wide).
fn interval_ok(case: &FitCase, subject: &str, name: &str, actual: f64, value: f64) -> bool {
    let parameter = name.split_once('.').map_or(name, |(_, p)| p);
    let Some(estimate) = expected_of(case, subject, &format!("estimate.{parameter}")) else {
        return false;
    };
    let allowed = Tolerance::FIT_PARAMETERS.allowed_difference(estimate)
        + Tolerance::FIT_PARAMETERS.allowed_difference(value - estimate);
    actual.is_finite() && (actual - value).abs() <= allowed
}

/// AIC = N ln(WRSS) + 2 P and SBC = N ln(WRSS) + P ln N: the engine's value must follow the
/// formula from its own WRSS (to rounding) and may differ from the oracle by N times the relative
/// tolerance allowed on WRSS (see `crates/caladrius-fit/tests/oracle_fit.rs`).
fn information_criterion_ok(
    case: &FitCase,
    subject: &str,
    result: &FitResult,
    name: &str,
    actual: f64,
    value: f64,
) -> bool {
    let (Some(n), Some(p), Some(wrss)) = (
        expected_of(case, subject, "n"),
        expected_of(case, subject, "p"),
        result.get("wrss"),
    ) else {
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

fn fit_row_ok(
    case: &FitCase,
    subject: &str,
    result: &FitResult,
    group: &str,
    name: &str,
    value: f64,
) -> bool {
    let Some(actual) = result.get(name) else {
        return false;
    };
    match group {
        "wrss" => Tolerance::FIT_WEIGHTED_SS.accepts(actual, value),
        "intervals" => interval_ok(case, subject, name, actual, value),
        "information_criteria" => {
            information_criterion_ok(case, subject, result, name, actual, value)
        }
        _ => Tolerance::FIT_PARAMETERS.accepts(actual, value),
    }
}

fn evaluate_fit(case: &FitCase) -> Result<CaseReport> {
    let mut counts: BTreeMap<&str, Count> =
        FIT_GROUPS.iter().map(|g| (*g, Count::default())).collect();
    let mut errors = Vec::new();
    let mut results: BTreeMap<&str, FitResult> = BTreeMap::new();
    for subject in &case.subjects {
        let converged = match run_fit(&fit_input(case, subject)?) {
            Ok(result) => {
                let ok = result.status() == FitStatus::Converged;
                if !ok {
                    errors.push(format!("subject {subject}: {}", result.status().message()));
                }
                results.insert(subject.as_str(), result);
                ok
            }
            Err(e) => {
                errors.push(format!("subject {subject}: {e}"));
                false
            }
        };
        let status = counts.entry("status").or_default();
        status.expected += 1;
        status.validated += usize::from(converged);
    }
    // No fixed point: the engine must not report a converged fit.
    for subject in &case.no_fixed_point {
        let honest = match run_fit(&fit_input(case, subject)?) {
            Err(_) => true,
            Ok(result) => result.status() != FitStatus::Converged,
        };
        let status = counts.entry("status").or_default();
        status.expected += 1;
        status.validated += usize::from(honest);
    }
    for (subject, name, value) in case.expected.iter() {
        if !case.subjects.iter().any(|s| s == subject) {
            continue;
        }
        let group = group_of(name);
        let ok = match (value, results.get(subject)) {
            (Some(v), Some(result)) => fit_row_ok(case, subject, result, group, name, v),
            _ => false,
        };
        let count = counts.entry(group).or_default();
        count.expected += 1;
        count.validated += usize::from(ok);
    }
    Ok(CaseReport {
        kind: Kind::Fit,
        name: case.name.clone(),
        details: vec![
            case.model.clone(),
            case.weighting.clone(),
            format!(
                "{}{}",
                case.subjects.len(),
                if case.no_fixed_point.is_empty() {
                    String::new()
                } else {
                    format!(" (+{} without a fixed point)", case.no_fixed_point.len())
                }
            ),
        ],
        parameters: FIT_GROUPS
            .iter()
            .map(|g| ((*g).to_owned(), counts.get(g).copied().unwrap_or_default()))
            .collect(),
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(report: &CaseReport) -> Count {
        report.total()
    }

    /// Two-compartment derivative cases whose oracle expects an exact 0 for `d_alpha` at late times
    /// where the derivative is a tiny non-zero number (about 1e-52 to 1e-228, below the resolution
    /// of the oracle's 256-bit differences). The engine returns the true value; the oracle agent is
    /// asked to correct the files (`board/messages/2026-10-09-engine-oracle-pk2-derivative-zeros.md`,
    /// card T-033). Only the `d_alpha` rows of these cases may fail, and they must fail: when the
    /// files are corrected this list goes stale and the test says so.
    const PENDING_ORACLE_CORRECTION: [&str; 13] = [
        "model_pk2_deriv_iv_bolus_base_macro",
        "model_pk2_deriv_iv_bolus_distribution_macro",
        "model_pk2_deriv_iv_infusion_base_macro",
        "model_pk2_deriv_iv_infusion_distribution_macro",
        "model_pk2_deriv_oral_0_base_macro",
        "model_pk2_deriv_oral_0_distribution_macro",
        "model_pk2_deriv_oral_0_lag_base_macro",
        "model_pk2_deriv_oral_0_lag_distribution_macro",
        "model_pk2_deriv_oral_1_base_macro",
        "model_pk2_deriv_oral_1_distribution_macro",
        "model_pk2_deriv_oral_1_ka_eq_alpha_macro",
        "model_pk2_deriv_oral_1_lag_base_macro",
        "model_pk2_deriv_oral_1_lag_distribution_macro",
    ];

    #[test]
    fn every_model_case_is_fully_validated() {
        let reports = model_reports().unwrap();
        // 21 one-compartment cases, 107 + 64 two-compartment cases and the error suite.
        assert_eq!(reports.len(), 21 + 107 + 64 + 1);
        for r in reports {
            let t = total(&r);
            assert!(r.errors.is_empty(), "{}: {:?}", r.name, r.errors);
            let names: Vec<&str> = r.parameters.iter().map(|(n, _)| n.as_str()).collect();
            if r.name == PK2_ERRORS_CASE {
                assert_eq!(t.expected, 80, "{}", r.name);
            } else if r.name.starts_with("model_pk2_deriv_") {
                assert!(names.iter().all(|n| n.starts_with("d_")), "{}", r.name);
            } else {
                // The grid quantities come first: conc and auc, then aumc for two compartments.
                let first: &[&str] = if r.name.starts_with("model_pk2_") {
                    &GRID_QUANTITIES
                } else {
                    &GRID_QUANTITIES[..2]
                };
                assert_eq!(names.get(..first.len()), Some(first), "{}", r.name);
            }
            if PENDING_ORACLE_CORRECTION.contains(&r.name.as_str()) {
                for (name, c) in &r.parameters {
                    if name == "d_alpha" {
                        assert!(
                            c.validated < c.expected,
                            "{}: remove it from the list",
                            r.name
                        );
                    } else {
                        assert_eq!(c.validated, c.expected, "{} {name}", r.name);
                    }
                }
            } else {
                assert_eq!(t.validated, t.expected, "{}", r.name);
            }
        }
    }

    #[test]
    fn two_compartment_quantities_are_mapped_and_a_wrong_value_is_caught() {
        let mut case = load_pk2_case("model_pk2_oral_1_base").unwrap().case;
        let r = evaluate_model(&case);
        let get = |r: &CaseReport, n: &str| {
            r.parameters
                .iter()
                .find(|(name, _)| name == n)
                .map(|(_, c)| *c)
                .unwrap()
        };
        assert_eq!(get(&r, "aumc").validated, get(&r, "aumc").expected);
        assert!(get(&r, "aumc").expected > 0);
        if let Some(v) = case.parameters.get_mut("vp") {
            *v *= 1.0 + 1e-9;
        }
        let r = evaluate_model(&case);
        assert!(get(&r, "aumc").validated < get(&r, "aumc").expected);
        // Derivatives: a parameter moved is caught per column.
        let mut case = load_pk2_case("model_pk2_deriv_oral_1_base").unwrap().case;
        let r = evaluate_derivatives(&case);
        assert_eq!(total(&r).validated, total(&r).expected);
        if let Some(v) = case.parameters.get_mut("ka") {
            *v *= 1.0 + 1e-9;
        }
        let r = evaluate_derivatives(&case);
        assert!(get(&r, "d_ka").validated < get(&r, "d_ka").expected);
    }

    #[test]
    fn an_unknown_model_id_is_an_error_line_not_an_abort() {
        for name in ["model_pk2_iv_bolus_base", "model_pk2_deriv_iv_bolus_base"] {
            let mut case = load_pk2_case(name).unwrap().case;
            case.model = "pk9.nothing".to_owned();
            let r = if name.contains("deriv") {
                evaluate_derivatives(&case)
            } else {
                evaluate_model(&case)
            };
            assert_eq!(r.errors.len(), 1, "{name}");
            assert!(r.errors[0].contains("pk9.nothing"), "{:?}", r.errors);
            assert_eq!(total(&r).validated, 0);
            assert_eq!(total(&r).expected, case.expected.len());
        }
    }

    #[test]
    fn an_input_the_engine_accepts_is_not_a_validated_refusal() {
        let mut cases = load_pk2_errors().unwrap();
        let r = evaluate_errors("errors", &cases);
        assert_eq!(total(&r).validated, total(&r).expected);
        // Make the first case valid: it is now accepted, so it fails, with an error line.
        let first = cases.first_mut().unwrap();
        first.parameters = [("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();
        first.model = "pk2.iv_bolus".to_owned();
        first.dose = 100.0;
        first.times = vec![1.0];
        let r = evaluate_errors("errors", &cases);
        assert_eq!(total(&r).validated + 1, total(&r).expected);
        assert!(
            r.errors.iter().any(|e| e.contains("accepted")),
            "{:?}",
            r.errors
        );
    }

    #[test]
    fn every_fit_case_is_fully_validated_in_every_group() {
        let reports = fit_reports().unwrap();
        // 6 datasets (Theoph, Indometh, the five-point example, and the infusion, zero-order and
        // lag profiles of T-029) x 5 weightings.
        assert_eq!(reports.len(), 30);
        for r in reports {
            assert!(r.errors.is_empty(), "{}: {:?}", r.name, r.errors);
            for (group, c) in &r.parameters {
                assert!(c.expected > 0, "{} {group}: no rows", r.name);
                assert_eq!(c.validated, c.expected, "{} {group}", r.name);
            }
        }
    }

    #[test]
    fn a_wrong_model_value_is_caught_per_quantity() {
        let mut case = load_model_case("model_oral_1").unwrap();
        // A parameter 1e-9 off is far outside 1e-12 on the concentrations.
        if let Some(v) = case.parameters.get_mut("v") {
            *v *= 1.0 + 1e-9;
        }
        let report = evaluate_model(&case);
        let get = |n: &str| {
            report
                .parameters
                .iter()
                .find(|(name, _)| name == n)
                .map(|(_, c)| *c)
                .unwrap()
        };
        assert!(get("conc").validated < get("conc").expected);
        assert!(get("auc").validated < get("auc").expected);
    }

    #[test]
    fn a_wrong_fit_is_caught_and_a_failed_model_is_reported() {
        let mut case = load_fit_case("fit_spec_uniform").unwrap();
        // Another start is fine (same minimum), another dose is not.
        let r = evaluate_fit(&case).unwrap();
        assert_eq!(total(&r).validated, total(&r).expected);
        for profile in &mut case.dataset.profiles {
            profile.dose *= 2.0;
        }
        let r = evaluate_fit(&case).unwrap();
        let estimates = r
            .parameters
            .iter()
            .find(|(n, _)| n == "estimates")
            .unwrap()
            .1;
        assert!(estimates.validated < estimates.expected);
        // A model that cannot run gives an error line and no validated value.
        let mut model = load_model_case("model_iv_bolus_k").unwrap();
        model.parameters.clear();
        let r = evaluate_model(&model);
        assert!(!r.errors.is_empty());
        assert_eq!(total(&r).validated, 0);
    }

    #[test]
    fn quantities_are_grouped_as_the_oracle_tests_group_them() {
        for (name, group) in [
            ("estimate.v", "estimates"),
            ("wrss", "wrss"),
            ("se.k", "statistics"),
            ("correlation.v.k", "statistics"),
            ("ci_lo.v", "intervals"),
            ("planar_hi.k", "intervals"),
            ("aic", "information_criteria"),
            ("estimate.cl", "secondary"),
            ("se.half_life", "secondary"),
        ] {
            assert_eq!(group_of(name), group, "{name}");
        }
    }
}
