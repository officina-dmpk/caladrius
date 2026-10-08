#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! Cross-checks the public oracle against independent, naive arithmetic (AGENTS.md section 5,
//! source 3): the expected values PKNCA produced must be reproducible from the input profile plus
//! the terminal-phase window PKNCA reports. This tests the oracle and the testkit, not the engine.
//!
//! What is NOT re-derived here: the choice of the terminal-phase window (best adjusted R squared).
//! That rule is the engine's job and is tested against `expected/*.csv` in `caladrius-nca`.

use caladrius_testkit::naive::{self, Rule};
use caladrius_testkit::{OracleCase, Table, Tolerance, compare_tables, list_cases, load_case};

fn expected_f64(case: &OracleCase, subject: &str, name: &str) -> f64 {
    case.expected
        .get(subject, name)
        .flatten()
        .unwrap_or_else(|| {
            panic!(
                "{}: no value for subject {subject}, parameter {name}",
                case.name
            )
        })
}

/// Everything derivable from the profile and the reported terminal window, for every subject.
fn derive(case: &OracleCase) -> Table {
    let rule = if case.options.pknca_options.auc_method == "linear" {
        Rule::Linear
    } else {
        Rule::LinUpLogDown
    };
    let iv = case.options.route == "iv_bolus";
    let mut out = Table::new();
    for profile in &case.dataset.profiles {
        let s = profile.subject.to_string();
        let s = s.as_str();
        let mut put = |name: &str, value: f64| out.insert(s, name, Some(value));

        // Observed-sample parameters.
        let (mut cmax, mut tmax) = (f64::NEG_INFINITY, f64::NAN);
        for (&t, &c) in profile.time.iter().zip(&profile.conc) {
            if c > cmax {
                cmax = c;
                tmax = t;
            }
        }
        let last = profile
            .conc
            .iter()
            .rposition(|&c| c > 0.0)
            .expect("a positive concentration");
        let tlast = profile.time[last];
        let clast = profile.conc[last];
        put("cmax", cmax);
        put("tmax", tmax);
        // PKNCA's Tfirst is the first time with a positive concentration, not the first sample time.
        let first_pos = profile
            .conc
            .iter()
            .position(|&c| c > 0.0)
            .expect("a positive concentration");
        put("tfirst", profile.time[first_pos]);
        put("tlast", tlast);
        put("clast.obs", clast);

        // IV bolus: C0 by log-linear back-extrapolation of the first two points, added at t = 0.
        let (mut time, mut conc) = (profile.time.clone(), profile.conc.clone());
        if iv && time[0] > 0.0 {
            let slope = (conc[1].ln() - conc[0].ln()) / (time[1] - time[0]);
            let c0 = (conc[0].ln() - slope * time[0]).exp();
            put("c0", c0);
            time.insert(0, 0.0);
            conc.insert(0, c0);
        }

        // AUC and AUMC up to the last positive concentration.
        let end = time
            .iter()
            .position(|&t| t == tlast)
            .expect("tlast is a sample time")
            + 1;
        let (tt, cc) = (&time[..end], &conc[..end]);
        let auclast = naive::auc(rule, tt, cc).expect("auc");
        let aumclast = naive::aumc(rule, tt, cc).expect("aumc");
        put("auclast", auclast);
        put("aumclast", aumclast);
        let end_all = time.len();
        put(
            "aucall",
            naive::auc(rule, &time[..end_all], &conc[..end_all]).expect("auc"),
        );

        // Terminal phase: regression on the window PKNCA reports. A subject for which PKNCA
        // reports no terminal phase (synthetic cases, task T-005) has nothing more to derive.
        if case
            .expected
            .get(s, "lambda.z.time.first")
            .flatten()
            .is_none()
        {
            continue;
        }
        let first = expected_f64(case, s, "lambda.z.time.first");
        let lz_last = expected_f64(case, s, "lambda.z.time.last");
        let (wx, wy): (Vec<f64>, Vec<f64>) = profile
            .time
            .iter()
            .zip(&profile.conc)
            .filter(|&(&t, &c)| t >= first && t <= lz_last && c > 0.0)
            .map(|(&t, &c)| (t, c.ln()))
            .unzip();
        let fit = naive::linear_regression(&wx, &wy).expect("regression");
        let n = wx.len() as f64;
        let lz = -fit.slope;
        put("lambda.z", lz);
        put("lambda.z.n.points", n);
        put("r.squared", fit.r_squared);
        put(
            "adj.r.squared",
            1.0 - (1.0 - fit.r_squared) * (n - 1.0) / (n - 2.0),
        );
        let half = std::f64::consts::LN_2 / lz;
        put("half.life", half);
        put("span.ratio", (lz_last - first) / half);
        let clast_pred = (fit.intercept + fit.slope * tlast).exp();
        put("clast.pred", clast_pred);

        // Extrapolation to infinity, observed and predicted last concentration.
        let dose = profile.dose;
        for (suffix, c_end) in [("obs", clast), ("pred", clast_pred)] {
            let aucinf = auclast + c_end / lz;
            let aumcinf = aumclast + tlast * c_end / lz + c_end / (lz * lz);
            let mrt = aumcinf / aucinf;
            let cl = dose / aucinf;
            put(&format!("aucinf.{suffix}"), aucinf);
            put(&format!("aumcinf.{suffix}"), aumcinf);
            put(
                &format!("aucpext.{suffix}"),
                100.0 * (aucinf - auclast) / aucinf,
            );
            put(&format!("cl.{suffix}"), cl);
            put(&format!("vz.{suffix}"), cl / lz);
            if iv {
                put(&format!("mrt.iv.{suffix}"), mrt);
                put(&format!("vss.iv.{suffix}"), cl * mrt);
            } else {
                put(&format!("mrt.{suffix}"), mrt);
            }
        }
    }
    out
}

