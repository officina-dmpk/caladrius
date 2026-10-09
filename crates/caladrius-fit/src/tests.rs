//! Unit tests: worked examples F1 to F3 of `specs/fit.md` section 11 (printed to 7 or 8
//! significant digits, compared within `Tolerance::FIT_PARAMETERS`), the fixed point of
//! predicted-value weights checked by an independent computation of the stationarity equations,
//! statuses and serde.

use std::collections::BTreeMap;

use caladrius_testkit::Tolerance;

use crate::*;

const T: [f64; 5] = [0.5, 1.0, 2.0, 4.0, 8.0];
const Y: [f64; 5] = [9.31, 7.92, 6.85, 4.31, 2.11];

/// The worked examples of `specs/fit.md` section 11 are stated with the stop ε = 1e-4 of the
/// reference conventions (T-040 kept them as the preset).
fn spec(weighting: Weighting, tight: bool) -> FitInput {
    let mut options = FitOptions {
        derivatives: Derivatives::Analytic,
        ..FitOptions::reference_conventions()
    };
    if tight {
        options.convergence = 1e-10;
        options.max_iterations = 200;
    }
    FitInput {
        model: ModelId::IvBolus,
        dose: 100.0,
        time: T.to_vec(),
        conc: Y.to_vec(),
        weighting,
        initial: BTreeMap::from([("v".to_string(), 12.0), ("k".to_string(), 0.15)]),
        options,
    }
}

#[track_caller]
fn close(r: &FitResult, name: &str, expected: f64) {
    let x = r
        .get(name)
        .unwrap_or_else(|| panic!("{name} is not calculated"));
    assert!(
        Tolerance::FIT_PARAMETERS.accepts(x, expected),
        "{name}: expected {expected}, got {x}"
    );
}

#[test]
fn f1_trace_matches_the_printed_iterations() {
    // WRSS and estimates printed with 7 significant digits or more; compared at that precision.
    let r = run(&spec(Weighting::Uniform, false)).unwrap();
    assert_eq!(r.status(), FitStatus::Converged);
    let expected = [
        (3.7383091, [12.0, 0.15]),
        (0.4285909, [9.6163537, 0.2032666]),
        (0.1957952, [9.9096786, 0.2040143]),
        (0.1955901388, [9.9186464, 0.2040567]),
        (0.1955901382, [9.9186411, 0.2040577]),
    ];
    assert_eq!(r.trace().len(), expected.len());
    for (row, (wrss, est)) in r.trace().iter().zip(expected) {
        assert!((row.wrss - wrss).abs() <= 5e-8, "{} {}", row.wrss, wrss);
        for (a, e) in row.estimates.iter().zip(est) {
            assert!((a - e).abs() <= 5e-7 * e.abs().max(1.0), "{a} against {e}");
        }
        // Full Gauss-Newton steps, no damping (the halving is only a safeguard).
        assert!(row.step.is_none_or(|s| s == 1.0));
        assert_eq!(row.lambda, 0.0);
    }
}

#[test]
fn f2_all_statistics_uniform() {
    let r = run(&spec(Weighting::Uniform, true)).unwrap();
    for (name, value) in [
        ("estimate.v", 9.9186407),
        ("estimate.k", 0.20405768),
        ("wrss", 0.19559014),
        ("s", 0.25533647),
        ("df", 3.0),
        ("ss_weighted", 219.3532),
        ("ss_corrected", 33.3032),
        ("corr_obs_pred", 0.9970964),
        ("aic", -4.1586697),
        ("sbc", -4.9397939),
        ("covariance.v.v", 0.06143111),
        ("covariance.v.k", -0.00229674),
        ("covariance.k.k", 0.00016400),
        ("se.v", 0.24785299),
        ("se.k", 0.01280633),
        ("cv_percent.v", 2.49886),
        ("cv_percent.k", 6.27584),
        ("correlation.v.k", -0.7235920),
        ("eigenvalue.1", 1.7235920),
        ("eigenvalue.2", 0.2764080),
        ("condition_number", 6.2356795),
        ("kappa_jacobian", 2.4971343),
        ("ci_lo.v", 9.1298619),
        ("ci_hi.v", 10.7074196),
        ("ci_lo.k", 0.16330221),
        ("ci_hi.k", 0.24481315),
        ("planar_lo.v", 8.8353165),
        ("planar_hi.v", 11.0019650),
        ("planar_lo.k", 0.14808332),
        ("planar_hi.k", 0.26003204),
        ("estimate.cl", 2.0239749),
        ("se.cl", 0.0969293),
        ("estimate.half_life", 3.3968198),
        ("se.half_life", 0.2131790),
    ] {
        close(&r, name, value);
    }
    assert_eq!(r.observations().len(), 5);
    assert_eq!(r.curve().len(), 1000);
    assert_eq!(r.curve().first().map(|p| p.time), Some(0.5));
    assert_eq!(r.curve().last().map(|p| p.time), Some(8.0));
}

