//! The model and fit cases of the conformance table (`oracle/expected/models/` and
//! `oracle/expected/fit/`), compared the way `crates/caladrius-models/tests/oracle_models.rs` and
//! `crates/caladrius-fit/tests/oracle_fit.rs` do, with the tolerances of `caladrius-testkit`.
//!
//! A model case counts one row per expected value, grouped by quantity (`conc`, `auc`, then each
//! secondary parameter). A fit case counts one row per expected value of every fitted subject,
//! grouped as the oracle tests group them, plus one `status` row per subject: converged for the
//! fitted subjects, not reported as converged for the subjects with no fixed point.

use std::collections::BTreeMap;

use caladrius_fit::{
    Criterion, Derivatives, FitInput, FitOptions, FitResult, FitStatus, Weighting,
    run as run_fit,
};
use caladrius_models::{ModelId, ModelInput, ModelOutput, run as run_model};
use caladrius_testkit::{
    FitCase, ModelCase, Tolerance, list_fit_cases, list_model_cases, load_fit_case,
    load_model_case,
};

use super::{CaseReport, Count, Kind};
use crate::error::{Result, XtaskError};

fn oracle_error(e: caladrius_testkit::OracleError) -> XtaskError {
    XtaskError::new(e.to_string())
}

// ---------------------------------------------------------------- models

/// The reports of every model case, in the order of the file names.
pub(super) fn model_reports() -> Result<Vec<CaseReport>> {
    let mut out = Vec::new();
    for name in list_model_cases().map_err(oracle_error)? {
        let case = load_model_case(&name).map_err(oracle_error)?;
        out.push(evaluate_model(&case)?);
    }
    Ok(out)
}

/// The value the engine gives for one expected row of a model case.
fn model_actual(case: &ModelCase, output: &ModelOutput, group: &str, name: &str) -> Option<f64> {
    if group == "scalar" {
        return output.get(name);
    }
    let index = case.time_keys.iter().position(|k| k == group)?;
    match name {
        "conc" => output.conc().get(index).copied(),
        "auc" => output.auc().get(index).copied(),
        _ => None,
    }
}

fn evaluate_model(case: &ModelCase) -> Result<CaseReport> {
    let model = ModelId::from_id(&case.model).ok_or_else(|| {
        XtaskError::new(format!(
            "{}: unknown model id {:?} in the oracle options",
            case.name, case.model
        ))
    })?;
    let outcome = run_model(&ModelInput {
        model,
        dose: case.dose,
        params: case.parameters.clone(),
        times: case.times.clone(),
    });
    let (output, errors) = match outcome {
        Ok(o) => (Some(o), Vec::new()),
        Err(e) => (None, vec![e.to_string()]),
    };
    let mut counts: BTreeMap<String, Count> = BTreeMap::new();
    for (group, name, expected) in case.expected.iter() {
        let actual = output
            .as_ref()
            .and_then(|o| model_actual(case, o, group, name));
        let ok = match (expected, actual, output.is_some()) {
            (Some(e), Some(a), _) => Tolerance::MODEL_VALUES.accepts(a, e),
            // Expected not available: the model must run and not have the quantity.
            (None, None, true) => true,
            _ => false,
        };
        let count = counts.entry(name.to_owned()).or_default();
        count.expected += 1;
        count.validated += usize::from(ok);
    }
    // `conc` and `auc` first, then the secondary parameters in name order.
    let mut parameters: Vec<(String, Count)> = Vec::new();
    for first in ["conc", "auc"] {
        if let Some(c) = counts.remove(first) {
            parameters.push((first.to_owned(), c));
        }
    }
    parameters.extend(counts);
    Ok(CaseReport {
        kind: Kind::Model,
        name: case.name.clone(),
        details: vec![case.model.clone()],
        parameters,
        errors,
    })
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
    let mut counts: BTreeMap<&str, Count> = FIT_GROUPS
        .iter()
        .map(|g| (*g, Count::default()))
        .collect();
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

    #[test]
    fn every_model_case_is_fully_validated() {
        let reports = model_reports().unwrap();
        assert!(reports.len() >= 20);
        for r in reports {
            let t = total(&r);
            assert!(r.errors.is_empty(), "{}: {:?}", r.name, r.errors);
            assert_eq!(t.validated, t.expected, "{}", r.name);
            // conc and auc come first.
            let names: Vec<&str> = r.parameters.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names.get(..2), Some(["conc", "auc"].as_slice()), "{}", r.name);
        }
    }

    #[test]
    fn every_fit_case_is_fully_validated_in_every_group() {
        let reports = fit_reports().unwrap();
        assert_eq!(reports.len(), 15);
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
        let report = evaluate_model(&case).unwrap();
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
        let estimates = r.parameters.iter().find(|(n, _)| n == "estimates").unwrap().1;
        assert!(estimates.validated < estimates.expected);
        // A model that cannot run gives an error line and no validated value.
        let mut model = load_model_case("model_iv_bolus_k").unwrap();
        model.parameters.clear();
        let r = evaluate_model(&model).unwrap();
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
