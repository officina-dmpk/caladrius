//! Unit tests: the worked examples of `specs/models.md` section 9 (printed to 10 decimals, compared
//! at that precision), serde, derivatives and error messages. The exact closed-form values are
//! checked by `tests/oracle_models.rs`.

use caladrius_testkit::Tolerance;

use crate::*;

fn input(model: ModelId, dose: f64, p: &[(&str, f64)], times: &[f64]) -> ModelInput {
    ModelInput {
        model,
        dose,
        params: p.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        times: times.to_vec(),
    }
}

fn out(model: ModelId, p: &[(&str, f64)], times: &[f64]) -> ModelOutput {
    run(&input(model, 100.0, p, times)).unwrap()
}

#[track_caller]
fn printed(actual: f64, expected: f64) {
    assert!(
        Tolerance::displayed(10).accepts(actual, expected),
        "expected {expected}, got {actual}"
    );
}

const VCL: [(&str, f64); 2] = [("v", 10.0), ("cl", 2.0)];

#[test]
fn m1_iv_bolus() {
    let r = out(ModelId::IvBolus, &VCL, &[1.0, 4.0, 12.0]);
    printed(r.conc()[0], 8.1873075308);
    printed(r.conc()[1], 4.4932896412);
    printed(r.conc()[2], 0.9071795329);
    printed(r.auc()[1], 27.5335517941);
    printed(r.auc()[2], 45.4641023355);
    printed(r.get("half_life").unwrap(), 3.4657359028);
    assert_eq!(r.get("c0"), Some(10.0));
    assert_eq!(r.get("auc_inf"), Some(50.0));
    printed(r.get("mrt").unwrap(), 5.0);
    assert_eq!(r.get("tmax_pred"), None);
}

#[test]
fn m2_first_order_absorption() {
    let p = [("v", 10.0), ("cl", 2.0), ("ka", 1.0)];
    let r = out(ModelId::Oral1, &p, &[0.5, 1.0, 2.0, 6.0, 12.0]);
    for (c, e) in r.conc().iter().zip([
        3.7288344790,
        5.6356413988,
        6.6873095350,
        3.7339432467,
        1.1338976135,
    ]) {
        printed(*c, e);
    }
    printed(r.auc()[3], 31.2063461577);
    printed(r.get("tmax_pred").unwrap(), 2.0117973905);
    printed(r.get("cmax_pred").unwrap(), 6.6874030498);
    printed(r.get("mrt").unwrap(), 6.0);
    // With a lag of 0.5 h.
    let lag = [("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", 0.5)];
    let r = out(ModelId::Oral1Lag, &lag, &[0.5, 1.5]);
    assert_eq!(r.conc()[0], 0.0);
    printed(r.conc()[1], 5.6356413988);
    printed(r.get("tmax_pred").unwrap(), 2.5117973905);
    printed(r.get("cmax_pred").unwrap(), 6.6874030498);
}

#[test]
fn m3_infusion_and_zero_order_with_lag() {
    let p = [("v", 10.0), ("cl", 2.0), ("dur", 2.0)];
    let r = out(ModelId::IvInfusion, &p, &[1.0, 2.0, 5.0, 1e6]);
    printed(r.conc()[0], 4.5317311731);
    printed(r.conc()[1], 8.2419988491);
    printed(r.conc()[2], 4.5233048731);
    printed(r.auc()[1], 8.7900057545);
    printed(r.auc()[3], 50.0);
    assert_eq!(r.get("tmax_pred"), Some(2.0));
    printed(r.get("cmax_pred").unwrap(), 8.2419988491);
    printed(r.get("mrt").unwrap(), 6.0);
    // pk1.oral_0 gives the same numbers.
    let o = out(ModelId::Oral0, &p, &[1.0, 2.0, 5.0, 1e6]);
    assert_eq!(o.conc(), r.conc());
    let lag = [("v", 10.0), ("cl", 2.0), ("dur", 2.0), ("tlag", 0.5)];
    let r = out(ModelId::Oral0Lag, &lag, &[3.0]);
    printed(r.conc()[0], 7.4576689581);
    assert_eq!(r.get("tmax_pred"), Some(2.5));
}

#[test]
fn m4_flip_flop_pair() {
    let a = out(
        ModelId::Oral1,
        &[("v", 10.0), ("k", 0.2), ("ka", 1.0)],
        &[1.0, 3.0],
    );
    let b = out(
        ModelId::Oral1,
        &[("v", 2.0), ("k", 1.0), ("ka", 0.2)],
        &[1.0, 3.0],
    );
    printed(a.conc()[0], 5.6356413988);
    printed(b.conc()[0], 5.6356413988);
    printed(a.conc()[1], 6.2378070966);
    printed(b.conc()[1], 6.2378070966);
}

