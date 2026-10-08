//! Unit tests of task T-030b: a forward-difference fit whose last Gauss-Newton direction cannot
//! decrease the WRSS ends `converged` by FIT-CNV-01 (a) with the derivative-error floor, at the
//! same iteration on every platform, instead of depending on a damped trial that lowers the WRSS
//! by one rounding unit (it did on Windows; on Linux none did and the fit ended `no_decrease`).

use std::collections::BTreeMap;

use crate::{Derivatives, FitInput, FitOptions, FitStatus, ModelId, Weighting, run};

/// Subject 1 of `oracle/data/fit/synthetic_infusion.csv` (T-029), fitted as in
/// `fit_infusion_inv_y` (1/y weights, `dur` = 2 h fixed, ε = 1e-10, 200 iterations), with the
/// default forward differences (h = 0.001).
fn infusion_subject_1(increment: f64) -> FitInput {
    let time = [
        0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0,
    ];
    let conc = [
        1.841, 3.418, 4.44, 5.785, 5.718, 4.244, 3.088, 1.817, 1.182, 0.3836, 0.1278, 0.01712,
    ];
    FitInput {
        model: ModelId::IvInfusion,
        dose: 100.0,
        time: time.to_vec(),
        conc: conc.to_vec(),
        weighting: Weighting::InvY,
        initial: BTreeMap::from([("v".to_string(), 16.2), ("k".to_string(), 0.229)]),
        options: FitOptions {
            derivatives: Derivatives::ForwardDifference,
            increment,
            convergence: 1e-10,
            max_iterations: 200,
            fixed: BTreeMap::from([("dur".to_string(), 2.0)]),
            ..FitOptions::default()
        },
    }
}

/// After four accepted steps the forward-difference direction predicts a gain of about 1.1e-9
/// of the WRSS (above ε = 1e-10, below h² = 1e-6) and every step along it raises the WRSS by far
/// more than rounding (1e-11 relative at ν = 1/1024): the derivatives cannot see further. The
/// fit stops there, `converged`, without trying damped steps whose outcome is decided by the last
/// bit of the arithmetic.
#[test]
fn a_forward_difference_fit_stops_at_the_resolution_of_its_derivatives() {
    let result = run(&infusion_subject_1(0.001)).unwrap();
    assert_eq!(result.status(), FitStatus::Converged);
    let trace = result.trace();
    // Iterations 0 to 4, then the stop at iteration 5 with no accepted step.
    assert_eq!(trace.len(), 5, "{trace:?}");
    assert!(trace.iter().all(|row| row.lambda == 0.0), "{trace:?}");
    let last = trace.last().unwrap();
    assert!(last.relative_decrease.unwrap() > 1e-10, "{last:?}");
    // The point is the forward-difference fixed point, within D-03 of the exact minimum (the
    // reference fit `fit_infusion_inv_y`: V = 13.1435897, k = 0.2784972).
    let v = result.get("estimate.v").unwrap();
    let k = result.get("estimate.k").unwrap();
    assert!((v / 13.1435897 - 1.0).abs() < 1e-4, "{v}");
    assert!((k / 0.2784972 - 1.0).abs() < 1e-4, "{k}");
}

/// The floor is h² only when it is above ε: with a fine increment (h² = 1e-12 < ε) the test of
/// (a) is the plain one and the fit still ends `converged` (the derivatives are good enough to
/// keep decreasing the WRSS until the relative decrease is below ε).
#[test]
fn with_a_fine_increment_the_plain_criterion_applies() {
    let result = run(&infusion_subject_1(1e-6)).unwrap();
    assert_eq!(result.status(), FitStatus::Converged);
}
