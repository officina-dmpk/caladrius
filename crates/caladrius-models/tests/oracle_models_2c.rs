#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Two-compartment model oracle (task T-032): `caladrius-models` against exact values on time grids
//! (`oracle/expected/models/pk2/`, produced by `oracle/scripts/models_2c_closed_form.R`: the
//! explicit sums of exponentials of `specs/models.md` MOD-2C-07 to 12 in 256-bit arithmetic,
//! cross-checked against a matrix exponential and an ODE solver), for the six ids `pk2.*`, their
//! three parameterisations, the derived quantities, the partial derivatives and the inputs the
//! engine must refuse.
//!
//! Written before the engine knows any `pk2.*` id: every test below fails today with "unknown model
//! id" (`ModelId::from_id` returns `None`) and must stay red until the engine card for two
//! compartments lands. The file compiles against the present API of the crate: the engine card does
//! not need to change a signature for it to compile, only to add the ids.
//!
//! # What the engine must provide (all in the crate root, `caladrius_models`)
//!
//! - `ModelId::from_id` for `pk2.iv_bolus`, `pk2.iv_infusion`, `pk2.oral_1`, `pk2.oral_1_lag`,
//!   `pk2.oral_0`, `pk2.oral_0_lag` (MOD-2C-20), with the same `ModelInput` as the one-compartment
//!   models: `dose` is the effective dose, `params` are by name, in exactly one complete set
//!   (MOD-2C-02): `cl, vc, q, vp` (clearance), `k10, k12, k21, vc` (micro), or `a, b, alpha, beta`
//!   (macro, the dose then required and > 0), plus `ka`, `dur`, `tlag` as the id needs.
//! - `ModelOutput::conc()` and `auc()` as before; `ModelOutput::get(name)` for the scalars of the
//!   case files: `alpha, beta, k10, k12, k21, cl, vc, q, vp, a, b, w_alpha, w_beta, half_life`
//!   (ln 2 / beta), `half_life_alpha, vss, vz, v_extrap, auc_inf, aumc_inf, mrt_system, mrt`, `c0`
//!   (bolus), `tmax_pred, cmax_pred` (every id but the bolus), `a_oral, b_oral` (first-order input,
//!   when `ka` is not close to `alpha` or `beta`). `a` and `b` are the INTRAVENOUS coefficients
//!   D*w/vc for every id (MOD-2C-02).
//! - AUMC(0, t) at the i-th input time is read as `output.get("aumc[i]")` (a name the engine card
//!   may replace by an `aumc()` slice next to `auc()`, changing the one helper `aumc_of` below, in
//!   agreement with the orchestrator; the tolerances are never to be touched).
//! - `jacobian(&input, Derivatives::Analytic)` returns, for a `pk2.*` input, the closed-form
//!   partial derivatives (`method == Derivatives::Analytic`, not the forward-difference fallback)
//!   with respect to every parameter of `input.params`, whichever set it is in.
//!
//! Tolerances: `Tolerance::MODEL_VALUES` for every value and `Tolerance::MODEL_DERIVATIVES` for
//! every derivative (both relative 1e-12, an expected zero returned as exactly zero). The oracle
//! omits the derivatives whose condition number exceeds 1e3 (zero crossings), see its options.
//!
//! The contract comparisons are the ones carried by the case files, which use only these two
//! constants. The tests in the last section ("properties of the models") are property checks beside
//! that contract, not comparisons with an oracle value: their bounds (continuity in ka within
//! 1e-2, 1e-5, 1e-8 of the limit for steps of 1e-3, 1e-6, 1e-9; a 1e-8 h infusion within 1e-7 of the
//! bolus; the lag shift within 1e-13) are not engine tolerances and decide nothing about the
//! tolerances of the engine.

use std::collections::BTreeMap;

use caladrius_models::{Derivatives, ModelId, ModelInput, ModelOutput, jacobian, run};
use caladrius_testkit::{
    ModelCase, Pk2Kind, Table, Tolerance, compare_tables, list_pk2_cases, load_pk2_case,
    load_pk2_errors,
};

fn input_of(case: &ModelCase) -> ModelInput {
    ModelInput {
        model: ModelId::from_id(&case.model)
            .unwrap_or_else(|| panic!("{}: unknown model id {:?}", case.name, case.model)),
        dose: case.dose,
        params: case.parameters.clone(),
        times: case.times.clone(),
    }
}

