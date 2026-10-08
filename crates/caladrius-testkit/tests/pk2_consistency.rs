#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! Cross-checks the two-compartment oracle (task T-032) against independent arithmetic
//! (`AGENTS.md` section 5, source 3), so that the expected files are tested by something other than
//! the R script that wrote them:
//!
//! - the files load, are complete and have the expected shape;
//! - the naive double-precision textbook forms of `caladrius_testkit::naive2c` reproduce the
//!   concentrations, AUC and AUMC of the cases where those forms keep their digits;
//! - the scalars satisfy the identities of `specs/models.md` MOD-2C-03, 04, 13, 14;
//! - the three parameterisations of one point give one curve;
//! - the worked examples N1 to N8 of `specs/models.md` section 11.6, computed by the reader with
//!   70-digit arithmetic in a throwaway script, are reproduced at the digits printed there;
//! - the derivative grids agree with central differences of the naive implementation.
//!
//! This tests the oracle and the testkit, not the engine.

use std::collections::BTreeMap;

use caladrius_testkit::naive2c::{conc_auc_aumc, exponents};
use caladrius_testkit::{
    Pk2Case, Pk2Kind, Tolerance, list_pk2_cases, load_pk2_case, load_pk2_errors, pk2_dir,
};

const VALUE_CASES: usize = 107;

fn all_cases() -> Vec<Pk2Case> {
    list_pk2_cases()
        .unwrap()
        .iter()
        .map(|n| load_pk2_case(n).unwrap())
        .collect()
}

fn of_kind(kind: Pk2Kind) -> Vec<Pk2Case> {
    all_cases().into_iter().filter(|c| c.kind == kind).collect()
}

fn close(what: &str, actual: f64, expected: f64, rel: f64) {
    let allowed = if expected == 0.0 {
        0.0
    } else {
        rel * expected.abs()
    };
    assert!(
        (actual - expected).abs() <= allowed,
        "{what}: independent value {actual:e}, oracle {expected:e}"
    );
}

fn scalar(c: &Pk2Case, name: &str) -> Option<f64> {
    c.case.expected.get("scalar", name).flatten()
}

fn grid(c: &Pk2Case, key: &str, quantity: &str) -> f64 {
    c.case.expected.get(key, quantity).flatten().unwrap()
}

const SCALARS: [&str; 22] = [
    "alpha",
    "beta",
    "k10",
    "k12",
    "k21",
    "cl",
    "vc",
    "q",
    "vp",
    "a",
    "b",
    "w_alpha",
    "w_beta",
    "half_life",
    "half_life_alpha",
    "vss",
    "vz",
    "v_extrap",
    "auc_inf",
    "aumc_inf",
    "mrt_system",
    "mrt",
];

