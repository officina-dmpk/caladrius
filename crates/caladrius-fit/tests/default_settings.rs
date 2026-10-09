#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! The default fit settings against the exact minimum (task T-040, Q-014 option 2,
//! `specs/differences.md` D-03 and D-04).
//!
//! Every reference fit of `oracle/expected/fit/` (185 fits, 490 estimates) is run with the
//! engine's DEFAULT options, only the case's initial estimates, fixed parameters and confidence
//! level being taken from the options file, and its estimates are compared with the reference at
//! the contract tolerance `FIT_PARAMETERS` (1e-4 relative). The oracle tests themselves run with
//! the reference run's settings (FIT-CNV-04); this file checks what a user gets by default.

use caladrius_fit::{Criterion, Derivatives, FitInput, FitOptions, FitStatus, Weighting, run};
use caladrius_models::ModelId;
use caladrius_testkit::{FitCase, Tolerance, list_fit_cases, load_fit_case};

fn input_of(case: &FitCase, subject: &str, options: &FitOptions) -> FitInput {
    let profile = case.profile(subject).unwrap();
    let (time, conc) = case.fitted_observations(profile);
    FitInput {
        model: ModelId::from_id(&case.model).unwrap(),
        dose: profile.dose,
        time,
        conc,
        weighting: Weighting::from_id(&case.weighting).unwrap(),
        initial: case.initial_estimates[subject].clone(),
        options: FitOptions {
            confidence_level: case.options.confidence_level,
            fixed: case.fixed.clone(),
            ..options.clone()
        },
    }
}

/// Agreement of one setting with the references.
#[derive(Default, Debug)]
struct Agreement {
    fits: usize,
    not_converged: Vec<String>,
    estimates: usize,
    beyond: usize,
    worst: (f64, String),
    worst_wrss: (f64, String),
    most_iterations: usize,
}

fn measure(options: &FitOptions) -> Agreement {
    let mut a = Agreement::default();
    for name in list_fit_cases().unwrap() {
        let case = load_fit_case(&name).unwrap();
        for subject in &case.subjects {
            a.fits += 1;
            let result = run(&input_of(&case, subject, options))
                .unwrap_or_else(|e| panic!("{name} subject {subject}: {e}"));
            if result.status() != FitStatus::Converged {
                a.not_converged
                    .push(format!("{name} {subject} {:?}", result.status()));
            }
            a.most_iterations = a
                .most_iterations
                .max(result.trace().len().saturating_sub(1));
            for (s, quantity, value) in case.expected.iter() {
                let (true, Some(expected), Some(actual)) =
                    (s == subject, value, result.get(quantity))
                else {
                    continue;
                };
                let secondary = ["cl", "half_life", "auc_inf"]
                    .iter()
                    .any(|x| quantity.ends_with(x));
                let gap = Tolerance::FIT_PARAMETERS.error(actual, expected);
                if quantity == "wrss" && gap > a.worst_wrss.0 {
                    a.worst_wrss = (gap, format!("{name} subject {subject}"));
                }
                if !quantity.starts_with("estimate.") || secondary {
                    continue;
                }
                a.estimates += 1;
                a.beyond += usize::from(!Tolerance::FIT_PARAMETERS.accepts(actual, expected));
                if gap > a.worst.0 {
                    a.worst = (gap, format!("{name} subject {subject} {quantity}"));
                }
            }
        }
    }
    a
}

fn print(title: &str, a: &Agreement) {
    println!(
        "{title}: {} fits, {} of {} estimates beyond 1e-4, worst {:.2e} ({}), worst WRSS {:.2e} ({}), \
         at most {} iterations, {} not converged {:?}",
        a.fits,
        a.beyond,
        a.estimates,
        a.worst.0,
        a.worst.1,
        a.worst_wrss.0,
        a.worst_wrss.1,
        a.most_iterations,
        a.not_converged.len(),
        a.not_converged
    );
}

/// The gate of T-040: with the default options every reference fit converges within the default
/// 50 iterations and every estimate lies within 1e-4 relative of the exact minimum. Measured on
/// 2026-10-09: 185 fits, 0 of 490 estimates beyond 1e-4, worst 7.4e-6 (Indometh `inv_y`, subject
/// 2, k), at most 22 iterations.
#[test]
fn the_default_settings_reach_the_exact_minimum_on_every_reference_fit() {
    let a = measure(&FitOptions::default());
    print("default", &a);
    assert_eq!(a.fits, 185);
    assert_eq!(a.estimates, 490);
    assert!(a.not_converged.is_empty(), "{:?}", a.not_converged);
    assert_eq!(a.beyond, 0, "{a:?}");
    assert!(a.most_iterations <= FitOptions::default().max_iterations);
}

/// Not a gate: the candidate stopping rules of T-040, printed for the card.
/// `cargo test -p caladrius-fit --test default_settings -- --ignored --nocapture`
#[test]
#[ignore = "prints the measured agreement of candidate settings; not a gate"]
fn report_candidate_settings() {
    let base = FitOptions {
        derivatives: Derivatives::Analytic,
        ..FitOptions::default()
    };
    let candidates: Vec<(String, FitOptions)> = vec![
        (
            "analytic, relative decrease 1e-10, 50 iterations".into(),
            FitOptions {
                criterion: Criterion::RelativeDecrease,
                convergence: 1e-10,
                max_iterations: 50,
                ..base.clone()
            },
        ),
        (
            "analytic, relative decrease 1e-8, 50 iterations".into(),
            FitOptions {
                criterion: Criterion::RelativeDecrease,
                convergence: 1e-8,
                max_iterations: 50,
                ..base.clone()
            },
        ),
        (
            "analytic, relative offset 1e-5, 50 iterations".into(),
            FitOptions {
                criterion: Criterion::RelativeOffset,
                convergence: 1e-5,
                max_iterations: 50,
                ..base.clone()
            },
        ),
        (
            "analytic, relative offset 1e-6, 50 iterations".into(),
            FitOptions {
                criterion: Criterion::RelativeOffset,
                convergence: 1e-6,
                max_iterations: 50,
                ..base.clone()
            },
        ),
        (
            "forward 1e-5, relative decrease 1e-10, 50 iterations".into(),
            FitOptions {
                derivatives: Derivatives::ForwardDifference,
                increment: 1e-5,
                criterion: Criterion::RelativeDecrease,
                convergence: 1e-10,
                max_iterations: 50,
                ..base.clone()
            },
        ),
    ];
    for (title, options) in &candidates {
        print(title, &measure(options));
    }
    print(
        "reference conventions",
        &measure(&FitOptions::reference_conventions()),
    );
}
