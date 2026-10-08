#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! Cross-checks the edge-case oracle (task T-012) against independent, naive arithmetic
//! (AGENTS.md section 5, source 3): the BLQ and missing-value cleaning is re-implemented from the
//! rules of `specs/nca.md` and the areas of the cleaned profile are integrated with the testkit's
//! naive trapezoids; the relations between parameters (MRT, Vss, dose-normalised values, percent
//! extrapolated, percent back-extrapolated, C0, Tlag) are applied to the expected values. This
//! tests the oracle and the testkit, not the engine.
//!
//! What is not re-derived: the terminal-phase selection (the engine's job, tested in
//! `caladrius-nca`), and the values PKNCA reports for Tlast, Clast and Tfirst when a zero or a
//! missing value was replaced by a number (PKNCA computes them from the data before replacement,
//! while its areas use the data after; the oracle records that behaviour, it is not re-derived).

use caladrius_testkit::naive::{self, Rule};
use caladrius_testkit::oracle::{BlqRule, NaRule};
use caladrius_testkit::{OracleCase, Profile, Tolerance, load_case};

fn get(case: &OracleCase, profile: &Profile, name: &str) -> Option<f64> {
    case.expected
        .get(&profile.subject.to_string(), name)
        .unwrap_or_else(|| {
            panic!(
                "{}: no entry for subject {}, parameter {name}",
                case.name, profile.subject
            )
        })
}

fn close(case: &str, subject: u32, name: &str, actual: Option<f64>, expected: Option<f64>) {
    match (actual, expected) {
        (None, None) => {}
        (Some(a), Some(e)) => assert!(
            Tolerance::NCA_VS_PKNCA.accepts(a, e),
            "{case} subject {subject} {name}: independent value {a}, oracle {e}"
        ),
        (a, e) => panic!(
            "{case} subject {subject} {name}: independent value {a:?}, oracle {e:?} (one is not available)"
        ),
    }
}

fn rule_of(case: &OracleCase) -> Rule {
    if case.options.pknca_options.auc_method == "linear" {
        Rule::Linear
    } else {
        Rule::LinUpLogDown
    }
}

/// Cleaning as the rules of `specs/nca.md` (NCA-DAT-03, DAT-06) describe it: missing values first,
/// then each class of zeros, the classes being positions relative to the first and last positive
/// concentration, or relative to the first maximum, of the data after the missing values.
fn clean(case: &OracleCase, profile: &Profile) -> Vec<(f64, f64)> {
    let pk = &case.options.pknca_options;
    let mut points: Vec<(f64, f64)> = Vec::new();
    for (&t, &c) in profile.time.iter().zip(&profile.conc) {
        if c.is_nan() {
            if let NaRule::Replace(x) = pk.conc_na {
                points.push((t, x));
            }
        } else {
            points.push((t, c));
        }
    }
    // PKNCA measures the classes on the data after the missing values were dropped or replaced.
    let (tfirst, tlast) = bounds(&points);
    let tmax = {
        let mut best = (f64::NAN, f64::NEG_INFINITY);
        for &(t, c) in &points {
            if c > best.1 {
                best = (t, c);
            }
        }
        best.0
    };
    // (rule, class index): 0 first, 1 middle, 2 last, 3 before Tmax, 4 from Tmax on.
    let classes = [
        (pk.conc_blq.first, 0),
        (pk.conc_blq.middle, 1),
        (pk.conc_blq.last, 2),
        (pk.conc_blq.before_tmax, 3),
        (pk.conc_blq.after_tmax, 4),
    ];
    let in_class = |class: usize, t: f64| match class {
        0 => t <= tfirst,
        1 => tfirst < t && t < tlast,
        2 => t >= tlast,
        3 => t < tmax,
        _ => t >= tmax,
    };
    for (rule, class) in classes {
        let Some(rule) = rule else { continue };
        points = points
            .into_iter()
            .filter_map(|(t, c)| {
                if c == 0.0 && in_class(class, t) {
                    match rule {
                        BlqRule::Keep => Some((t, c)),
                        BlqRule::Drop => None,
                        BlqRule::Set(x) => Some((t, x)),
                    }
                } else {
                    Some((t, c))
                }
            })
            .collect();
    }
    points
}