#[test]
fn m5_ka_equal_and_close_to_k() {
    // The spec prints 3.6787944006 for ka = k + 1e-9; the exact value (T-009 oracle) is
    // 3.6787944209 (the printed one is the textbook form's lost digits).
    for (ka, expected) in [
        (0.2, 3.6787944117),
        (0.201, 3.6879607985),
        (0.200001, 3.6788036087),
        (0.200000001, 3.6787944209),
    ] {
        let r = out(
            ModelId::Oral1,
            &[("v", 10.0), ("k", 0.2), ("ka", ka)],
            &[5.0],
        );
        printed(r.conc()[0], expected);
    }
}

#[test]
fn a_zero_dose_gives_exact_zeros() {
    let r = run(&input(
        ModelId::Oral1Lag,
        0.0,
        &[("v", 10.0), ("cl", 2.0), ("ka", 1.0), ("tlag", 0.5)],
        &[0.0, 1.0, 10.0],
    ))
    .unwrap();
    assert!(r.conc().iter().chain(r.auc()).all(|&x| x == 0.0));
    assert_eq!(r.get("cmax_pred"), Some(0.0));
}

#[test]
fn values_are_never_negative_or_nan_at_extreme_times_and_rates() {
    // MOD-NUM-01: underflow to 0, no inf·0.
    for (p, model) in [
        (vec![("v", 2.0), ("k", 50.0), ("ka", 0.001)], ModelId::Oral1),
        (vec![("v", 2.0), ("k", 0.001), ("ka", 50.0)], ModelId::Oral1),
        (
            vec![("v", 2.0), ("k", 30.0), ("dur", 1e-6)],
            ModelId::IvInfusion,
        ),
    ] {
        let r = out(model, &p, &[1e-300, 1e-9, 1.0, 1e3, 1e9, 1e300]);
        for x in r.conc().iter().chain(r.auc()) {
            assert!(x.is_finite() && *x >= 0.0, "{model:?}: {x}");
        }
    }
}

