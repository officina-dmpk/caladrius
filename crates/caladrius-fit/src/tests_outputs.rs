//! Unit tests of task T-011b: initial estimates by curve stripping (worked example F4), fixed
//! parameters, bounds and the at-bound status, partial derivatives at the solution, quality flags.

use std::collections::BTreeMap;

use caladrius_testkit::Tolerance;

use crate::*;

/// Worked example F4: exact concentrations of D = 100, V = 10, k = 0.2, ka = 1.
const F4_T: [f64; 8] = [0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 24.0];

fn f4_conc() -> Vec<f64> {
    F4_T.iter()
        .map(|t| 12.5 * ((-0.2 * t).exp() - (-t).exp()))
        .collect()
}

#[track_caller]
fn close(actual: f64, expected: f64) {
    assert!(
        Tolerance::FIT_PARAMETERS.accepts(actual, expected),
        "expected {expected}, got {actual}"
    );
}

fn none() -> BTreeMap<String, f64> {
    BTreeMap::new()
}

#[test]
fn f4_curve_stripping() {
    let e = initial_estimates(ModelId::Oral1, 100.0, &F4_T, &f4_conc(), &none()).unwrap();
    close(e["k"], 0.19968599);
    close(e["ka"], 1.0370213);
    close(e["v"], 9.9716735);
    assert_eq!(e.len(), 3);
    // With a lag: the two lines meet at 0.0347 h.
    let e = initial_estimates(ModelId::Oral1Lag, 100.0, &F4_T, &f4_conc(), &none()).unwrap();
    assert!(
        Tolerance::displayed(4).accepts(e["tlag"], 0.0347),
        "{}",
        e["tlag"]
    );
    // A fixed lag is used, not estimated.
    let fixed = BTreeMap::from([("tlag".to_string(), 0.0)]);
    let e = initial_estimates(ModelId::Oral1Lag, 100.0, &F4_T, &f4_conc(), &fixed).unwrap();
    assert!(!e.contains_key("tlag"));
    close(e["v"], 9.9716735);
}

fn fit(model: ModelId, time: &[f64], conc: &[f64], options: FitOptions) -> FitResult {
    run(&FitInput {
        model,
        dose: 100.0,
        time: time.to_vec(),
        conc: conc.to_vec(),
        weighting: Weighting::Uniform,
        initial: BTreeMap::new(),
        options: FitOptions {
            derivatives: Derivatives::Analytic,
            convergence: 1e-10,
            max_iterations: 200,
            ..options
        },
    })
    .unwrap()
}