/// Time of the first and of the last positive concentration.
fn bounds(points: &[(f64, f64)]) -> (f64, f64) {
    let first = points.iter().find(|p| p.1 > 0.0).map_or(f64::NAN, |p| p.0);
    let last = points
        .iter()
        .rev()
        .find(|p| p.1 > 0.0)
        .map_or(f64::NAN, |p| p.0);
    (first, last)
}

/// AUC and AUMC of the cleaned profile to its last positive point (`last`) or to its last point
/// (`all`); not available without a sample at the dose time (no start imputation).
fn areas(case: &OracleCase, cleaned: &[(f64, f64)]) -> [Option<f64>; 4] {
    if cleaned.first().is_none_or(|p| p.0 != 0.0) || cleaned.len() < 2 {
        return [None; 4];
    }
    let rule = rule_of(case);
    let (t, c): (Vec<f64>, Vec<f64>) = cleaned.iter().copied().unzip();
    let end = cleaned
        .iter()
        .rposition(|p| p.1 > 0.0)
        .map_or(cleaned.len(), |i| i + 1);
    [
        naive::auc(rule, &t[..end], &c[..end]),
        naive::auc(rule, &t, &c),
        naive::aumc(rule, &t[..end], &c[..end]),
        naive::aumc(rule, &t, &c),
    ]
}

/// The cleaning rules and the areas of the cleaned profile, on every BLQ and missing-value case.
#[test]
fn cleaned_profile_areas_are_reproduced_by_naive_arithmetic() {
    for name in [
        "edge_blq_default",
        "edge_blq_keep",
        "edge_blq_last_drop",
        "edge_blq_first_drop",
        "edge_blq_set",
        "edge_blq_tmax",
        "edge_missing_drop",
        "edge_missing_replace",
    ] {
        let case = load_case(name).unwrap();
        for p in &case.dataset.profiles {
            let [auclast, aucall, aumclast, aumcall] = areas(&case, &clean(&case, p));
            close(
                name,
                p.subject,
                "auclast",
                auclast,
                get(&case, p, "auclast"),
            );
            close(name, p.subject, "aucall", aucall, get(&case, p, "aucall"));
            close(
                name,
                p.subject,
                "aumclast",
                aumclast,
                get(&case, p, "aumclast"),
            );
            close(
                name,
                p.subject,
                "aumcall",
                aumcall,
                get(&case, p, "aumcall"),
            );
        }
    }
}

/// Cmax and Tmax of the cleaned profile (the first maximum).
#[test]
fn cmax_and_tmax_come_from_the_cleaned_profile() {
    for name in ["edge_blq_set", "edge_blq_tmax", "edge_missing_replace"] {
        let case = load_case(name).unwrap();
        for p in &case.dataset.profiles {
            let cleaned = clean(&case, p);
            let mut best = (f64::NAN, f64::NEG_INFINITY);
            for &(t, c) in &cleaned {
                if c > best.1 {
                    best = (t, c);
                }
            }
            close(name, p.subject, "cmax", Some(best.1), get(&case, p, "cmax"));
            close(name, p.subject, "tmax", Some(best.0), get(&case, p, "tmax"));
        }
    }
}

/// The cases do separate the policies (a guard against a case that tests nothing): the same
/// profiles give different areas under different policies.
#[test]
fn the_blq_policies_give_different_oracles() {
    let value = |case: &str, subject: u32, name: &str| {
        let c = load_case(case).unwrap();
        c.expected
            .get(&subject.to_string(), name)
            .flatten()
            .unwrap_or(f64::NAN)
    };
    // Subject 1 has an interior zero at 2 h: dropping it and keeping it integrate different areas.
    assert!(
        (value("edge_blq_default", 1, "auclast") - value("edge_blq_keep", 1, "auclast")).abs()
            > 1e-3
    );
    // Subject 3 has trailing zeros: AUCall sees them (a lin-up/log-down segment to 0 is linear).
    assert!(
        (value("edge_blq_keep", 3, "aucall") - value("edge_blq_last_drop", 3, "aucall")).abs()
            > 1e-3
    );
    // Dropping the zero at the dose time leaves no AUC.
    assert!(value("edge_blq_first_drop", 1, "auclast").is_nan());
    // A zero before Tmax and after the first positive (subject 6) is dropped by the position
    // default and kept by the Tmax-relative rule.
    assert!(
        (value("edge_blq_default", 6, "auclast") - value("edge_blq_tmax", 6, "auclast")).abs()
            > 1e-3
    );
}

