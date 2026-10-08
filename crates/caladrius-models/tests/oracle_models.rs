#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Model oracle (task T-009): `caladrius-models` against exact closed-form values on time grids
//! (`oracle/expected/models/`, produced by `oracle/scripts/models_closed_form.R` in 256-bit
//! arithmetic and cross-checked against a matrix exponential and an ODE solver), for the six
//! one-compartment models of `specs/models.md`.
//!
//! Written before the engine exists (task T-009): the file does not compile until `caladrius-models`
//! provides the API below, and its tests are not ignored.
//!
//! # API the engine must provide (all in the crate root, `caladrius_models`)
//!
//! Close to the NCA style: one input value, one `run`, results by name. The engine agent may add
//! fields, variants and methods, but must keep these names and meanings or change this file in
//! agreement with the orchestrator (the tolerances are never to be touched). Inputs and outputs
//! are data and should derive `Serialize`/`Deserialize` (golden rule 4).
//!
//! ```ignore
//! /// Model ids of specs/models.md MOD-GEN-05.
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum ModelId { IvBolus, IvInfusion, Oral1, Oral1Lag, Oral0, Oral0Lag }
//! impl ModelId {
//!     /// "pk1.iv_bolus", "pk1.iv_infusion", "pk1.oral_1", "pk1.oral_1_lag", "pk1.oral_0", "pk1.oral_0_lag".
//!     pub fn from_id(id: &str) -> Option<ModelId>;
//!     pub fn id(&self) -> &'static str;
//! }
//! pub struct ModelInput {
//!     pub model: ModelId,
//!     /// Effective dose F * D (MOD-GEN-03). 0 is allowed and gives 0 everywhere.
//!     pub dose: f64,
//!     /// Parameters by the names of MOD-VOC-01: `v`; exactly one of `cl` and `k`; `ka` (first-order
//!     /// input); `dur` (infusion, zero-order input); `tlag` (lag models). Any other combination,
//!     /// or an unknown name, is an error.
//!     pub params: BTreeMap<String, f64>,
//!     /// Times since the dose, in any order; a time before the dose (or before the lag) gives 0.
//!     pub times: Vec<f64>,
//! }
//! pub struct ModelOutput { /* private */ }
//! impl ModelOutput {
//!     /// Concentration at each input time, in the order of `times`.
//!     pub fn conc(&self) -> &[f64];
//!     /// AUC(0, t) from the dose time at each input time (0 for t <= 0).
//!     pub fn auc(&self) -> &[f64];
//!     /// Secondary parameters by name: `v`, `k`, `cl`, `half_life`, `auc_inf`, `mrt_system` (1/k),
//!     /// `mrt` (profile MRT: 1/k + 1/ka + tlag first-order, 1/k + dur/2 + tlag zero-order and
//!     /// infusion, 1/k bolus), `vss`; bolus: `c0`; models with a peak: `tmax_pred`, `cmax_pred`.
//!     /// `None` when the model has no such quantity.
//!     pub fn get(&self, name: &str) -> Option<f64>;
//! }
//! pub struct ModelError { /* Display, std::error::Error; says which parameter and what to fix */ }
//! pub fn run(input: &ModelInput) -> Result<ModelOutput, ModelError>;
//! ```
//!
//! Numerics the tests demand (MOD-AB1-03, MOD-NUM-01): relative error at most 1e-12 on every
//! quantity (`Tolerance::MODEL_VALUES`), also for ka = k and for ka - k as small as 1e-9 relative
//! to k, where the textbook forms of the concentration AND of the AUC lose digits; an expected zero
//! is returned as exactly 0.

use std::collections::BTreeMap;

use caladrius_models::{ModelId, ModelInput, run};
use caladrius_testkit::{ModelCase, Table, Tolerance, compare_tables, load_model_case};

fn input_of(case: &ModelCase) -> ModelInput {
    ModelInput {
        model: ModelId::from_id(&case.model)
            .unwrap_or_else(|| panic!("{}: unknown model id {:?}", case.name, case.model)),
        dose: case.dose,
        params: case.parameters.clone(),
        times: case.times.clone(),
    }
}

/// The expected values of one quantity (`conc`, `auc`) on the grid, or of every `scalar`.
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

fn check(case_name: &str, quantity: &str) {
    let case = load_model_case(case_name).expect("the model oracle case loads");
    let expected = expected_of(&case, quantity);
    assert!(
        !expected.is_empty(),
        "{case_name}: nothing expected for {quantity}"
    );
    let output = run(&input_of(&case))
        .unwrap_or_else(|e| panic!("{case_name}: the engine refuses a valid model: {e}"));
    let mut actual = Table::new();
    match quantity {
        "conc" | "auc" => {
            let values = if quantity == "conc" {
                output.conc()
            } else {
                output.auc()
            };
            assert_eq!(
                values.len(),
                case.times.len(),
                "{case_name}: one {quantity} per time"
            );
            for (key, value) in case.time_keys.iter().zip(values) {
                actual.insert(key.as_str(), quantity, Some(*value));
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

macro_rules! oracle_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::check;
                #[test]
                fn concentration() { check(concat!("model_", stringify!($case)), "conc"); }
                #[test]
                fn auc() { check(concat!("model_", stringify!($case)), "auc"); }
                #[test]
                fn secondary_parameters() { check(concat!("model_", stringify!($case)), "scalar"); }
            }
        )*
    };
}