#[test]
fn f3_weighted_fit_inv_y2() {
    let r = run(&spec(Weighting::InvY2, true)).unwrap();
    for (name, value) in [
        ("estimate.v", 10.10481932),
        ("estimate.k", 0.19590905),
        ("wrss", 0.005450726),
        ("s", 0.04262521),
        ("se.v", 0.29449444),
        ("se.k", 0.00710790),
        ("correlation.v.k", -0.7561139),
        ("ci_lo.v", 9.1676066),
        ("ci_hi.v", 11.0420321),
        ("estimate.cl", 1.9796255),
        ("se.cl", 0.0471264),
        ("estimate.half_life", 3.5381071),
        ("se.half_life", 0.1283683),
        ("ss_weighted", 5.0),
        ("ss_corrected", 1.3984729),
        ("aic", -22.0600324),
        ("sbc", -22.8411565),
    ] {
        close(&r, name, value);
    }
}

/// FIT-WGT-03: with predicted-value weights the solution is a fixed point, J(θ)ᵀ·W(θ)·r(θ) = 0
/// with W from the predictions at θ. Checked with an independent evaluation of the bolus model.
#[test]
fn predicted_value_weights_reach_a_fixed_point() {
    for (weighting, power) in [(Weighting::InvYhat, 1), (Weighting::InvYhat2, 2)] {
        let r = run(&spec(weighting, true)).unwrap();
        assert_eq!(r.status(), FitStatus::Converged);
        let (v, k) = (r.get("estimate.v").unwrap(), r.get("estimate.k").unwrap());
        let (mut gv, mut gk, mut scale) = (0.0, 0.0, 0.0);
        for (&t, &y) in T.iter().zip(&Y) {
            let f = 100.0 / v * (-k * t).exp();
            let w = 1.0 / f.powi(power);
            gv += (-f / v) * w * (y - f);
            gk += (-t * f) * w * (y - f);
            scale += w * y * y;
        }
        let scale = scale.sqrt();
        assert!(
            gv.abs() / scale < 1e-8 && gk.abs() / scale < 1e-8,
            "{gv} {gk}"
        );
    }
}

#[test]
fn statuses_and_options() {
    // A start far away with one iteration: the iteration limit, never convergence.
    let mut i = spec(Weighting::Uniform, false);
    i.options.max_iterations = 1;
    i.options.convergence = 0.0;
    assert_eq!(run(&i).unwrap().status(), FitStatus::MaxIterations);
    // Relative-offset criterion also converges to the same minimum.
    let mut i = spec(Weighting::Uniform, false);
    i.options.criterion = Criterion::RelativeOffset;
    i.options.convergence = 1e-6;
    let r = run(&i).unwrap();
    assert_eq!(r.status(), FitStatus::Converged);
    close(&r, "estimate.v", 9.9186407);
    // Forward differences (the reference conventions) reach the same minimum.
    let mut i = spec(Weighting::Uniform, true);
    i.options.derivatives = Derivatives::ForwardDifference;
    close(&run(&i).unwrap(), "estimate.k", 0.20405768);
    // Analytic derivatives for a model without them: a readable error.
    let mut i = spec(Weighting::Uniform, false);
    i.initial = BTreeMap::from([("v".to_string(), 12.0), ("cl".to_string(), 1.8)]);
    let e = run(&i).unwrap_err();
    assert!(e.to_string().contains("forward differences"), "{e}");
    // Invalid options.
    for (name, options) in [
        (
            "increment",
            FitOptions {
                increment: 0.0,
                ..FitOptions::default()
            },
        ),
        (
            "convergence",
            FitOptions {
                convergence: f64::NAN,
                ..FitOptions::default()
            },
        ),
        (
            "confidence_level",
            FitOptions {
                confidence_level: 1.0,
                ..FitOptions::default()
            },
        ),
    ] {
        let mut i = spec(Weighting::Uniform, false);
        i.options = options;
        match run(&i) {
            Err(FitError::InvalidOption { option, .. }) => assert_eq!(option, name),
            other => panic!("{name}: {other:?}"),
        }
    }
    // Observations before the dose.
    let mut i = spec(Weighting::Uniform, false);
    i.time[0] = -0.5;
    assert!(matches!(
        run(&i),
        Err(FitError::TimeBeforeDose { index: 0, .. })
    ));
}