#[test]
fn every_public_case_is_reproduced_by_independent_arithmetic() {
    let cases = list_cases().expect("list cases");
    assert!(!cases.is_empty());
    for name in cases {
        let case = load_case(&name).expect("load case");
        let derived = derive(&case);
        assert!(
            derived.len() > 20 * case.options.n_subjects,
            "{name}: only {} derived values",
            derived.len()
        );
        // `derived` is the reference side here, so only the entries it derives are checked; the
        // oracle may hold more.
        let report = compare_tables(&derived, &case.expected, Tolerance::NCA_VS_PKNCA);
        if let Err(text) = report.into_result() {
            panic!("{name}: the oracle disagrees with naive arithmetic\n{text}");
        }
    }
}

#[test]
fn the_cross_check_actually_detects_a_wrong_oracle() {
    let case = load_case("theoph").expect("load case");
    let derived = derive(&case);
    let mut tampered = Table::new();
    for (g, n, v) in case.expected.iter() {
        let v = if g == "3" && n == "auclast" {
            v.map(|x| x * 1.001)
        } else {
            v
        };
        tampered.insert(g, n, v);
    }
    let report = compare_tables(&derived, &tampered, Tolerance::NCA_VS_PKNCA);
    assert_eq!(report.mismatches.len(), 1, "{report}");
}

#[test]
fn the_two_trapezoid_rules_give_different_oracles() {
    let a = load_case("theoph").expect("load case");
    let b = load_case("theoph_linear").expect("load case");
    let (x, y) = (
        expected_f64(&a, "1", "auclast"),
        expected_f64(&b, "1", "auclast"),
    );
    assert!((x - y).abs() > 1e-3 * x, "rules should differ: {x} vs {y}");
    // The terminal phase does not depend on the AUC rule.
    assert_eq!(
        expected_f64(&a, "1", "lambda.z"),
        expected_f64(&b, "1", "lambda.z")
    );
}