fn expected_of(case: &ModelCase, quantity: &str) -> Table {
    let mut expected = Table::new();
    for (group, name, value) in case.expected.iter() {
        let wanted = if quantity == "scalar" {
            group == "scalar"
        } else {
            group.starts_with("t=") && name == quantity
        };
        if wanted {
            expected.insert(group, name, value);
        }
    }
    expected
}

/// AUMC(0, t_i) of the i-th input time (see the header).
fn aumc_of(output: &ModelOutput, index: usize) -> Option<f64> {
    output.get(&format!("aumc[{index}]"))
}

fn check(case_name: &str, quantity: &str) {
    let loaded = load_pk2_case(case_name).expect("the two-compartment oracle case loads");
    assert_eq!(loaded.kind, Pk2Kind::Values, "{case_name}");
    let case = &loaded.case;
    let expected = expected_of(case, quantity);
    assert!(
        !expected.is_empty(),
        "{case_name}: nothing expected for {quantity}"
    );
    let output = run(&input_of(case))
        .unwrap_or_else(|e| panic!("{case_name}: the engine refuses a valid model: {e}"));
    let mut actual = Table::new();
    match quantity {
        "conc" | "auc" | "aumc" => {
            for (i, key) in case.time_keys.iter().enumerate() {
                let value = match quantity {
                    "conc" => output.conc().get(i).copied(),
                    "auc" => output.auc().get(i).copied(),
                    _ => aumc_of(&output, i),
                };
                actual.insert(key.as_str(), quantity, value);
            }
        }
        _ => {
            for (_, name, _) in expected.iter() {
                actual.insert("scalar", name, output.get(name));
            }
        }
    }
    let report = compare_tables(&expected, &actual, Tolerance::MODEL_VALUES);
    assert!(report.is_ok(), "{case_name} {quantity}: {report}");
}

fn check_derivatives(case_name: &str) {
    let loaded = load_pk2_case(case_name).expect("the two-compartment oracle case loads");
    assert_eq!(loaded.kind, Pk2Kind::Derivatives, "{case_name}");
    let case = &loaded.case;
    let input = input_of(case);
    let jac = jacobian(&input, Derivatives::Analytic)
        .unwrap_or_else(|e| panic!("{case_name}: no Jacobian for a valid model: {e}"));
    assert_eq!(
        jac.method,
        Derivatives::Analytic,
        "{case_name}: closed-form derivatives expected, not the forward-difference fallback"
    );
    let mut actual = Table::new();
    for (group, name, _) in case.expected.iter() {
        let column = name
            .strip_prefix("d_")
            .and_then(|p| jac.parameters.iter().position(|q| q == p))
            .and_then(|j| jac.columns.get(j));
        let index = case.time_keys.iter().position(|k| k == group);
        let value = column.zip(index).and_then(|(c, i)| c.get(i)).copied();
        actual.insert(group, name, value);
    }
    let report = compare_tables(&case.expected, &actual, Tolerance::MODEL_DERIVATIVES);
    assert!(report.is_ok(), "{case_name} derivatives: {report}");
}

macro_rules! value_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::check;
                #[test]
                fn concentration() { check(concat!("model_pk2_", stringify!($case)), "conc"); }
                #[test]
                fn auc() { check(concat!("model_pk2_", stringify!($case)), "auc"); }
                #[test]
                fn aumc() { check(concat!("model_pk2_", stringify!($case)), "aumc"); }
                #[test]
                fn scalars() { check(concat!("model_pk2_", stringify!($case)), "scalar"); }
            }
        )*
    };
}

macro_rules! derivative_tests {
    ($($case:ident),* $(,)?) => {
        mod derivatives {
            $(
                #[test]
                fn $case() { super::check_derivatives(concat!("model_pk2_deriv_", stringify!($case))); }
            )*
        }
    };
}

// Generated by oracle/scripts/models_2c_closed_form.R: the case lists, the invocations of the two
// macros above, and the constants VALUE_CASES and DERIVATIVE_CASES used by the guard below.
include!("oracle_models_2c.cases");

/// Guards the guard: every case file of the two-compartment oracle is run by a test above, and
/// every case named above exists.
#[test]
fn every_pk2_case_has_tests() {
    let on_disk = list_pk2_cases().unwrap();
    let mut listed: Vec<String> = VALUE_CASES
        .iter()
        .map(|c| format!("model_pk2_{c}"))
        .chain(
            DERIVATIVE_CASES
                .iter()
                .map(|c| format!("model_pk2_deriv_{c}")),
        )
        .collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "the cases on disk and the cases tested differ"
    );
}

