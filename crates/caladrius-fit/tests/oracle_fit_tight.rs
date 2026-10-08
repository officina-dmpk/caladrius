#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! A fit case on which `caladrius-fit` and R agree at 1e-6 (task T-029), and the measured
//! agreement on the whole fit oracle.
//!
//! Why the fit tolerance (`Tolerance::FIT_PARAMETERS`, 1e-4 relative) is looser than the NCA one
//! (1e-6) is explained in `specs/fit.md` section 10 (tolerance note). In short: two optimizers
//! stop at points that differ by their stopping rules and by the way the partial derivatives are
//! formed, not by arithmetic error; 1e-4 is the contract (`AGENTS.md` section 5), and this file
//! shows that on a case small enough to check by hand the engine and R agree at the NCA tolerance,
//! and reports how large the gaps actually are on the other 94 reference fits of the older models. Nothing here
//! loosens or replaces a tolerance; the one used below, `FIT_PARAMETERS_HAND_CHECKED`, is stricter.
//!
//! The hand-checked case is the five-point example of `specs/fit.md` section 11 (F1, F2): 5
//! observations, 2 parameters, uniform weights. Its Gauss-Newton iterations were written out by
//! hand in the specification (JtJ and Jtr are 2x2) and are reproduced to 15 digits by an
//! independent recurrence (`oracle/expected/fit/gauss_newton_f1.csv`), which converges to the
//! same minimum that `minpack.lm::nlsLM` and `stats::nls` reach.

mod common;

use caladrius_fit::Derivatives;
use caladrius_testkit::{Tolerance, list_fit_cases, load_fit_case, load_gauss_newton_f1};
use common::{check_group_with, engine, fit};

const SPEC_UNIFORM: &str = "fit_spec_uniform";

/// Every parameter and statistic of F2 (uniform weights) at 1e-6 relative: the estimates, their
/// standard errors, CV%, covariance, correlation, eigenvalues, S, sums of squares, the secondary
/// parameters. The weighted sum of squares keeps its own 1e-6 and the confidence limits the
/// derived allowance of the older tests, both with the stricter parameter tolerance.
#[test]
fn the_hand_checked_example_agrees_with_r_at_1e_6() {
    for group in [
        "estimates",
        "wrss",
        "statistics",
        "intervals",
        "information_criteria",
        "secondary",
    ] {
        check_group_with(
            SPEC_UNIFORM,
            group,
            Derivatives::Analytic,
            engine,
            Tolerance::FIT_PARAMETERS_HAND_CHECKED,
        );
    }
}

/// The hand-iterated Gauss-Newton of the specification reaches R's minimum: iteration 5 of the
/// recurrence is within 1e-6 of the `nlsLM` estimates, and so is the engine's fit. Both come from
/// the same 15-digit file, so a change of either side is caught.
#[test]
fn hand_iterated_gauss_newton_the_engine_and_r_reach_the_same_minimum() {
    let case = load_fit_case(SPEC_UNIFORM).unwrap();
    let r_v = case.expected.get("1", "estimate.v").flatten().unwrap();
    let r_k = case.expected.get("1", "estimate.k").flatten().unwrap();
    let last = load_gauss_newton_f1().unwrap().pop().unwrap();
    assert_eq!(last.iteration, 5);
    let tight = Tolerance::FIT_PARAMETERS_HAND_CHECKED;
    assert!(
        tight.accepts(last.v, r_v),
        "hand recurrence V {} against R {r_v}",
        last.v
    );
    assert!(
        tight.accepts(last.k, r_k),
        "hand recurrence k {} against R {r_k}",
        last.k
    );
    let engine_fit = fit(&case, "1", Derivatives::Analytic, engine);
    assert!(tight.accepts(engine_fit.get("estimate.v").unwrap(), r_v));
    assert!(tight.accepts(engine_fit.get("estimate.k").unwrap(), r_k));
    // And the three agree with the values printed in the specification (F2), to the 7 digits
    // printed there.
    assert!((r_v - 9.9186407).abs() < 5e-8 * 1.01 + 1e-9);
    assert!((r_k - 0.2040577).abs() < 5e-8 * 1.01 + 1e-9);
}

