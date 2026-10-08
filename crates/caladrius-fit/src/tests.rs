//! Unit tests: worked examples F1 to F3 of `specs/fit.md` section 11 (printed to 7 or 8
//! significant digits, compared within `Tolerance::FIT_PARAMETERS`), the fixed point of
//! predicted-value weights checked by an independent computation of the stationarity equations,
//! statuses and serde.

use std::collections::BTreeMap;

use caladrius_testkit::Tolerance;

use crate::*;

const T: [f64; 5] = [0.5, 1.0, 2.0, 4.0, 8.0];
const Y: [f64; 5] = [9.31, 7.92, 6.85, 4.31, 2.11];

fn spec(weighting: Weighting, tight: bool) -> FitInput {
    let mut options = FitOptions {
        derivatives: Derivatives::Analytic,
        ..FitOptions::default()
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
    // Forward differences (the default) reach the same minimum.
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
    assert_eq!(o.increment, 0.001);
}