/// Relations between parameters, applied to the expected values of every edge case.
#[test]
fn parameter_relations_hold_in_every_edge_case() {
    for name in caladrius_testkit::list_cases().unwrap() {
        if !name.starts_with("edge_") {
            continue;
        }
        let case = load_case(&name).unwrap();
        let infusion = case.options.infusion_duration.unwrap_or(0.0);
        let has = |p: &str| case.options.parameters.iter().any(|q| q == p);
        for p in &case.dataset.profiles {
            let g = |n: &str| get(&case, p, n);
            let s = p.subject;
            let dose = p.dose;
            let ratio = |a: Option<f64>, b: Option<f64>| match (a, b) {
                (Some(a), Some(b)) if b != 0.0 => Some(a / b),
                _ => None,
            };
            // Percent AUMC extrapolated (NCA-EXT-03b).
            for v in ["obs", "pred"] {
                if has(&format!("aumcpext.{v}")) {
                    let expected =
                        ratio(g("aumclast"), g(&format!("aumcinf.{v}"))).map(|r| 100.0 * (1.0 - r));
                    close(
                        &name,
                        s,
                        &format!("aumcpext.{v}"),
                        expected,
                        g(&format!("aumcpext.{v}")),
                    );
                }
                // Percent AUC extrapolated (NCA-EXT-02), same relation.
                let expected =
                    ratio(g("auclast"), g(&format!("aucinf.{v}"))).map(|r| 100.0 * (1.0 - r));
                close(
                    &name,
                    s,
                    &format!("aucpext.{v}"),
                    expected,
                    g(&format!("aucpext.{v}")),
                );
                // Clearance and volume (NCA-EXT-05, EXT-06).
                if has(&format!("cl.{v}")) {
                    let cl = g(&format!("aucinf.{v}")).map(|a| dose / a);
                    close(&name, s, &format!("cl.{v}"), cl, g(&format!("cl.{v}")));
                    let vz = ratio(cl, g("lambda.z"));
                    close(&name, s, &format!("vz.{v}"), vz, g(&format!("vz.{v}")));
                }
                // Plain Vss (NCA-EXT-07): clearance times the uncorrected MRT.
                if has(&format!("vss.{v}")) {
                    let mrt = ratio(g(&format!("aumcinf.{v}")), g(&format!("aucinf.{v}")));
                    let cl = g(&format!("aucinf.{v}")).map(|a| dose / a);
                    let vss = cl.zip(mrt).map(|(c, m)| c * m);
                    close(&name, s, &format!("vss.{v}"), vss, g(&format!("vss.{v}")));
                }
                // IV MRT and Vss: the infusion correction T/2 (NCA-EXT-04, EXT-07).
                if has(&format!("mrt.iv.{v}")) {
                    let mrt = ratio(g(&format!("aumcinf.{v}")), g(&format!("aucinf.{v}")))
                        .map(|m| m - infusion / 2.0);
                    close(
                        &name,
                        s,
                        &format!("mrt.iv.{v}"),
                        mrt,
                        g(&format!("mrt.iv.{v}")),
                    );
                    let cl = g(&format!("aucinf.{v}")).map(|a| dose / a);
                    let vss = cl.zip(mrt).map(|(c, m)| c * m);
                    close(
                        &name,
                        s,
                        &format!("vss.iv.{v}"),
                        vss,
                        g(&format!("vss.iv.{v}")),
                    );
                }
                if has(&format!("mrt.{v}")) {
                    let mrt = ratio(g(&format!("aumcinf.{v}")), g(&format!("aucinf.{v}")));
                    close(&name, s, &format!("mrt.{v}"), mrt, g(&format!("mrt.{v}")));
                }
            }
            // MRT to Tlast and IV Vss to Tlast.
            if has("mrt.last") {
                close(
                    &name,
                    s,
                    "mrt.last",
                    ratio(g("aumclast"), g("auclast")),
                    g("mrt.last"),
                );
            }
            if has("mrt.iv.last") {
                let mrt = ratio(g("aumclast"), g("auclast")).map(|m| m - infusion / 2.0);
                close(&name, s, "mrt.iv.last", mrt, g("mrt.iv.last"));
                let vss = g("auclast").zip(mrt).map(|(a, m)| dose / a * m);
                close(&name, s, "vss.iv.last", vss, g("vss.iv.last"));
            }
            // Dose-normalised values (NCA-OBS-05).
            for (dn, base) in [
                ("cmax.dn", "cmax"),
                ("clast.obs.dn", "clast.obs"),
                ("auclast.dn", "auclast"),
                ("aucall.dn", "aucall"),
                ("aucinf.obs.dn", "aucinf.obs"),
                ("aucinf.pred.dn", "aucinf.pred"),
                ("aumclast.dn", "aumclast"),
                ("aumcall.dn", "aumcall"),
                ("aumcinf.obs.dn", "aumcinf.obs"),
                ("aumcinf.pred.dn", "aumcinf.pred"),
            ] {
                if has(dn) {
                    close(&name, s, dn, g(base).map(|x| x / dose), g(dn));
                }
            }
            // Percent back-extrapolated, PKNCA form (NCA-IV-03): 100 * (1 - AUC / AUC_iv).
            for (pbext, auc, auciv) in [
                ("aucivpbextlast", "auclast", "aucivlast"),
                ("aucivpbextall", "aucall", "aucivall"),
                ("aucivpbextinf.obs", "aucinf.obs", "aucivinf.obs"),
                ("aucivpbextinf.pred", "aucinf.pred", "aucivinf.pred"),
            ] {
                if has(pbext) {
                    let expected = ratio(g(auc), g(auciv)).map(|r| 100.0 * (1.0 - r));
                    close(&name, s, pbext, expected, g(pbext));
                }
            }
            // No terminal-phase point during an infusion (NCA-LZ-02 rule 3).
            if infusion > 0.0 {
                if let Some(first) = g("lambda.z.time.first") {
                    assert!(
                        first > infusion,
                        "{name} subject {s}: window starts at {first}"
                    );
                }
            }
        }
    }
}