#[test]
fn input_and_result_round_trip_through_json() {
    let i = spec(Weighting::InvYhat, false);
    let text = serde_json::to_string(&i).unwrap();
    assert!(text.contains(r#""weighting":"inv_yhat""#), "{text}");
    assert_eq!(serde_json::from_str::<FitInput>(&text).unwrap(), i);
    let r = run(&i).unwrap();
    let back: FitResult = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back, r);
    assert!(serde_json::from_str::<FitOptions>(r#"{"max_iteration":3}"#).is_err());
    let o: FitOptions = serde_json::from_str(r#"{"max_iterations":3}"#).unwrap();
    assert_eq!(o.max_iterations, 3);
    assert_eq!(o.increment, 1e-5);
}

// ---- T-011a review fixes ----

#[test]
fn options_are_bounded() {
    for (name, options) in [
        (
            "n_curve",
            FitOptions {
                n_curve: 1 << 40,
                ..FitOptions::default()
            },
        ),
        (
            "n_curve",
            FitOptions {
                n_curve: 1,
                ..FitOptions::default()
            },
        ),
        (
            "convergence",
            FitOptions {
                convergence: 1.0,
                ..FitOptions::default()
            },
        ),
        (
            "max_iterations",
            FitOptions {
                max_iterations: 1_000_000,
                ..FitOptions::default()
            },
        ),
        (
            "increment",
            FitOptions {
                increment: 0.5,
                ..FitOptions::default()
            },
        ),
    ] {
        let mut i = spec(Weighting::Uniform, false);
        i.options = options;
        match run(&i) {
            Err(FitError::InvalidOption { option, .. }) => assert_eq!(option, name),
            other => panic!("{name}: {other:?}"),
        }
    }
    // From JSON too, before anything is allocated.
    let mut i = spec(Weighting::Uniform, false);
    i.options = serde_json::from_str(r#"{"n_curve":1099511627776}"#).unwrap();
    assert!(run(&i).is_err());
    let mut i = spec(Weighting::Uniform, false);
    i.options.n_curve = 0;
    assert!(run(&i).unwrap().curve().is_empty());
}

#[test]
fn weights_must_be_finite() {
    // 1/y² overflows for y = 1e-200: refused by name, not a misleading `singular`.
    let mut i = spec(Weighting::InvY2, false);
    i.conc[4] = 1e-200;
    match run(&i) {
        Err(FitError::UnweightableObservation { index, .. }) => assert_eq!(index, 4),
        other => panic!("{other:?}"),
    }
    let mut i = spec(Weighting::InvY, false);
    i.conc[4] = 1e-200;
    assert!(run(&i).is_ok());
}

/// A model that predicts like the IV bolus but defines no secondary parameter.
struct Plain;

impl FitModel for Plain {
    fn predict(
        &self,
        dose: f64,
        params: &BTreeMap<String, f64>,
        times: &[f64],
    ) -> Result<Vec<f64>, caladrius_models::ModelError> {
        ModelId::IvBolus.predict(dose, params, times)
    }

    fn analytic_derivatives(
        &self,
        dose: f64,
        params: &BTreeMap<String, f64>,
        times: &[f64],
        names: &[String],
    ) -> Option<Vec<Vec<f64>>> {
        ModelId::IvBolus.analytic_derivatives(dose, params, times, names)
    }
}

#[test]
fn secondary_parameters_come_from_the_model() {
    let i = spec(Weighting::Uniform, true);
    let custom = run_model(&Plain, &i).unwrap();
    close(&custom, "estimate.v", 9.9186407);
    assert_eq!(custom.get("estimate.cl"), None);
    assert_eq!(custom.get("se.half_life"), None);
    close(&run(&i).unwrap(), "estimate.cl", 2.0239749);
}

#[test]
fn an_exact_fit_has_no_information_criteria_and_a_readable_trace() {
    let mut i = spec(Weighting::Uniform, true);
    // Exact data on V = 10, k = 0.2, more points than parameters.
    i.conc = T.iter().map(|t| 10.0 * (-0.2 * t).exp()).collect();
    let r = run(&i).unwrap();
    close(&r, "estimate.k", 0.2);
    assert_eq!(r.get("aic"), None);
    assert_eq!(r.get("sbc"), None);
    let back: FitResult = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back.trace(), r.trace());
}

#[test]
fn a_far_start_under_predicted_value_weights_is_not_converged() {
    // T-011a re-review: from (500, 3) the points with ŷ ~ 1e-17 carry weights ~ 1e16; no step
    // decreases WRSS and the predicted gain is tiny relative to it, but this is not a minimum.
    let times = [0.5, 1.0, 2.0, 4.0, 8.0, 12.0, 24.0];
    let conc: Vec<f64> = times
        .iter()
        .zip([1.02, 0.98, 1.01, 0.99, 1.03, 0.97, 1.0])
        .map(|(t, noise): (&f64, f64)| 10.0 * (-0.4 * t).exp() * noise)
        .collect();
    let mut options = FitOptions {
        derivatives: Derivatives::Analytic,
        ..FitOptions::default()
    };
    options.max_iterations = 50;
    let input = FitInput {
        model: ModelId::IvBolus,
        dose: 100.0,
        time: times.to_vec(),
        conc,
        weighting: Weighting::InvYhat,
        initial: BTreeMap::from([("v".to_string(), 500.0), ("k".to_string(), 3.0)]),
        options,
    };
    let r = run(&input).unwrap();
    let start = r.trace()[0].wrss;
    let wrss = r.get("wrss").unwrap();
    if matches!(r.status(), FitStatus::Converged | FitStatus::AtBound) {
        // Only acceptable if it actually moved to the solution.
        assert!(r.trace().len() > 1, "converged at the initial estimates");
        assert!(wrss <= start, "{wrss} > {start}");
        assert!(
            (r.get("estimate.v").unwrap() - 10.0).abs() < 1.0,
            "{:?} {:?}",
            r.status(),
            r.values()
        );
    }
    // Whatever the status, the result is never worse than the start (T-011b review).
    assert!(wrss <= start, "{:?}: {wrss} > {start}", r.status());
}

/// A dosing regimen among the fixed parameters (here two doses 24 h apart, of which only the first
/// has been given at the observed times): closed-form derivatives are refused with a message that
/// names the regimen, and forward differences fit it as the single dose.
#[test]
fn analytic_derivatives_are_refused_for_a_regimen() {
    let mut input = spec(Weighting::Uniform, true);
    input.options.fixed = BTreeMap::from([("tau".to_string(), 24.0), ("n_doses".to_string(), 2.0)]);
    let e = run(&input).unwrap_err();
    assert!(
        matches!(&e, FitError::RegimenDerivativesUnavailable { model } if model == "pk1.iv_bolus"),
        "{e:?}"
    );
    let message = e.to_string();
    assert!(
        message.contains("unavailable") && message.contains("forward differences"),
        "{message}"
    );
    input.options.derivatives = Derivatives::ForwardDifference;
    let regimen = run(&input).unwrap();
    let mut single = spec(Weighting::Uniform, true);
    single.options.derivatives = Derivatives::ForwardDifference;
    let single = run(&single).unwrap();
    // The same minimum, to rounding (the regimen sums kernels of volume 1, divided by V after).
    for name in ["estimate.v", "estimate.k", "wrss"] {
        let (a, b) = (regimen.get(name).unwrap(), single.get(name).unwrap());
        assert!((a - b).abs() <= 1e-10 * b.abs(), "{name}: {a} against {b}");
    }
}