/// Gaps between the engine and the reference over the fits of the cases selected by `select`.
#[derive(Default)]
struct Gaps {
    fits: usize,
    estimates: usize,
    estimates_over_tolerance: usize,
    worst_estimate: (f64, String),
    worst_se: (f64, String),
    worst_wrss: (f64, String),
}

fn measure(select: impl Fn(&str) -> bool, derivatives: Derivatives) -> Gaps {
    let mut gaps = Gaps::default();
    for name in list_fit_cases().unwrap() {
        if !select(&name) {
            continue;
        }
        let case = load_fit_case(&name).unwrap();
        for subject in &case.subjects {
            gaps.fits += 1;
            let result = fit(&case, subject, derivatives, engine);
            for (s, quantity, value) in case.expected.iter() {
                let (true, Some(expected), Some(actual)) =
                    (s == subject, value, result.get(quantity))
                else {
                    continue;
                };
                let gap = Tolerance::FIT_PARAMETERS.error(actual, expected);
                let secondary = ["cl", "half_life", "auc_inf"]
                    .iter()
                    .any(|x| quantity.ends_with(x));
                let is_estimate = quantity.starts_with("estimate.") && !secondary;
                if is_estimate {
                    gaps.estimates += 1;
                    gaps.estimates_over_tolerance +=
                        usize::from(!Tolerance::FIT_PARAMETERS.accepts(actual, expected));
                }
                let target = if is_estimate {
                    &mut gaps.worst_estimate
                } else if quantity.starts_with("se.") && !secondary {
                    &mut gaps.worst_se
                } else if quantity == "wrss" {
                    &mut gaps.worst_wrss
                } else {
                    continue;
                };
                if gap > target.0 {
                    *target = (gap, format!("{name} subject {subject} {quantity}"));
                }
            }
        }
    }
    gaps
}

fn print(title: &str, gaps: &Gaps) {
    println!("{title}: {} fits", gaps.fits);
    println!(
        "  estimates beyond 1e-4: {} of {}",
        gaps.estimates_over_tolerance, gaps.estimates
    );
    println!(
        "  worst estimate gap: {:.2e} ({})",
        gaps.worst_estimate.0, gaps.worst_estimate.1
    );
    println!(
        "  worst standard-error gap: {:.2e} ({})",
        gaps.worst_se.0, gaps.worst_se.1
    );
    println!(
        "  worst WRSS gap: {:.2e} ({})",
        gaps.worst_wrss.0, gaps.worst_wrss.1
    );
}

/// Not a gate: prints the largest relative gap between the engine and R, per quantity.
///
/// - With analytic derivatives and the convergence settings of the reference run, over the
///   reference fits of the older models (Theoph, Indometh, the five-point example: 95 fits).
/// - With the engine's default forward differences and the same convergence settings, over the
///   fits of every case, grouped by dataset (`specs/differences.md` D-03).
///
/// The figures in `specs/fit.md` section 10, `specs/differences.md` D-03 and the README come from
/// this run: `cargo test -p caladrius-fit --test oracle_fit_tight -- --ignored --nocapture`.
#[test]
#[ignore = "prints the measured agreement with the reference; not a gate"]
fn report_the_measured_agreement_with_the_reference() {
    let older = |n: &str| {
        ["fit_theoph_", "fit_indometh_", "fit_spec_"]
            .iter()
            .any(|p| n.starts_with(p))
    };
    let analytic = measure(older, Derivatives::Analytic);
    print("analytic derivatives, older models", &analytic);
    assert_eq!(analytic.fits, 95);

    let mut total = (0, 0);
    for prefix in [
        "fit_theoph_",
        "fit_indometh_",
        "fit_spec_",
        "fit_infusion_",
        "fit_zero_order_",
        "fit_lag_",
    ] {
        let gaps = measure(|n| n.starts_with(prefix), Derivatives::ForwardDifference);
        print(&format!("forward differences, {prefix}*"), &gaps);
        total.0 += gaps.estimates_over_tolerance;
        total.1 += gaps.estimates;
    }
    println!(
        "forward differences, all: {} of {} estimates beyond 1e-4",
        total.0, total.1
    );
}