/// C0 of an IV bolus (NCA-IV-01) from the profile alone: the observed concentration at the dose
/// time when positive; otherwise the log-linear back-extrapolation through the first two post-dose
/// points when they decrease; otherwise the first post-dose concentration.
#[test]
fn c0_follows_the_three_methods_in_order() {
    for name in ["edge_iv", "edge_iv_linear"] {
        let case = load_case(name).unwrap();
        for p in &case.dataset.profiles {
            let expected = if p.conc[0] > 0.0 {
                p.conc[0]
            } else {
                let (t1, c1, t2, c2) = (p.time[1], p.conc[1], p.time[2], p.conc[2]);
                if c2 > 0.0 && c2 < c1 {
                    let k = (c1 / c2).ln() / (t2 - t1);
                    c1 * (k * (t1 - p.time[0])).exp()
                } else {
                    c1
                }
            };
            close(name, p.subject, "c0", Some(expected), get(&case, p, "c0"));
        }
    }
    // The three subjects exercise the three methods.
    let case = load_case("edge_iv").unwrap();
    let c0: Vec<f64> = case
        .dataset
        .profiles
        .iter()
        .map(|p| get(&case, p, "c0").unwrap())
        .collect();
    assert_eq!(c0[0], 10.0, "method 1: observed value at the dose time");
    assert!(
        c0[1] > case.dataset.profiles[1].conc[1],
        "method 2: extrapolated above C1"
    );
    assert_eq!(c0[2], 6.0, "method 3: first post-dose value");
}

/// Tlag (NCA-OBS-04): the time of the sample before the first sample that exceeds its predecessor;
/// not available when the profile never rises.
#[test]
fn tlag_is_the_sample_before_the_first_rise() {
    for name in ["edge_oral", "edge_blq_default"] {
        let case = load_case(name).unwrap();
        for p in &case.dataset.profiles {
            // PKNCA computes it on the cleaned profile; the default policy keeps leading zeros and
            // drops interior ones.
            let cleaned = clean(&case, p);
            let rise = (1..cleaned.len()).find(|&i| cleaned[i].1 > cleaned[i - 1].1);
            let expected = rise.map(|i| cleaned[i - 1].0);
            close(name, p.subject, "tlag", expected, get(&case, p, "tlag"));
        }
    }
}