oracle_tests! {
    iv_bolus_cl, iv_bolus_k, iv_bolus_b, iv_bolus_zero_dose,
    iv_infusion, iv_infusion_b,
    oral_1, oral_1_k, oral_1_slow_ka, oral_1_fast_ka, oral_1_ka_eq_k,
    oral_1_ka_near_k_1e9, oral_1_ka_near_k_1e6, oral_1_ka_near_k_1e3, oral_1_flip_flop,
    oral_1_lag, oral_1_lag_b, oral_1_lag_ka_eq_k,
    oral_0, oral_0_lag, oral_0_lag_b,
}

/// Guards the guard: every model case of the oracle is run by a test above.
#[test]
fn every_model_case_has_tests() {
    let covered = [
        "iv_bolus_cl",
        "iv_bolus_k",
        "iv_bolus_b",
        "iv_bolus_zero_dose",
        "iv_infusion",
        "iv_infusion_b",
        "oral_1",
        "oral_1_k",
        "oral_1_slow_ka",
        "oral_1_fast_ka",
        "oral_1_ka_eq_k",
        "oral_1_ka_near_k_1e9",
        "oral_1_ka_near_k_1e6",
        "oral_1_ka_near_k_1e3",
        "oral_1_flip_flop",
        "oral_1_lag",
        "oral_1_lag_b",
        "oral_1_lag_ka_eq_k",
        "oral_0",
        "oral_0_lag",
        "oral_0_lag_b",
    ];
    for name in caladrius_testkit::list_model_cases().unwrap() {
        let short = name.strip_prefix("model_").unwrap();
        assert!(covered.contains(&short), "{name} has no test");
    }
}

// ---------------------------------------------------------------- properties of the models

fn params(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn conc_at(model: ModelId, dose: f64, p: &[(&str, f64)], times: &[f64]) -> Vec<f64> {
    run(&ModelInput {
        model,
        dose,
        params: params(p),
        times: times.to_vec(),
    })
    .unwrap()
    .conc()
    .to_vec()
}

/// MOD-AB1-06 (worked example M4): exchanging ka and k and replacing V by V * k / ka gives the same
/// concentrations, to rounding.
#[test]
fn flip_flop_pair_gives_the_same_curve() {
    let times = [0.5, 1.0, 3.0, 8.0, 24.0];
    let a = conc_at(
        ModelId::Oral1,
        100.0,
        &[("v", 10.0), ("k", 0.2), ("ka", 1.0)],
        &times,
    );
    let b = conc_at(
        ModelId::Oral1,
        100.0,
        &[("v", 2.0), ("k", 1.0), ("ka", 0.2)],
        &times,
    );
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() <= 1e-13 * x.abs(), "{x} against {y}");
    }
}

/// MOD-AB1-02: the value at ka = k is the limit of the neighbours (continuity in ka).
#[test]
fn ka_equal_k_is_the_limit_of_its_neighbours() {
    let times = [1.0, 5.0, 20.0];
    let at = |ka: f64| {
        conc_at(
            ModelId::Oral1,
            100.0,
            &[("v", 10.0), ("k", 0.2), ("ka", ka)],
            &times,
        )
    };
    let exact = at(0.2);
    for (delta, bound) in [(1e-3, 1e-2), (1e-6, 1e-5), (1e-9, 1e-8)] {
        let below = at(0.2 - delta);
        let above = at(0.2 + delta);
        for i in 0..times.len() {
            // The function is smooth in ka: the neighbours differ from the limit by O(delta), and
            // the mean of the two sides by O(delta^2).
            assert!((above[i] - exact[i]).abs() <= bound * exact[i]);
            assert!((below[i] - exact[i]).abs() <= bound * exact[i]);
            let mid = 0.5 * (above[i] + below[i]);
            assert!((mid - exact[i]).abs() <= 100.0 * delta * delta * exact[i] + 1e-13 * exact[i]);
        }
    }
}

/// MOD-IVI-01: the infusion reduces to the bolus when the duration tends to 0 with the dose fixed.
#[test]
fn a_very_short_infusion_is_a_bolus_after_the_end() {
    let times = [1.0, 3.0, 10.0];
    let short = conc_at(
        ModelId::IvInfusion,
        100.0,
        &[("v", 10.0), ("k", 0.2), ("dur", 1e-8)],
        &times,
    );
    let bolus = conc_at(ModelId::IvBolus, 100.0, &[("v", 10.0), ("k", 0.2)], &times);
    for (x, y) in short.iter().zip(&bolus) {
        assert!((x - y).abs() <= 1e-7 * y.abs(), "{x} against {y}");
    }
}