#[test]
fn the_oracle_has_its_expected_shape() {
    let cases = all_cases();
    let values: Vec<_> = cases.iter().filter(|c| c.kind == Pk2Kind::Values).collect();
    let derivs: Vec<_> = cases
        .iter()
        .filter(|c| c.kind == Pk2Kind::Derivatives)
        .collect();
    assert_eq!(values.len(), VALUE_CASES);
    assert!(derivs.len() >= 60, "{} derivative cases", derivs.len());
    // all six ids, in all three parameterisations
    for model in [
        "pk2.iv_bolus",
        "pk2.iv_infusion",
        "pk2.oral_1",
        "pk2.oral_1_lag",
        "pk2.oral_0",
        "pk2.oral_0_lag",
    ] {
        for set in ["clearance", "micro", "macro"] {
            assert!(
                values
                    .iter()
                    .any(|c| c.case.model == model && c.parameterisation == set),
                "{model} in the {set} set"
            );
            assert!(
                derivs
                    .iter()
                    .any(|c| c.case.model == model && c.parameterisation == set),
                "derivatives of {model} in the {set} set"
            );
        }
    }
    for c in &values {
        for key in &c.case.time_keys {
            for q in ["conc", "auc", "aumc"] {
                assert!(
                    c.case.expected.get(key, q).flatten().is_some(),
                    "{} {key} {q}",
                    c.case.name
                );
            }
        }
        for name in SCALARS {
            assert!(
                scalar(c, name).is_some(),
                "{} lacks the scalar {name}",
                c.case.name
            );
        }
        if c.case.model == "pk2.iv_bolus" {
            assert!(scalar(c, "c0").is_some(), "{}", c.case.name);
        } else if c.case.dose > 0.0 || !c.case.model.starts_with("pk2.oral_1") {
            assert!(
                scalar(c, "tmax_pred").is_some() && scalar(c, "cmax_pred").is_some(),
                "{}",
                c.case.name
            );
        }
        assert!(
            c.case.times.iter().any(|&t| t < 0.0),
            "{}: a time before the dose",
            c.case.name
        );
        assert!(c.case.times.contains(&0.0), "{}: t = 0", c.case.name);
    }
    for c in &derivs {
        for (group, name, value) in c.case.expected.iter() {
            assert!(group.starts_with("t="), "{} {group}", c.case.name);
            let parameter = name.strip_prefix("d_").unwrap();
            assert!(
                c.case.parameters.contains_key(parameter),
                "{}: derivative with respect to {parameter}, which is not a parameter",
                c.case.name
            );
            assert!(
                value.is_some_and(f64::is_finite),
                "{} {group} {name}",
                c.case.name
            );
        }
    }
    // the cases reach the extremes the card asks for
    let names: Vec<_> = cases.iter().map(|c| c.case.name.as_str()).collect();
    for needed in [
        "model_pk2_oral_1_ka_eq_alpha_macro",
        "model_pk2_oral_1_ka_eq_beta_macro",
        "model_pk2_oral_1_ka_alpha_plus_1e9",
        "model_pk2_oral_1_ka_beta_minus_1e9",
        "model_pk2_oral_1_ka_between",
        "model_pk2_oral_1_ka_below_beta",
        "model_pk2_oral_1_ka_500_alpha",
        "model_pk2_iv_bolus_near_degenerate_1e12_micro",
        "model_pk2_iv_bolus_ab_ratio_1e3",
        "model_pk2_iv_bolus_zero_dose",
        "model_pk2_oral_0_lag_extreme_fast",
        "model_pk2_oral_1_extreme_slow",
    ] {
        assert!(names.contains(&needed), "{needed} is missing");
    }
}

#[test]
fn the_error_suite_covers_every_rule_of_the_spec() {
    let errors = load_pk2_errors().unwrap();
    assert!(errors.len() >= 60, "{} error cases", errors.len());
    let mut ids: Vec<_> = errors.iter().map(|e| e.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), errors.len(), "duplicate ids");
    for group in [
        "domain",
        "one_compartment",
        "macro",
        "sets",
        "numeric",
        "times",
    ] {
        assert!(
            errors.iter().any(|e| e.group == group),
            "no case in {group}"
        );
    }
    for e in &errors {
        assert!(e.model.starts_with("pk2."), "{}", e.id);
        assert!(!e.reason.is_empty() && !e.spec.is_empty(), "{}", e.id);
        for alternatives in &e.message_contains {
            assert!(!alternatives.is_empty(), "{}", e.id);
            assert!(
                alternatives.iter().all(|w| *w == w.to_lowercase()),
                "{}: the words are lower case",
                e.id
            );
        }
    }
    // the rules named by the card (OM-07) and by MOD-2C-05
    for needed in [
        "one_compartment_q_zero_iv_bolus",
        "one_compartment_vp_zero_oral_1",
        "micro_k12_zero",
        "macro_alpha_equals_beta",
        "macro_alpha_below_beta",
        "macro_dose_zero",
        "sets_clearance_plus_k10",
        "sets_unknown_name",
        "numeric_degenerate_exponents",
        "numeric_overflow_c0",
    ] {
        assert!(errors.iter().any(|e| e.id == needed), "{needed}");
    }
    // a refused input is never a valid one: every non-finite number is a real NaN or infinity
    let nan = errors.iter().find(|e| e.id == "clearance_q_nan").unwrap();
    assert!(nan.parameters["q"].is_nan());
    let times = errors.iter().find(|e| e.id == "times_nan").unwrap();
    assert!(times.times.iter().any(|t| t.is_nan()));
}

