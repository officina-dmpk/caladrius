#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! The synthetic lambda_z cases (task T-005) must discriminate between the competing readings of
//! the selection rule (`specs/nca.md`, open items O-01 and O-02), and PKNCA's answer recorded in
//! `oracle/expected/synthetic_lz*.csv` must be the one of a single reading.
//!
//! The readings are re-implemented here from the profile alone with the testkit's naive
//! regression, independently of PKNCA and of the engine:
//!
//! - `tolerance`: among the fits with lambda_z > 0 and adjusted R squared > M - f, where M is the
//!   best adjusted R squared of ALL fits (lambda_z <= 0 included), take the most points;
//! - `bonus`: the fit with the largest adjusted R squared + f * n (then the most points), kept
//!   only if its lambda_z > 0;
//! - `positive_first`: as `tolerance`, but M is the best adjusted R squared among the fits with
//!   lambda_z > 0 only;
//! - `max_only`: the best adjusted R squared among the fits with lambda_z > 0, no tolerance.
//!
//! This tests the oracle, not the engine.

use caladrius_testkit::naive::linear_regression;
use caladrius_testkit::{OracleCase, Profile, load_case};

struct Fit {
    n: usize,
    lambda_z: f64,
    adj: f64,
}

/// Fits on the last n eligible points, n = `min_points`..=m (eligible: after Tmax, positive).
fn fits(profile: &Profile, min_points: usize) -> Vec<Fit> {
    let mut tmax_index = 0;
    for (i, &c) in profile.conc.iter().enumerate() {
        if c > profile.conc[tmax_index] {
            tmax_index = i;
        }
    }
    let tmax = profile.time[tmax_index];
    let eligible: Vec<(f64, f64)> = profile
        .time
        .iter()
        .zip(&profile.conc)
        .filter(|&(&t, &c)| t > tmax && c > 0.0)
        .map(|(&t, &c)| (t, c.ln()))
        .collect();
    let m = eligible.len();
    let mut out = Vec::new();
    for n in min_points..=m {
        let (x, y): (Vec<f64>, Vec<f64>) = eligible[m - n..].iter().copied().unzip();
        let fit = linear_regression(&x, &y).expect("regression");
        let nf = n as f64;
        out.push(Fit {
            n,
            lambda_z: -fit.slope,
            adj: 1.0 - (1.0 - fit.r_squared) * (nf - 1.0) / (nf - 2.0),
        });
    }
    out
}

/// The number of points chosen by each reading (`None`: no terminal phase).
struct Readings {
    tolerance: Option<usize>,
    bonus: Option<usize>,
    positive_first: Option<usize>,
    max_only: Option<usize>,
}

fn longest_within(fits: &[&Fit], best: f64, f: f64) -> Option<usize> {
    fits.iter()
        .filter(|x| x.lambda_z > 0.0 && x.adj > best - f)
        .map(|x| x.n)
        .max()
}

fn readings(profile: &Profile, min_points: usize, f: f64) -> Readings {
    let all = fits(profile, min_points);
    let everything: Vec<&Fit> = all.iter().collect();
    let best_all = all.iter().map(|x| x.adj).fold(f64::NEG_INFINITY, f64::max);
    let tolerance = longest_within(&everything, best_all, f);

    let best_score = all
        .iter()
        .max_by(|a, b| {
            let (sa, sb) = (a.adj + f * a.n as f64, b.adj + f * b.n as f64);
            sa.total_cmp(&sb)
        })
        .expect("at least one fit");
    let bonus = (best_score.lambda_z > 0.0).then_some(best_score.n);

    let positive: Vec<&Fit> = all.iter().filter(|x| x.lambda_z > 0.0).collect();
    let best_positive = positive
        .iter()
        .map(|x| x.adj)
        .fold(f64::NEG_INFINITY, f64::max);
    let positive_first = longest_within(&positive, best_positive, f);
    let max_only = positive
        .iter()
        .max_by(|a, b| a.adj.total_cmp(&b.adj))
        .map(|x| x.n);
    Readings {
        tolerance,
        bonus,
        positive_first,
        max_only,
    }
}