/// MOD-GEN-02: values before the dose are 0, also for the bolus; the bolus at t = 0 is D / V.
#[test]
fn time_origin_conventions() {
    let c = conc_at(
        ModelId::IvBolus,
        100.0,
        &[("v", 10.0), ("cl", 2.0)],
        &[-5.0, -1e-9, 0.0],
    );
    assert_eq!(c[0], 0.0);
    assert_eq!(c[1], 0.0);
    assert!((c[2] - 10.0).abs() <= 1e-14);
    let oral = conc_at(
        ModelId::Oral1Lag,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", 0.5)],
        &[-1.0, 0.0, 0.25, 0.5],
    );
    assert!(oral.iter().all(|&x| x == 0.0), "{oral:?}");
}

/// The order of the times does not matter and a repeated time gives the same value twice.
#[test]
fn times_may_be_unsorted_or_repeated() {
    let p = [("v", 10.0), ("cl", 2.0), ("ka", 1.0)];
    let a = conc_at(ModelId::Oral1, 100.0, &p, &[6.0, 0.5, 6.0, 2.0]);
    let b = conc_at(ModelId::Oral1, 100.0, &p, &[0.5, 2.0, 6.0]);
    assert_eq!(a[0], a[2]);
    assert_eq!(a[1], b[0]);
    assert_eq!(a[3], b[1]);
    assert_eq!(a[0], b[2]);
}

// ---------------------------------------------------------------- errors (MOD-GEN-04, golden rule 6)

fn error_of(model: ModelId, dose: f64, p: &[(&str, f64)], times: &[f64]) -> String {
    match run(&ModelInput {
        model,
        dose,
        params: params(p),
        times: times.to_vec(),
    }) {
        Ok(_) => panic!("the engine accepted {p:?} with dose {dose} and times {times:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn parameters_outside_their_domain_are_refused_by_name() {
    let t = [1.0];
    let with = |name: &'static str, value: f64| -> Vec<(&'static str, f64)> {
        let mut p: BTreeMap<&'static str, f64> = [("v", 10.0), ("cl", 2.0), ("ka", 1.0)].into();
        p.insert(name, value);
        p.into_iter().collect()
    };
    for (name, bad) in [
        ("v", 0.0),
        ("v", -1.0),
        ("v", f64::NAN),
        ("cl", 0.0),
        ("cl", -2.0),
        ("ka", 0.0),
        ("ka", -1.0),
        ("ka", f64::INFINITY),
    ] {
        let message = error_of(ModelId::Oral1, 100.0, &with(name, bad), &t);
        assert!(
            message.contains(name),
            "the error for {name} = {bad} should name the parameter: {message}"
        );
    }
    let message = error_of(
        ModelId::Oral1Lag,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", -0.5)],
        &t,
    );
    assert!(message.contains("tlag"), "{message}");
    let message = error_of(
        ModelId::IvInfusion,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("dur", 0.0)],
        &t,
    );
    assert!(message.contains("dur"), "{message}");
}

#[test]
fn a_wrong_set_of_parameters_is_refused() {
    let t = [1.0];
    // Both cl and k: ambiguous. Neither: incomplete.
    error_of(
        ModelId::IvBolus,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("k", 0.2)],
        &t,
    );
    error_of(ModelId::IvBolus, 100.0, &[("v", 10.0)], &t);
    // Missing the input parameter of the model.
    error_of(ModelId::Oral1, 100.0, &[("v", 10.0), ("cl", 2.0)], &t);
    error_of(ModelId::IvInfusion, 100.0, &[("v", 10.0), ("cl", 2.0)], &t);
    error_of(
        ModelId::Oral1Lag,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("ka", 1.0)],
        &t,
    );
    // A parameter the model does not have, and an unknown name.
    error_of(
        ModelId::IvBolus,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("ka", 1.0)],
        &t,
    );
    let message = error_of(
        ModelId::IvBolus,
        100.0,
        &[("v", 10.0), ("cl", 2.0), ("volume", 3.0)],
        &t,
    );
    assert!(message.contains("volume"), "{message}");
}

#[test]
fn doses_and_times_that_cannot_be_evaluated_are_refused() {
    let p = [("v", 10.0), ("cl", 2.0)];
    error_of(ModelId::IvBolus, -1.0, &p, &[1.0]);
    error_of(ModelId::IvBolus, f64::NAN, &p, &[1.0]);
    error_of(ModelId::IvBolus, f64::INFINITY, &p, &[1.0]);
    error_of(ModelId::IvBolus, 100.0, &p, &[1.0, f64::NAN]);
    error_of(ModelId::IvBolus, 100.0, &p, &[f64::INFINITY]);
    error_of(ModelId::IvBolus, 100.0, &p, &[f64::NEG_INFINITY]);
}

/// An empty grid is a valid request with empty answers; the secondary parameters still exist.
#[test]
fn an_empty_time_grid_is_not_an_error() {
    let out = run(&ModelInput {
        model: ModelId::IvBolus,
        dose: 100.0,
        params: params(&[("v", 10.0), ("cl", 2.0)]),
        times: Vec::new(),
    })
    .unwrap();
    assert!(out.conc().is_empty() && out.auc().is_empty());
    assert_eq!(out.get("auc_inf"), Some(50.0));
    assert_eq!(out.get("no_such_quantity"), None);
}