// ---------------------------------------------------------------- textbook forms in double precision

#[test]
fn the_naive_textbook_forms_reproduce_the_exact_values() {
    let mut checked = 0;
    let mut skipped = 0;
    let mut failures = Vec::new();
    for c in of_kind(Pk2Kind::Values) {
        if !c.textbook_double_ok {
            skipped += 1;
            continue;
        }
        checked += 1;
        let case = &c.case;
        for (name, index) in [("conc", 0), ("auc", 1), ("aumc", 2)] {
            let scale = case
                .time_keys
                .iter()
                .map(|k| grid(&c, k, name).abs())
                .fold(0.0, f64::max);
            for (t, key) in case.times.iter().zip(&case.time_keys) {
                let got = conc_auc_aumc(
                    &case.model,
                    &c.parameterisation,
                    &case.parameters,
                    case.dose,
                    *t,
                )
                .unwrap_or_else(|| panic!("{}: the naive implementation cannot do it", case.name));
                let expected = grid(&c, key, name);
                // Relative 1e-8, and a floor of 1e-13 of the largest value of the profile for the
                // decayed tail, where the sum of exponentials of the textbook form is made of
                // terms that cancel at rounding level.
                let allowed = 1e-8 * expected.abs() + 1e-13 * scale;
                if (got[index] - expected).abs() > allowed {
                    failures.push(format!(
                        "{} {name} at t = {t}: naive {:e}, oracle {expected:e}",
                        case.name, got[index]
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} values differ:
{}",
        failures.len(),
        failures.iter().take(40).cloned().collect::<Vec<_>>().join(
            "
"
        )
    );
    assert!(
        checked >= 50,
        "only {checked} cases checked, {skipped} skipped"
    );
    assert!(
        skipped >= 20,
        "the near-degenerate cases are the ones the textbook form cannot do"
    );
}

// ---------------------------------------------------------------- identities

#[test]
fn the_scalars_satisfy_the_identities_of_the_spec() {
    for c in of_kind(Pk2Kind::Values) {
        let case = &c.case;
        let name = &case.name;
        let s = |n: &str| scalar(&c, n).unwrap();
        let dose = case.dose;
        let (alpha, beta) = (s("alpha"), s("beta"));
        let (k10, k12, k21) = (s("k10"), s("k12"), s("k21"));
        let (cl, vc, q, vp) = (s("cl"), s("vc"), s("q"), s("vp"));
        assert!(alpha > beta && beta > 0.0, "{name}");
        // MOD-2C-03: sum and product of the exponents, k10 and k21 lie between them
        close(
            &format!("{name} alpha + beta"),
            alpha + beta,
            k10 + k12 + k21,
            1e-13,
        );
        close(
            &format!("{name} alpha * beta"),
            alpha * beta,
            k10 * k21,
            1e-13,
        );
        assert!(
            beta < k10 && k10 < alpha && beta < k21 && k21 < alpha,
            "{name}"
        );
        close(
            &format!("{name} w sum"),
            s("w_alpha") + s("w_beta"),
            1.0,
            1e-14,
        );
        close(&format!("{name} cl"), cl, k10 * vc, 1e-14);
        close(&format!("{name} q"), q, k12 * vc, 1e-14);
        close(&format!("{name} vp"), vp, q / k21, 1e-14);
        // MOD-2C-13
        close(
            &format!("{name} half_life"),
            s("half_life"),
            std::f64::consts::LN_2 / beta,
            1e-14,
        );
        close(
            &format!("{name} half_life_alpha"),
            s("half_life_alpha"),
            std::f64::consts::LN_2 / alpha,
            1e-14,
        );
        close(&format!("{name} vss"), s("vss"), vc + vp, 1e-14);
        close(&format!("{name} vz"), s("vz"), cl / beta, 1e-14);
        close(
            &format!("{name} mrt_system"),
            s("mrt_system"),
            s("vss") / cl,
            1e-14,
        );
        // order of the volumes
        assert!(
            vc < s("vss") && s("vss") < s("vz") && s("vz") < s("v_extrap"),
            "{name}"
        );
        // MOD-2C-03: a and b are the intravenous coefficients, a + b = D / vc
        close(
            &format!("{name} vc from a + b"),
            s("a") + s("b"),
            dose / vc,
            1e-13,
        );
        if dose > 0.0 {
            close(
                &format!("{name} a/b"),
                s("a") / s("b"),
                s("w_alpha") / s("w_beta"),
                1e-13,
            );
            close(&format!("{name} auc_inf"), s("auc_inf"), dose / cl, 1e-14);
            close(
                &format!("{name} v_extrap"),
                s("v_extrap"),
                dose / s("b"),
                1e-13,
            );
            // MOD-2C-14: AUMC(0, inf) = D Vss / CL^2 + (D / CL) m, MRT = AUMC / AUC
            close(
                &format!("{name} mrt"),
                s("mrt"),
                s("aumc_inf") / s("auc_inf"),
                1e-13,
            );
            let m = s("mrt") - s("mrt_system");
            let expected_m = match case.model.as_str() {
                "pk2.iv_bolus" => 0.0,
                "pk2.iv_infusion" => case.parameters["dur"] / 2.0,
                "pk2.oral_0" | "pk2.oral_0_lag" => {
                    case.parameters["dur"] / 2.0
                        + case.parameters.get("tlag").copied().unwrap_or(0.0)
                }
                _ => {
                    1.0 / case.parameters["ka"]
                        + case.parameters.get("tlag").copied().unwrap_or(0.0)
                }
            };
            if expected_m == 0.0 {
                assert!(m.abs() <= 1e-13 * s("mrt"), "{name}");
            } else {
                close(&format!("{name} m"), m, expected_m, 1e-9);
            }
            if case.model == "pk2.iv_bolus" {
                close(
                    &format!("{name} c0"),
                    scalar(&c, "c0").unwrap(),
                    dose / vc,
                    1e-14,
                );
            }
            // areas are monotone and bounded by their limits; the concentration by Cmax
            let (mut prev_auc, mut prev_aumc) = (0.0, 0.0);
            for key in &case.time_keys {
                let (auc, aumc) = (grid(&c, key, "auc"), grid(&c, key, "aumc"));
                assert!(auc >= prev_auc && aumc >= prev_aumc, "{name} {key}");
                assert!(auc <= s("auc_inf") * (1.0 + 1e-12), "{name} {key}");
                assert!(aumc <= s("aumc_inf") * (1.0 + 1e-12), "{name} {key}");
                prev_auc = auc;
                prev_aumc = aumc;
                if let Some(cmax) = scalar(&c, "cmax_pred") {
                    assert!(
                        grid(&c, key, "conc") <= cmax * (1.0 + 1e-12),
                        "{name} {key}"
                    );
                }
            }
            if let (Some(tmax), Some(cmax)) = (scalar(&c, "tmax_pred"), scalar(&c, "cmax_pred")) {
                // Tmax is after the lag and, for the zero-order and infusion ids, at the end of the input
                assert!(
                    tmax > case.parameters.get("tlag").copied().unwrap_or(0.0),
                    "{name}"
                );
                assert!(cmax > 0.0, "{name}");
                if let Some(dur) = case.parameters.get("dur") {
                    let lag = case.parameters.get("tlag").copied().unwrap_or(0.0);
                    close(&format!("{name} tmax"), tmax, lag + dur, 1e-14);
                }
            }
        } else {
            // v_extrap = vc / w_beta does not depend on the dose
            for n in ["auc_inf", "aumc_inf", "a", "b"] {
                assert_eq!(s(n), 0.0, "{name} {n}");
            }
            for key in &case.time_keys {
                assert_eq!(grid(&c, key, "conc"), 0.0, "{name} {key}");
            }
        }
    }
}

// ---------------------------------------------------------------- one model, three parameterisations

fn case_named(name: &str) -> Pk2Case {
    load_pk2_case(&format!("model_pk2_{name}")).unwrap()
}

#[test]
fn parameterisations_of_one_point_give_one_curve() {
    let mut pairs = 0;
    for (id, point, ka_free) in [
        ("iv_bolus", "base", true),
        ("iv_bolus", "distribution", true),
        ("iv_bolus", "small_exchange", true),
        ("iv_bolus", "ab_ratio_1e3", false),
        ("iv_infusion", "base", true),
        ("iv_infusion", "distribution", true),
        ("oral_1", "base", true),
        ("oral_1_lag", "base", true),
        ("oral_0", "base", true),
        ("oral_0", "distribution", true),
    ] {
        let reference = case_named(&format!("{id}_{point}"));
        for set in ["micro", "macro"] {
            let name = format!("{id}_{point}_{set}");
            if !list_pk2_cases()
                .unwrap()
                .contains(&format!("model_pk2_{name}"))
            {
                assert!(!ka_free || point == "ab_ratio_1e3", "{name} missing");
                continue;
            }
            let other = case_named(&name);
            assert_eq!(other.parameterisation, set);
            pairs += 1;
            for (t, key) in reference.case.times.iter().zip(&reference.case.time_keys) {
                let other_key =
                    &other.case.time_keys[other.case.times.iter().position(|u| u == t).unwrap()];
                for q in ["conc", "auc", "aumc"] {
                    close(
                        &format!("{name} {key} {q}"),
                        grid(&other, other_key, q),
                        grid(&reference, key, q),
                        1e-11,
                    );
                }
            }
            for n in SCALARS {
                if n == "aumc_inf" || n == "mrt" || n == "auc_inf" || n == "a" || n == "b" {
                    // the same dose and the same model: equal too
                }
                close(
                    &format!("{name} {n}"),
                    scalar(&other, n).unwrap(),
                    scalar(&reference, n).unwrap(),
                    1e-11,
                );
            }
        }
    }
    assert!(pairs >= 14, "{pairs} pairs compared");
}

// ---------------------------------------------------------------- worked examples N1 to N8

fn digits(c: &Pk2Case, key: &str, quantity: &str, printed: f64, decimals: u32) {
    let k = format!("t={key}");
    let got = grid(c, &k, quantity);
    assert!(
        Tolerance::displayed(decimals).accepts(got, printed),
        "{} {quantity} at {key}: oracle {got:.14}, printed {printed}",
        c.case.name
    );
}

fn at_scalar(c: &Pk2Case, name: &str, printed: f64, decimals: u32) {
    let got = scalar(c, name).unwrap();
    assert!(
        Tolerance::displayed(decimals).accepts(got, printed),
        "{} {name}: oracle {got:.14}, printed {printed}",
        c.case.name
    );
}

#[test]
#[allow(clippy::approx_constant)] // ln 2, as printed in the spec
fn worked_example_n1_iv_bolus() {
    let c = case_named("iv_bolus_base");
    digits(&c, "1", "conc", 6.0652743089, 10);
    digits(&c, "4", "conc", 3.0809537540, 10);
    digits(&c, "12", "conc", 1.3386750763, 10);
    digits(&c, "24", "conc", 0.4031909037, 10);
    digits(&c, "0", "conc", 10.0, 10);
    digits(&c, "4", "auc", 20.1062444046, 10);
    digits(&c, "24", "auc", 45.9680909647, 10);
    at_scalar(&c, "half_life_alpha", 0.6931471806, 10);
    at_scalar(&c, "half_life", 6.9314718056, 10);
    for (n, v) in [
        ("auc_inf", 50.0),
        ("aumc_inf", 450.0),
        ("mrt", 9.0),
        ("vss", 18.0),
        ("vz", 20.0),
        ("v_extrap", 22.5),
        ("c0", 10.0),
        ("alpha", 1.0),
        ("beta", 0.1),
    ] {
        at_scalar(&c, n, v, 10);
    }
    at_scalar(&c, "w_alpha", 5.0 / 9.0, 10);
    at_scalar(&c, "a", 50.0 / 9.0, 10);
    at_scalar(&c, "b", 40.0 / 9.0, 10);
}

#[test]
fn worked_example_n2_iv_infusion_and_zero_order() {
    for id in ["iv_infusion_base", "oral_0_base"] {
        let c = case_named(id);
        digits(&c, "1", "conc", 3.8706144848, 10);
        digits(&c, "2", "conc", 6.4300519226, 10);
        digits(&c, "24", "conc", 0.4463378912, 10);
        digits(&c, "2", "auc", 7.3160986930, 10);
        digits(&c, "24", "auc", 45.5366210942, 10);
        at_scalar(&c, "tmax_pred", 2.0, 10);
        at_scalar(&c, "cmax_pred", 6.4300519226, 10);
        at_scalar(&c, "aumc_inf", 500.0, 10);
        at_scalar(&c, "mrt", 10.0, 10);
        at_scalar(&c, "auc_inf", 50.0, 10);
    }
    let c = case_named("iv_infusion_base");
    // C(5) of N2 is printed as 3.1037489142; 5 is on the grid of this case
    digits(&c, "5", "conc", 3.1037489142, 10);
}

#[test]
fn worked_example_n3_first_order_absorption() {
    let c = case_named("oral_1_base");
    digits(&c, "1", "conc", 6.1838339644, 10);
    digits(&c, "4", "conc", 3.3342105358, 10);
    digits(&c, "12", "conc", 1.4091639967, 10);
    digits(&c, "12", "auc", 35.9089744488, 10);
    digits(&c, "24", "auc", 45.7558852258, 10);
    at_scalar(&c, "a_oral", 11.1111111111, 10);
    at_scalar(&c, "b_oral", 4.6783625731, 10);
    at_scalar(&c, "tmax_pred", 0.9501291546, 10);
    at_scalar(&c, "cmax_pred", 6.1898890498, 10);
    at_scalar(&c, "mrt", 9.5, 10);
    at_scalar(&c, "auc_inf", 50.0, 10);
    // ka = alpha and ka = beta of N3
    let c = case_named("oral_1_ka_eq_alpha_macro");
    digits(&c, "1", "conc", 4.6954190034, 10);
    digits(&c, "4", "conc", 3.6267890476, 10);
    digits(&c, "12", "conc", 1.4877580966, 10);
    at_scalar(&c, "tmax_pred", 1.5350257231, 10);
    at_scalar(&c, "cmax_pred", 5.0089388633, 10);
    let c = case_named("oral_1_ka_eq_beta_macro");
    digits(&c, "1", "conc", 0.7336055048, 10);
    digits(&c, "4", "conc", 1.5941519381, 10);
    digits(&c, "12", "conc", 1.7922876905, 10);
    at_scalar(&c, "tmax_pred", 8.6170616924, 10);
    at_scalar(&c, "cmax_pred", 1.8785197816, 10);
}

#[test]
fn worked_example_n4_the_limits_and_their_neighbours() {
    let at3 = |name: &str, printed: f64| digits(&case_named(name), "3", "conc", printed, 12);
    at3("oral_1_ka_eq_alpha_macro", 4.242283990397);
    at3("oral_1_ka_alpha_plus_1e3", 4.242226751871);
    at3("oral_1_ka_alpha_plus_1e6", 4.242283933924);
    at3("oral_1_ka_alpha_plus_1e9", 4.242283990341);
    at3("oral_1_ka_eq_beta_macro", 1.414320067276);
    at3("oral_1_ka_beta_plus_1e3", 1.426062473717);
    at3("oral_1_ka_beta_plus_1e6", 1.414331830891);
    at3("oral_1_ka_beta_plus_1e9", 1.414320079039);
}

#[test]
fn worked_example_n5_lag_and_zero_order_with_lag() {
    let c = case_named("oral_1_lag_base");
    digits(&c, "0.5", "conc", 0.0, 10);
    digits(&c, "3", "conc", 4.4491793389, 10);
    at_scalar(&c, "tmax_pred", 1.4501291546, 10);
    at_scalar(&c, "cmax_pred", 6.1898890498, 10);
    at_scalar(&c, "mrt", 10.0, 10);
    let c = case_named("oral_0_lag_base");
    digits(&c, "3", "conc", 5.2885410903, 10);
    at_scalar(&c, "tmax_pred", 2.5, 10);
}

#[test]
fn worked_example_n6_nearly_equal_exponents() {
    let c = case_named("iv_bolus_near_degenerate_1e9_micro");
    digits(&c, "1", "conc", 8.187307523411, 12);
    digits(&c, "10", "conc", 1.353352832366, 12);
    let (alpha, beta) = (scalar(&c, "alpha").unwrap(), scalar(&c, "beta").unwrap());
    assert!(
        Tolerance::Relative {
            rel: 1e-9,
            abs: 0.0
        }
        .accepts(alpha - beta, 2.828427126514e-5),
        "alpha - beta = {:e}",
        alpha - beta
    );
    at_scalar(&c, "w_alpha", 0.500017677670, 12);
    at_scalar(&c, "w_beta", 0.499982322330, 12);
    // k12 = 1e-12: r = alpha - beta = 8.944271910005e-7 (the textbook S^2 - 4 k10 k21 gives 8.944110913856e-7)
    let e = case_named("iv_bolus_near_degenerate_1e12_micro");
    let r = scalar(&e, "alpha").unwrap() - scalar(&e, "beta").unwrap();
    assert!(
        Tolerance::Relative {
            rel: 1e-9,
            abs: 0.0
        }
        .accepts(r, 8.944271910005e-7),
        "alpha - beta = {r:e}"
    );
    let d = case_named("iv_bolus_k10_ne_k21_1e9_micro");
    at_scalar(&d, "alpha", 0.500000001667, 12);
    at_scalar(&d, "beta", 0.199999999333, 12);
    let wa = scalar(&d, "w_alpha").unwrap();
    assert!(
        Tolerance::Relative {
            rel: 1e-3,
            abs: 0.0
        }
        .accepts(wa, 5.556e-9),
        "{wa:e}"
    );
}

#[test]
fn worked_example_n7_conversions_both_ways() {
    // forward: clearance -> micro, macro
    let c = case_named("iv_bolus_base");
    for (n, v) in [
        ("k10", 0.2),
        ("k12", 0.4),
        ("k21", 0.5),
        ("alpha", 1.0),
        ("beta", 0.1),
    ] {
        at_scalar(&c, n, v, 12);
    }
    // reverse: macro (a, b, alpha, beta) and the dose -> clearance and micro
    let m = case_named("iv_bolus_base_macro");
    assert_eq!(m.parameterisation, "macro");
    for (n, v) in [
        ("vc", 10.0),
        ("k21", 0.5),
        ("k10", 0.2),
        ("k12", 0.4),
        ("cl", 2.0),
        ("q", 4.0),
        ("vp", 8.0),
    ] {
        at_scalar(&m, n, v, 12);
    }
    // micro -> clearance
    let u = case_named("iv_bolus_base_micro");
    assert_eq!(u.parameterisation, "micro");
    for (n, v) in [
        ("cl", 2.0),
        ("q", 4.0),
        ("vp", 8.0),
        ("alpha", 1.0),
        ("beta", 0.1),
    ] {
        at_scalar(&u, n, v, 12);
    }
    // the oral coefficients of N7 (ka = 2)
    let o = case_named("oral_1_base_macro");
    at_scalar(&o, "a_oral", 11.1111111111, 10);
    at_scalar(&o, "b_oral", 4.6783625731, 10);
}

#[test]
fn worked_example_n8_partial_derivatives() {
    let c = case_named("deriv_iv_bolus_base");
    assert_eq!(c.kind, Pk2Kind::Derivatives);
    for (n, v) in [
        ("d_cl", -0.5869035023),
        ("d_vc", -0.3130661305),
        ("d_q", -0.3180294080),
        ("d_vp", -0.0610860459),
    ] {
        let got = c.case.expected.get("t=1", n).flatten().unwrap();
        assert!(
            Tolerance::displayed(10).accepts(got, v),
            "{n}: oracle {got:.14}, printed {v}"
        );
    }
}

// ---------------------------------------------------------------- derivative grids

#[test]
fn the_derivative_grids_agree_with_central_differences_of_the_naive_forms() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for c in of_kind(Pk2Kind::Derivatives) {
        if !c.textbook_double_ok {
            continue;
        }
        let case = &c.case;
        // The textbook beta = (S - r) / 2 subtracts nearly equal numbers when alpha / beta is large
        // and its error is multiplied by 1 / h in a difference quotient.
        let separated = exponents(&c.parameterisation, &case.parameters, case.dose)
            .is_some_and(|(alpha, beta)| alpha / beta < 100.0);
        if !separated {
            continue;
        }
        let lag = case.parameters.get("tlag").copied().unwrap_or(0.0);
        let kinks = [
            lag,
            lag + case.parameters.get("dur").copied().unwrap_or(0.0),
        ];
        for (group, name, value) in case.expected.iter() {
            let expected = value.unwrap();
            let parameter = name.strip_prefix("d_").unwrap();
            let t = case.times[case.time_keys.iter().position(|k| k == group).unwrap()];
            let theta = case.parameters[parameter];
            // five-point stencil with a relative step of 1e-3: truncation of order (1e-3)^4 of the
            // fifth derivative, rounding 1e-16 * C / h
            let h = 1e-3 * theta.abs();
            // a stencil that straddles a kink (the lag, the end of a zero-order input) is not smooth
            let reach = match parameter {
                "tlag" | "dur" => 2.0 * h + 1e-12,
                _ => 1e-12,
            };
            if kinks.iter().any(|k| (t - k).abs() <= reach) {
                continue;
            }
            let eval = |x: f64| -> f64 {
                let mut p: BTreeMap<String, f64> = case.parameters.clone();
                p.insert(parameter.to_string(), x);
                conc_auc_aumc(&case.model, &c.parameterisation, &p, case.dose, t).unwrap()[0]
            };
            let fd = (-eval(theta + 2.0 * h) + 8.0 * eval(theta + h) - 8.0 * eval(theta - h)
                + eval(theta - 2.0 * h))
                / (12.0 * h);
            let c0 = eval(theta).abs();
            let allowed = 1e-7 * expected.abs() + 1e-10 * c0 / theta.abs();
            checked += 1;
            if (fd - expected).abs() > allowed {
                failures.push(format!(
                    "{} {name} at t = {t}: central difference {fd:e}, oracle {expected:e}",
                    case.name
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} derivatives differ:
{}",
        failures.len(),
        failures.iter().take(40).cloned().collect::<Vec<_>>().join(
            "
"
        )
    );
    eprintln!("{checked} derivative values compared with finite differences");
    assert!(checked >= 1000, "{checked} derivatives compared");
}

#[test]
fn the_oracle_directory_is_where_the_loaders_look() {
    assert!(pk2_dir().is_dir());
    assert!(pk2_dir().ends_with("models/pk2") || pk2_dir().ends_with("models\\pk2"));
}