#[test]
fn input_and_output_round_trip_through_json() {
    let i = input(
        ModelId::Oral0Lag,
        50.0,
        &[("v", 10.0), ("cl", 2.0), ("dur", 2.0), ("tlag", 0.5)],
        &[0.0, 1.0, 3.0],
    );
    let text = serde_json::to_string(&i).unwrap();
    assert!(text.contains(r#""model":"pk1.oral_0_lag""#), "{text}");
    assert_eq!(serde_json::from_str::<ModelInput>(&text).unwrap(), i);
    let o = run(&i).unwrap();
    let back: ModelOutput = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
    assert_eq!(back, o);
    assert!(
        serde_json::from_str::<ModelInput>(
            r#"{"model":"pk1.oral_9","dose":1,"params":{},"times":[]}"#
        )
        .is_err()
    );
    for m in ModelId::ALL {
        assert_eq!(ModelId::from_id(m.id()), Some(m));
    }
}

#[test]
fn error_messages_say_what_to_fix() {
    let e = run(&input(ModelId::IvBolus, 1.0, &[("v", 10.0)], &[])).unwrap_err();
    assert!(e.to_string().contains("cl or k"), "{e}");
    let e = run(&input(ModelId::IvBolus, 1.0, &VCL, &[0.0, f64::NAN])).unwrap_err();
    assert_eq!(e, ModelError::NonFiniteTime { index: 1 });
    assert!(e.to_string().contains("time 2"), "{e}");
    let e = run(&input(
        ModelId::IvInfusion,
        1.0,
        &[("v", 10.0), ("cl", 2.0), ("dur", -1.0)],
        &[],
    ))
    .unwrap_err();
    assert!(e.to_string().contains("`dur` = -1"), "{e}");
    // k derived from cl / v must stay usable.
    let e = run(&input(
        ModelId::IvBolus,
        1.0,
        &[("v", 1e300), ("cl", 1e-300)],
        &[],
    ))
    .unwrap_err();
    assert!(e.to_string().contains("`k`"), "{e}");
}

// ---- Derivatives (specs/fit.md FIT-JAC-01, JAC-02) ----

#[test]
fn analytic_bolus_derivatives_match_forward_differences() {
    let times = [-1.0, 0.0, 1.0, 4.0, 12.0];
    for p in [VCL.to_vec(), vec![("v", 10.0), ("k", 0.2)]] {
        let i = input(ModelId::IvBolus, 100.0, &p, &times);
        let exact = jacobian(&i, Derivatives::Analytic).unwrap();
        let fd = jacobian(&i, Derivatives::ForwardDifference { increment: 1e-7 }).unwrap();
        assert_eq!(exact.method, Derivatives::Analytic);
        assert_eq!(exact.parameters, fd.parameters);
        for (a, b) in exact
            .columns
            .iter()
            .flatten()
            .zip(fd.columns.iter().flatten())
        {
            assert!((a - b).abs() <= 1e-5 * (1.0 + a.abs()), "{a} against {b}");
        }
    }
}

#[test]
fn forward_differences_use_a_relative_increment() {
    // C = (D/V)·e^(−kt) with (V, k): the forward difference in k with Δ = 0.001·0.2 is
    // C·(e^(−Δ·t) − 1)/Δ exactly.
    let i = input(ModelId::IvBolus, 100.0, &[("v", 10.0), ("k", 0.2)], &[4.0]);
    let j = jacobian(
        &i,
        Derivatives::ForwardDifference {
            increment: DEFAULT_INCREMENT,
        },
    )
    .unwrap();
    let k_column = &j.columns[j.parameters.iter().position(|n| n == "k").unwrap()];
    let c = 10.0 * (-0.8f64).exp();
    let delta = 0.001 * 0.2;
    let expected = c * ((-delta * 4.0f64).exp() - 1.0) / delta;
    assert!((k_column[0] - expected).abs() <= 1e-9 * expected.abs());
    // Analytic falls back to forward differences for a model without spec'd derivatives.
    let oral = input(
        ModelId::Oral1,
        100.0,
        &[("v", 10.0), ("k", 0.2), ("ka", 1.0)],
        &[1.0],
    );
    assert_eq!(
        jacobian(&oral, Derivatives::Analytic).unwrap().method,
        Derivatives::ForwardDifference { increment: 0.001 }
    );
    assert!(jacobian(&oral, Derivatives::ForwardDifference { increment: 0.0 }).is_err());
}

// ---- T-010 review fixes ----

#[test]
fn huge_times_and_rates_give_zero_not_nan() {
    // MOD-NUM-01: k·s overflowing with e^−as = 0 must not give inf·0.
    for (p, t) in [
        (vec![("v", 10.0), ("k", 1.0), ("ka", 2.0)], 1e308),
        (vec![("v", 10.0), ("k", 1e10), ("ka", 2e10)], 1e300),
        (vec![("v", 10.0), ("k", 2e10), ("ka", 1e10)], 1e300),
    ] {
        for model in [ModelId::Oral1, ModelId::Oral1Lag] {
            let mut p = p.clone();
            if model == ModelId::Oral1Lag {
                p.push(("tlag", 0.5));
            }
            let r = out(model, &p, &[t]);
            assert_eq!(r.conc()[0], 0.0, "{model:?} {p:?}");
            printed(r.auc()[0], 100.0 / (10.0 * p[1].1));
        }
    }
}

#[test]
fn overflowing_results_are_an_error() {
    for (dose, p) in [
        (1e10, vec![("v", 1e-300), ("k", 1.0)]),
        (100.0, vec![("v", 10.0), ("cl", 1e-320)]),
    ] {
        let e = run(&input(ModelId::IvBolus, dose, &p, &[0.0, 1.0])).unwrap_err();
        assert!(matches!(e, ModelError::Overflow { .. }), "{e:?}");
        assert!(e.to_string().contains("rescale"), "{e}");
    }
    // A forward-difference step that overflows names the parameter (k + 0.5 makes CL = V·k inf).
    let i = input(
        ModelId::IvBolus,
        1.0,
        &[("v", f64::MAX), ("k", 1.0)],
        &[1.0],
    );
    let e = jacobian(&i, Derivatives::ForwardDifference { increment: 0.5 }).unwrap_err();
    assert!(
        e.to_string()
            .contains("forward-difference step of parameter `k`"),
        "{e}"
    );
}

#[test]
fn peak_time_when_ka_is_much_smaller_than_k() {
    // Tmax = ln(ka/k)/(ka − k): ln(1e10)/(1 − 1e-10) and ln(1e17)/1.
    let tmax = |ka: f64| {
        out(ModelId::Oral1, &[("v", 10.0), ("k", 1.0), ("ka", ka)], &[])
            .get("tmax_pred")
            .unwrap()
    };
    let t10 = 23.025850929940457 / (1.0 - 1e-10);
    assert!(
        Tolerance::MODEL_VALUES.accepts(tmax(1e-10), t10),
        "{}",
        tmax(1e-10)
    );
    let t17 = 17.0 * std::f64::consts::LN_10;
    assert!(
        Tolerance::MODEL_VALUES.accepts(tmax(1e-17), t17),
        "{}",
        tmax(1e-17)
    );
}

#[test]
fn non_finite_values_in_errors_and_options_round_trip() {
    let e = ModelError::ParameterOutOfDomain {
        name: "v".to_string(),
        value: f64::NAN,
        domain: "a finite number > 0".to_string(),
    };
    let text = serde_json::to_string(&e).unwrap();
    assert!(text.contains(r#""value":"NaN""#), "{text}");
    match serde_json::from_str::<ModelError>(&text).unwrap() {
        ModelError::ParameterOutOfDomain { value, .. } => assert!(value.is_nan()),
        other => panic!("{other:?}"),
    }
    let d = Derivatives::ForwardDifference {
        increment: f64::INFINITY,
    };
    let back: Derivatives = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(back, d);
    let e = ModelError::InvalidDose {
        value: f64::NEG_INFINITY,
    };
    assert_eq!(
        serde_json::from_str::<ModelError>(&serde_json::to_string(&e).unwrap()).unwrap(),
        e
    );
}