#[test]
fn fits_start_from_generated_estimates() {
    // FIT-INI-01: no initial estimates, the fit starts from curve stripping and reaches the truth.
    let r = fit(ModelId::Oral1, &F4_T, &f4_conc(), FitOptions::default());
    assert_eq!(r.status(), FitStatus::Converged);
    for (name, value) in [
        ("estimate.v", 10.0),
        ("estimate.k", 0.2),
        ("estimate.ka", 1.0),
    ] {
        close(r.get(name).unwrap(), value);
    }
    // IV bolus: the spec's five points reach the F2 minimum.
    let r = fit(
        ModelId::IvBolus,
        &[0.5, 1.0, 2.0, 4.0, 8.0],
        &[9.31, 7.92, 6.85, 4.31, 2.11],
        FitOptions::default(),
    );
    close(r.get("estimate.v").unwrap(), 9.9186407);
    // Infusion of 2 h (dur fixed): V = 10, CL = 2 from exact data (MOD-IVI-01).
    let t = [0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0];
    let c: Vec<f64> = t
        .iter()
        .map(|&t: &f64| {
            let during = 25.0 * (1.0 - (-0.2 * t.min(2.0)).exp());
            if t <= 2.0 {
                during
            } else {
                during * (-0.2 * (t - 2.0)).exp()
            }
        })
        .collect();
    let options = FitOptions {
        fixed: BTreeMap::from([("dur".to_string(), 2.0)]),
        derivatives: Derivatives::ForwardDifference,
        ..FitOptions::default()
    };
    let r = run(&FitInput {
        model: ModelId::IvInfusion,
        dose: 100.0,
        time: t.to_vec(),
        conc: c.clone(),
        weighting: Weighting::Uniform,
        initial: BTreeMap::new(),
        options: FitOptions {
            convergence: 1e-10,
            max_iterations: 200,
            ..options
        },
    })
    .unwrap();
    close(r.get("estimate.v").unwrap(), 10.0);
    close(r.get("estimate.k").unwrap(), 0.2);
    assert_eq!(r.get("estimate.dur"), None);
    // Without the duration: a readable error.
    let e = initial_estimates(ModelId::IvInfusion, 100.0, &t, &c, &none()).unwrap_err();
    assert!(e.to_string().contains("`dur`"), "{e}");
    // Too few points for a terminal phase.
    let e = initial_estimates(
        ModelId::Oral1,
        100.0,
        &[1.0, 2.0, 3.0],
        &[1.0, 3.0, 2.0],
        &none(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("enter initial estimates"), "{e}");
}

fn spec_input() -> FitInput {
    FitInput {
        model: ModelId::IvBolus,
        dose: 100.0,
        time: vec![0.5, 1.0, 2.0, 4.0, 8.0],
        conc: vec![9.31, 7.92, 6.85, 4.31, 2.11],
        weighting: Weighting::Uniform,
        initial: BTreeMap::from([("v".to_string(), 12.0), ("k".to_string(), 0.15)]),
        options: FitOptions {
            derivatives: Derivatives::Analytic,
            convergence: 1e-10,
            max_iterations: 200,
            ..FitOptions::default()
        },
    }
}

#[test]
fn a_parameter_held_on_a_bound() {
    // The unconstrained minimum has k = 0.2041; with k <= 0.19 the fit ends on the bound.
    let mut i = spec_input();
    i.options.bounds = BTreeMap::from([(
        "k".to_string(),
        Bounds {
            lower: None,
            upper: Some(0.19),
        },
    )]);
    let r = run(&i).unwrap();
    assert_eq!(r.status(), FitStatus::AtBound);
    assert_eq!(r.get("estimate.k"), Some(0.19));
    assert_eq!(r.get("se.k"), None);
    assert!(r.get("se.v").is_some());
    assert_eq!(r.get("p"), Some(1.0));
    assert!(r.flags().iter().any(|f| matches!(
        f,
        FitFlag::AtBound { parameter, bound } if parameter == "k" && *bound == 0.19
    )));
    // V is then the best V for k = 0.19 (one-parameter least squares).
    let v = r.get("estimate.v").unwrap();
    let wrss = |v: f64| -> f64 {
        i.time
            .iter()
            .zip(&i.conc)
            .map(|(t, y)| (y - 100.0 / v * (-0.19 * t).exp()).powi(2))
            .sum()
    };
    assert!(wrss(v) <= wrss(v * 1.0001) && wrss(v) <= wrss(v * 0.9999));
}

#[test]
fn bounds_and_fixed_parameters_are_checked() {
    let mut i = spec_input();
    i.options.bounds = BTreeMap::from([(
        "v".to_string(),
        Bounds {
            lower: Some(13.0),
            upper: None,
        },
    )]);
    assert!(matches!(
        run(&i),
        Err(FitError::InitialOutsideBounds { parameter, .. }) if parameter == "v"
    ));
    let mut i = spec_input();
    i.options.bounds = BTreeMap::from([("ka".to_string(), Bounds::default())]);
    assert!(matches!(run(&i), Err(FitError::InvalidOption { option, .. }) if option == "bounds"));
    let mut i = spec_input();
    i.options.fixed = BTreeMap::from([("k".to_string(), 0.2)]);
    assert!(matches!(run(&i), Err(FitError::InvalidOption { option, .. }) if option == "fixed"));
    // Fixing k fits V alone.
    let mut i = spec_input();
    i.initial.remove("k");
    i.options.fixed = BTreeMap::from([("k".to_string(), 0.2)]);
    i.options.derivatives = Derivatives::ForwardDifference;
    let r = run(&i).unwrap();
    assert_eq!(r.parameters(), &["v".to_string()]);
    assert_eq!(r.get("estimate.k"), None);
}

#[test]
fn partial_derivatives_at_the_solution() {
    // F2: analytic derivatives at the minimum.
    let r = run(&spec_input()).unwrap();
    let dv = [-0.9178783, -0.8288474, -0.6758548, -0.4493774, -0.1986674];
    let dk = [
        -4.5520527,
        -8.2210391,
        -13.4071227,
        -17.8288499,
        -15.7640869,
    ];
    let p = r.partials();
    for (a, e) in p[0].iter().zip(dv).chain(p[1].iter().zip(dk)) {
        close(*a, e);
    }
    // Uniform weights: the weighted table is the same.
    assert_eq!(r.weighted_partials(), p.to_vec());
}

#[test]
fn flags_of_a_clean_and_of_a_poor_fit() {
    assert!(run(&spec_input()).unwrap().flags().is_empty());
    // Two points, two parameters: no degrees of freedom.
    let mut i = spec_input();
    i.time.truncate(2);
    i.conc.truncate(2);
    let r = run(&i).unwrap();
    assert!(
        r.flags()
            .iter()
            .any(|f| matches!(f, FitFlag::FewDegreesOfFreedom { value: 0, .. }))
    );
    // Not converged.
    let mut i = spec_input();
    i.options.max_iterations = 1;
    i.options.convergence = 0.0;
    let r = run(&i).unwrap();
    assert!(
        r.flags()
            .iter()
            .any(|f| matches!(f, FitFlag::NotConverged { .. }))
    );
    for flag in r.flags() {
        assert!(!flag.to_string().is_empty());
    }
    // Flags round-trip through JSON with the result.
    let back: FitResult = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back.flags(), r.flags());
}

#[test]
fn new_options_round_trip_and_refuse_typos() {
    let o = FitOptions {
        fixed: BTreeMap::from([("dur".to_string(), 2.0)]),
        bounds: BTreeMap::from([(
            "k".to_string(),
            Bounds {
                lower: Some(0.01),
                upper: Some(5.0),
            },
        )]),
        ..FitOptions::default()
    };
    let text = serde_json::to_string(&o).unwrap();
    assert_eq!(serde_json::from_str::<FitOptions>(&text).unwrap(), o);
    assert!(serde_json::from_str::<Bounds>(r#"{"lowr":1}"#).is_err());
    assert!(serde_json::from_str::<FlagThresholds>(r#"{"max_cv":1}"#).is_err());
}