// ---------------------------------------------------------------- errors (MOD-2C-05, MOD-2C-22, MOD-GEN-04)

/// Every case of the group must be refused, with a readable message that names what the oracle
/// says it must name.
fn refused(group: &str) {
    let cases: Vec<_> = load_pk2_errors()
        .unwrap()
        .into_iter()
        .filter(|c| c.group == group)
        .collect();
    assert!(!cases.is_empty(), "no error case in group {group}");
    let mut failures = Vec::new();
    for c in &cases {
        let model = ModelId::from_id(&c.model)
            .unwrap_or_else(|| panic!("{}: unknown model id {:?}", c.id, c.model));
        let result = run(&ModelInput {
            model,
            dose: c.dose,
            params: c.parameters.clone(),
            times: c.times.clone(),
        });
        match result {
            Ok(_) => failures.push(format!("{}: accepted ({})", c.id, c.reason)),
            Err(e) => {
                let message = e.to_string().to_lowercase();
                for alternatives in &c.message_contains {
                    if !alternatives.iter().any(|w| message.contains(w.as_str())) {
                        failures.push(format!(
                            "{}: message {message:?} contains none of {alternatives:?} ({})",
                            c.id, c.reason
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn errors_domain() {
    refused("domain");
}
#[test]
fn errors_one_compartment() {
    refused("one_compartment");
}
#[test]
fn errors_macro() {
    refused("macro");
}
#[test]
fn errors_sets_and_names() {
    refused("sets");
}
#[test]
fn errors_numeric() {
    refused("numeric");
}
#[test]
fn errors_times() {
    refused("times");
}

// ---------------------------------------------------------------- properties of the models

fn params(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn id(text: &str) -> ModelId {
    ModelId::from_id(text).unwrap_or_else(|| panic!("unknown model id {text:?}"))
}

fn conc_at(model: &str, dose: f64, p: &[(&str, f64)], times: &[f64]) -> Vec<f64> {
    run(&ModelInput {
        model: id(model),
        dose,
        params: params(p),
        times: times.to_vec(),
    })
    .unwrap()
    .conc()
    .to_vec()
}

const BASE: [(&str, f64); 4] = [("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)];

fn with(extra: &[(&'static str, f64)]) -> Vec<(&'static str, f64)> {
    BASE.iter().chain(extra).copied().collect()
}

/// MOD-2C-10: the value at ka = alpha is the limit of its neighbours (continuity in ka, no 0/0).
#[test]
fn ka_equal_alpha_is_the_limit_of_its_neighbours() {
    let times = [1.0, 3.0, 12.0];
    let at = |ka: f64| conc_at("pk2.oral_1", 100.0, &with(&[("ka", ka)]), &times);
    let exact = at(1.0);
    for (delta, bound) in [(1e-3, 1e-2), (1e-6, 1e-5), (1e-9, 1e-8)] {
        let (below, above) = (at(1.0 - delta), at(1.0 + delta));
        for i in 0..times.len() {
            assert!((above[i] - exact[i]).abs() <= bound * exact[i]);
            assert!((below[i] - exact[i]).abs() <= bound * exact[i]);
        }
    }
}

/// MOD-2C-08: the infusion tends to the bolus when the duration tends to 0 with the dose fixed.
#[test]
fn a_very_short_infusion_is_a_bolus_after_the_end() {
    let times = [1.0, 3.0, 10.0];
    let short = conc_at("pk2.iv_infusion", 100.0, &with(&[("dur", 1e-8)]), &times);
    let bolus = conc_at("pk2.iv_bolus", 100.0, &BASE, &times);
    for (x, y) in short.iter().zip(&bolus) {
        assert!((x - y).abs() <= 1e-7 * y.abs(), "{x} against {y}");
    }
}

/// MOD-2C-12: zero-order absorption is the infusion function (the apparent parameters).
#[test]
fn zero_order_absorption_is_the_infusion_function() {
    let times = [0.5, 1.0, 2.0, 2.5, 6.0, 24.0];
    let a = conc_at("pk2.oral_0", 100.0, &with(&[("dur", 2.0)]), &times);
    let b = conc_at("pk2.iv_infusion", 100.0, &with(&[("dur", 2.0)]), &times);
    assert_eq!(a, b);
}

/// MOD-2C-11: the lag shifts the curve: C_lag(t) = C(t - tlag), and 0 up to the lag.
#[test]
fn the_lag_shifts_the_curve() {
    let late = [0.5, 1.5, 3.0, 8.0];
    for (lag_id, plain_id, extra) in [
        ("pk2.oral_1_lag", "pk2.oral_1", [("ka", 2.0)].as_slice()),
        ("pk2.oral_0_lag", "pk2.oral_0", [("dur", 2.0)].as_slice()),
    ] {
        let tlag = 0.75;
        let shifted: Vec<f64> = late.iter().map(|t| t + tlag).collect();
        let mut lagged = with(extra);
        lagged.push(("tlag", tlag));
        let a = conc_at(lag_id, 100.0, &lagged, &shifted);
        let b = conc_at(plain_id, 100.0, &with(extra), &late);
        for (x, y) in a.iter().zip(&b) {
            assert!(
                (x - y).abs() <= 1e-13 * y.abs(),
                "{lag_id}: {x} against {y}"
            );
        }
        let before = conc_at(lag_id, 100.0, &lagged, &[-1.0, 0.0, 0.5, tlag]);
        assert!(before.iter().all(|&x| x == 0.0), "{lag_id}: {before:?}");
    }
}

/// MOD-GEN-02: values before the dose are 0, also for the bolus; the bolus at t = 0 is D / vc.
#[test]
fn time_origin_conventions() {
    let c = conc_at("pk2.iv_bolus", 100.0, &BASE, &[-5.0, -1e-9, 0.0]);
    assert_eq!(c[0], 0.0);
    assert_eq!(c[1], 0.0);
    assert!((c[2] - 10.0).abs() <= 1e-13);
}

/// The order of the times does not matter and a repeated time gives the same value twice.
#[test]
fn times_may_be_unsorted_or_repeated() {
    let p = with(&[("ka", 2.0)]);
    let a = conc_at("pk2.oral_1", 100.0, &p, &[6.0, 0.5, 6.0, 2.0]);
    let b = conc_at("pk2.oral_1", 100.0, &p, &[0.5, 2.0, 6.0]);
    assert_eq!(a[0], a[2]);
    assert_eq!(a[1], b[0]);
    assert_eq!(a[3], b[1]);
    assert_eq!(a[0], b[2]);
}

/// An empty grid is a valid request with empty answers; the scalars still exist (worked example N1).
#[test]
fn an_empty_time_grid_is_not_an_error() {
    let out = run(&ModelInput {
        model: id("pk2.iv_bolus"),
        dose: 100.0,
        params: params(&BASE),
        times: Vec::new(),
    })
    .unwrap();
    assert!(out.conc().is_empty() && out.auc().is_empty());
    assert_eq!(out.get("auc_inf"), Some(50.0));
    assert_eq!(out.get("no_such_quantity"), None);
}

/// A dose of 0 gives 0 everywhere with the clearance and micro sets (MOD-2C-05).
#[test]
fn a_zero_dose_gives_zero_concentrations() {
    let c = conc_at("pk2.iv_bolus", 0.0, &BASE, &[0.0, 1.0, 10.0]);
    assert!(c.iter().all(|&x| x == 0.0), "{c:?}");
}

/// MOD-2C-02 and MOD-2C-04: the three parameterisations of worked example N7 are one model. The
/// engine's values for the micro and the macro set agree with the exact values of the clearance set
/// to the model tolerance (the macro inputs are doubles that only approximate 50/9 and 40/9, which
/// moves nothing at the 1e-12 level on this well-conditioned point).
#[test]
fn the_three_parameterisations_are_one_model() {
    let reference = load_pk2_case("model_pk2_iv_bolus_base").unwrap().case;
    for name in [
        "model_pk2_iv_bolus_base_micro",
        "model_pk2_iv_bolus_base_macro",
    ] {
        let case = load_pk2_case(name).unwrap().case;
        let out = run(&input_of(&case)).unwrap();
        let mut expected = Table::new();
        let mut actual = Table::new();
        for (i, key) in case.time_keys.iter().enumerate() {
            expected.insert(
                key.as_str(),
                "conc",
                reference.expected.get(key, "conc").flatten(),
            );
            actual.insert(key.as_str(), "conc", out.conc().get(i).copied());
        }
        for scalar in [
            "alpha", "beta", "k10", "k12", "k21", "cl", "vc", "q", "vp", "a", "b",
        ] {
            expected.insert(
                "scalar",
                scalar,
                reference.expected.get("scalar", scalar).flatten(),
            );
            actual.insert("scalar", scalar, out.get(scalar));
        }
        let report = compare_tables(&expected, &actual, Tolerance::MODEL_VALUES);
        assert!(report.is_ok(), "{name}: {report}");
    }
}