fn pknca_points(case: &OracleCase, subject: u32) -> Option<usize> {
    case.expected
        .get(&subject.to_string(), "lambda.z.n.points")
        .expect("the parameter is in the case")
        .map(|v| v as usize)
}

fn profile(case: &OracleCase, subject: u32) -> &Profile {
    case.dataset
        .profiles
        .iter()
        .find(|p| p.subject == subject)
        .expect("the subject is in the dataset")
}

fn factor(case: &OracleCase) -> f64 {
    case.options.pknca_options.adj_r_squared_factor
}

/// O-01, profile D1 of W6 (subject 1): tolerance picks 3 points, bonus picks 6. PKNCA returns 3.
#[test]
fn o01_d1_pknca_follows_the_tolerance_reading() {
    let case = load_case("synthetic_lz").unwrap();
    let r = readings(profile(&case, 1), 3, factor(&case));
    assert_eq!(
        (r.tolerance, r.bonus, r.max_only),
        (Some(3), Some(6), Some(3))
    );
    assert_eq!(pknca_points(&case, 1), Some(3));
}

/// O-01, subject 3: tolerance 4 points, bonus 5, best adjusted R squared alone 3, longest 6.
/// The four readings are pairwise different and PKNCA returns the tolerance one.
#[test]
fn o01_four_readings_pairwise_different_pknca_is_tolerance() {
    let case = load_case("synthetic_lz").unwrap();
    let r = readings(profile(&case, 3), 3, factor(&case));
    assert_eq!(
        (r.tolerance, r.bonus, r.max_only),
        (Some(4), Some(5), Some(3))
    );
    assert_eq!(pknca_points(&case, 3), Some(4));
}

/// O-02, profile D2 of W6 (subject 2): the best adjusted R squared belongs to a rising fit; PKNCA
/// discards it afterwards and reports no terminal phase, while positive-first would select 6 points.
#[test]
fn o02_d2_pknca_applies_the_positive_filter_after_the_selection() {
    let case = load_case("synthetic_lz").unwrap();
    let r = readings(profile(&case, 2), 3, factor(&case));
    assert_eq!((r.tolerance, r.positive_first), (None, Some(6)));
    assert_eq!(pknca_points(&case, 2), None);
}

/// O-02, subject 4: both orders give a terminal phase but not the same one (4 points against 5);
/// PKNCA returns the filter-after one.
#[test]
fn o02_non_empty_selection_differs_between_the_two_orders() {
    let case = load_case("synthetic_lz").unwrap();
    let r = readings(profile(&case, 4), 3, factor(&case));
    assert_eq!((r.tolerance, r.positive_first), (Some(4), Some(5)));
    assert_eq!(pknca_points(&case, 4), Some(4));
}

/// The second case widens the tolerance to 1e-3: D1 then takes all 6 points, which is both the
/// tolerance and the bonus answer, and the case checks that the factor reaches the selection.
#[test]
fn wider_tolerance_changes_the_selection_as_the_reading_predicts() {
    let case = load_case("synthetic_lz_f1e3").unwrap();
    assert_eq!(factor(&case), 1e-3);
    for subject in 1..=4 {
        let r = readings(profile(&case, subject), 3, factor(&case));
        assert_eq!(
            pknca_points(&case, subject),
            r.tolerance,
            "subject {subject}"
        );
    }
}

/// Every subject of both cases is reproduced by the tolerance reading with the filter after the
/// selection, i.e. by one reading only.
#[test]
fn every_synthetic_subject_is_the_tolerance_reading() {
    for name in ["synthetic_lz", "synthetic_lz_f1e3"] {
        let case = load_case(name).unwrap();
        for p in &case.dataset.profiles {
            let r = readings(p, 3, factor(&case));
            assert_eq!(
                pknca_points(&case, p.subject),
                r.tolerance,
                "{name} {}",
                p.subject
            );
        }
    }
}
