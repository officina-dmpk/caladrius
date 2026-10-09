#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! Cross-checks the multiple-dosing oracle (task T-047) against independent arithmetic
//! (`AGENTS.md` section 5, source 3), so that the expected files are tested by something other than
//! the R script that wrote them:
//!
//! - the files load, are complete and have the expected shape (twelve ids, every kind of regimen,
//!   the convention cases, the error suite);
//! - a plain double-precision superposition of the textbook single-dose forms (`naive2c` for two
//!   compartments, the forms written below for one) reproduces the schedules, the regular regimens
//!   and the steady-state profiles, in the cases where those forms keep their digits;
//! - the derived quantities satisfy the identities of `specs/models.md` MOD-MD-08 to 10
//!   (`cav_ss = D / (CL tau)`, the area over one interval is `D / CL`, the profile stays between
//!   `cmin_ss` and `cmax_ss`, the bolus of one compartment accumulates by `1 / (1 - exp(-k tau))`);
//! - the worked examples P1 to P4 of `specs/models.md` section 12.2, computed by the reader in a
//!   throwaway script, are reproduced at the digits printed there, including the cases where the
//!   printed branch of S-36 fails.
//!
//! This tests the oracle and the testkit, not the engine.

use std::collections::BTreeMap;

use caladrius_testkit::naive2c::{conc_auc_aumc, exponents};
use caladrius_testkit::{
    MdCase, RegimenKind, dose_parameters, list_md_cases, load_md_case, load_md_errors, md_dir,
};

fn all_cases() -> Vec<MdCase> {
    list_md_cases()
        .unwrap()
        .iter()
        .map(|n| load_md_case(n).unwrap())
        .collect()
}

fn scalar(c: &MdCase, name: &str) -> Option<f64> {
    c.case.expected.get("scalar", name).flatten()
}

/// A grid value by the time written as `t=<number>`: the number is matched to the times of the case, so
/// that `t=0.2` finds the key written with 17 digits.
fn at(c: &MdCase, key: &str, quantity: &str) -> Option<f64> {
    let t: f64 = key.trim_start_matches("t=").parse().unwrap();
    let i = c
        .case
        .times
        .iter()
        .position(|&x| x == t)
        .unwrap_or_else(|| panic!("{}: no time {key}", c.case.name));
    c.case
        .expected
        .get(&c.case.time_keys[i], quantity)
        .flatten()
}

fn kind_of(c: &MdCase) -> &'static str {
    let id = c.case.model.split('.').nth(1).unwrap_or("");
    if id == "iv_bolus" {
        "bolus"
    } else if id.starts_with("oral_1") {
        "first"
    } else {
        "zero"
    }
}

// ---------------------------------------------------------------- the independent single dose

/// Concentration and AUC of one dose at time `t` after it, by the textbook forms in double
/// precision; `None` when the forms lose their digits (`ka` close to `k`) or a parameter is missing.
fn single_dose(case: &MdCase, dose: f64, dur: Option<f64>, t: f64) -> Option<(f64, f64)> {
    let model = case.case.model.as_str();
    let mut p = case.case.parameters.clone();
    if let Some(d) = dur {
        p.insert("dur".to_string(), d);
    }
    if model.starts_with("pk2.") {
        let [c, a, _] = conc_auc_aumc(model, &case.parameterisation, &p, dose, t)?;
        return Some((c, a));
    }
    let v = *p.get("v")?;
    let cl = *p.get("cl")?;
    let k = cl / v;
    let tl = p.get("tlag").copied().unwrap_or(0.0);
    match model {
        "pk1.iv_bolus" => {
            if t < 0.0 {
                return Some((0.0, 0.0));
            }
            Some((
                dose / v * (-k * t).exp(),
                dose / (v * k) * (1.0 - (-k * t).exp()),
            ))
        }
        "pk1.iv_infusion" | "pk1.oral_0" | "pk1.oral_0_lag" => {
            let big_t = *p.get("dur")?;
            let u = t - tl;
            if u <= 0.0 {
                return Some((0.0, 0.0));
            }
            let r = dose / big_t;
            if u <= big_t {
                Some((
                    r / (v * k) * (1.0 - (-k * u).exp()),
                    r / (v * k) * (u - (1.0 - (-k * u).exp()) / k),
                ))
            } else {
                let ct = r / (v * k) * (1.0 - (-k * big_t).exp());
                let at_t = r / (v * k) * (big_t - (1.0 - (-k * big_t).exp()) / k);
                Some((
                    ct * (-k * (u - big_t)).exp(),
                    at_t + ct / k * (1.0 - (-k * (u - big_t)).exp()),
                ))
            }
        }
        "pk1.oral_1" | "pk1.oral_1_lag" => {
            let ka = *p.get("ka")?;
            if (ka - k).abs() < 5e-2 * ka {
                return None;
            }
            let u = t - tl;
            if u <= 0.0 {
                return Some((0.0, 0.0));
            }
            let f = dose * ka / (v * (ka - k));
            Some((
                f * ((-k * u).exp() - (-ka * u).exp()),
                f * ((1.0 - (-k * u).exp()) / k - (1.0 - (-ka * u).exp()) / ka),
            ))
        }
        _ => None,
    }
}

/// The doses of a schedule or a regular regimen as (time, dose, dur).
fn doses_of(case: &MdCase) -> Vec<(f64, f64, Option<f64>)> {
    match case.regimen.kind {
        RegimenKind::Schedule => case
            .regimen
            .records
            .iter()
            .map(|r| (r.time, r.dose, r.dur))
            .collect(),
        RegimenKind::Regular => {
            let tau = case.regimen.tau.unwrap();
            let dur = case.case.parameters.get("dur").copied();
            (0..case.regimen.n_doses.unwrap())
                .map(|j| (f64::from(j) * tau, case.case.dose, dur))
                .collect()
        }
        RegimenKind::SteadyState => Vec::new(),
    }
}

/// Number of doses after which the slowest exponent has removed 1e-20 of a dose.
fn terms_for(case: &MdCase, tau: f64) -> usize {
    let p = &case.case.parameters;
    let slowest = if case.case.model.starts_with("pk1.") {
        let k = p["cl"] / p["v"];
        p.get("ka").map_or(k, |&ka| k.min(ka))
    } else {
        let (_, beta) = exponents(&case.parameterisation, p, case.case.dose).unwrap();
        p.get("ka").map_or(beta, |&ka| beta.min(ka))
    };
    let lag = p.get("tlag").copied().unwrap_or(0.0) + p.get("dur").copied().unwrap_or(0.0);
    ((46.0 / (slowest * tau)).ceil() + (lag / tau).ceil() + 3.0) as usize
}

fn close(what: &str, actual: f64, expected: f64, rel: f64, floor: f64) {
    let allowed = rel * expected.abs() + floor;
    assert!(
        (actual - expected).abs() <= allowed,
        "{what}: independent value {actual:e}, oracle {expected:e}"
    );
}

fn digits(what: &str, actual: Option<f64>, printed: f64) {
    let actual = actual.unwrap_or(f64::NAN);
    assert!(
        (actual - printed).abs() <= 5.1e-11 * printed.abs().max(1.0),
        "{what}: oracle {actual}, specification {printed}"
    );
}

// ---------------------------------------------------------------- shape

#[test]
fn the_oracle_has_its_expected_shape() {
    let cases = all_cases();
    let ids = [
        "iv_bolus",
        "iv_infusion",
        "oral_1",
        "oral_1_lag",
        "oral_0",
        "oral_0_lag",
    ];
    for family in ["pk1", "pk2"] {
        for id in ids {
            let model = format!("{family}.{id}");
            let mine: Vec<_> = cases.iter().filter(|c| c.case.model == model).collect();
            for n in [1, 3, 10, 40] {
                assert!(
                    mine.iter()
                        .any(|c| c.regimen.kind == RegimenKind::Schedule
                            && c.regimen.records.len() == n),
                    "{model}: no schedule of {n} doses"
                );
                assert!(
                    mine.iter().any(|c| c.regimen.kind == RegimenKind::Regular
                        && c.regimen.n_doses == Some(n as u32)),
                    "{model}: no regular regimen of {n} doses"
                );
            }
            let steady = mine
                .iter()
                .filter(|c| c.regimen.kind == RegimenKind::SteadyState)
                .count();
            assert!(steady >= 5, "{model}: {steady} steady-state cases");
            // irregular schedules: unsorted records and a zero dose
            let n10 = mine
                .iter()
                .find(|c| c.regimen.kind == RegimenKind::Schedule && c.regimen.records.len() == 10)
                .unwrap();
            let times: Vec<f64> = n10.regimen.records.iter().map(|r| r.time).collect();
            assert!(
                times.windows(2).any(|w| w[0] > w[1]),
                "{model}: sorted records"
            );
            assert!(n10.regimen.records.iter().any(|r| r.dose == 0.0));
        }
    }
    for c in &cases {
        for key in &c.case.time_keys {
            assert!(
                at(c, key, "conc").is_some() && at(c, key, "auc").is_some(),
                "{}: a time without values",
                c.case.name
            );
        }
        match c.regimen.kind {
            RegimenKind::SteadyState => {
                let tau = c.regimen.tau.unwrap();
                assert!(c.case.times.iter().all(|&s| (0.0..=tau).contains(&s)));
                assert!(c.case.times.contains(&0.0) && c.case.times.contains(&tau));
                for name in [
                    "cmax_ss",
                    "tmax_ss",
                    "cmin_ss",
                    "cav_ss",
                    "auc_tau_ss",
                    "accum_cmax",
                    "accum_auc",
                ] {
                    assert!(
                        c.case.expected.get("scalar", name).is_some(),
                        "{}: no row {name}",
                        c.case.name
                    );
                }
            }
            _ => assert!(scalar(c, "auc_inf").is_some(), "{}", c.case.name),
        }
    }
    // the groups and the convention cases
    for group in ["superposition", "steady_state", "convention"] {
        assert!(cases.iter().any(|c| c.group == group), "group {group}");
    }
    let lag_conv = cases
        .iter()
        .filter(|c| c.group == "convention" && c.case.model.ends_with("_lag"))
        .count();
    assert!(lag_conv >= 30, "{lag_conv} lag convention cases");
    // the error suite
    let errors = load_md_errors().unwrap();
    assert!(errors.len() >= 40, "{} error cases", errors.len());
    for group in [
        "tau",
        "tau_below_dur",
        "times",
        "n_doses",
        "schedule",
        "lag",
        "domain",
    ] {
        assert!(
            errors.iter().any(|e| e.group == group),
            "error group {group}"
        );
    }
    assert!(md_dir().join("model_md_errors.csv").is_file());
}

#[test]
fn the_regimen_parameters_are_encoded_by_name() {
    let c = load_md_case("model_md_pk1_iv_infusion_sched_n3").unwrap();
    let named: BTreeMap<String, f64> = dose_parameters(&c.regimen).into_iter().collect();
    assert_eq!(named["dose_time[1]"], 2.0);
    assert_eq!(named["dose_amount[2]"], 50.0);
    assert_eq!(named["dose_dur[2]"], 2.0);
    let c = load_md_case("model_md_pk1_iv_bolus_ss_tau_6").unwrap();
    let named: BTreeMap<String, f64> = dose_parameters(&c.regimen).into_iter().collect();
    assert_eq!(named.len(), 1);
    assert_eq!(named["tau"], 6.0);
    let c = load_md_case("model_md_pk2_oral_1_regular_n10").unwrap();
    let named: BTreeMap<String, f64> = dose_parameters(&c.regimen).into_iter().collect();
    assert_eq!((named["tau"], named["n_doses"]), (12.0, 10.0));
}

// ---------------------------------------------------------------- independent arithmetic

#[test]
fn schedules_and_regular_regimens_match_a_naive_superposition() {
    let mut checked = 0;
    for c in all_cases() {
        if c.regimen.kind == RegimenKind::SteadyState || !c.textbook_double_ok {
            continue;
        }
        let doses = doses_of(&c);
        let scale_c = c
            .case
            .times
            .iter()
            .filter_map(|t| at(&c, &format!("t={t}"), "conc"))
            .fold(0.0f64, |m, v| m.max(v.abs()));
        for (key, &t) in c.case.time_keys.iter().zip(&c.case.times) {
            let (mut conc, mut auc) = (0.0, 0.0);
            let mut ok = true;
            for &(t0, dose, dur) in &doses {
                match single_dose(&c, dose, dur, t - t0) {
                    Some((x, y)) => {
                        conc += x;
                        auc += y;
                    }
                    None => ok = false,
                }
            }
            if !ok {
                continue;
            }
            close(
                &format!("{} conc at {t}", c.case.name),
                conc,
                at(&c, key, "conc").unwrap(),
                1e-9,
                1e-12 * scale_c,
            );
            close(
                &format!("{} auc at {t}", c.case.name),
                auc,
                at(&c, key, "auc").unwrap(),
                1e-9,
                1e-12 * scale_c,
            );
            checked += 1;
        }
    }
    assert!(checked > 1500, "{checked} values compared");
}

#[test]
fn steady_state_profiles_match_a_naive_sum_of_single_doses() {
    let mut checked = 0;
    let mut cases = 0;
    for c in all_cases() {
        if c.regimen.kind != RegimenKind::SteadyState || !c.textbook_double_ok {
            continue;
        }
        let tau = c.regimen.tau.unwrap();
        let dur = c.case.parameters.get("dur").copied();
        let terms = terms_for(&c, tau);
        assert!(terms < 100_000, "{}: {terms} terms", c.case.name);
        let scale = at(&c, &format!("t={}", c.case.times[0]), "conc").unwrap_or(1.0);
        for (key, &s) in c.case.time_keys.iter().zip(&c.case.times) {
            let mut conc = 0.0;
            for j in 0..terms {
                conc += single_dose(&c, c.case.dose, dur, s + j as f64 * tau)
                    .unwrap()
                    .0;
            }
            close(
                &format!("{} conc at s = {s}", c.case.name),
                conc,
                at(&c, key, "conc").unwrap(),
                2e-9,
                1e-12 * scale,
            );
            checked += 1;
        }
        cases += 1;
    }
    assert!(cases >= 70, "{cases} cases compared");
    assert!(checked > 1200, "{checked} values compared");
}

// ---------------------------------------------------------------- identities

#[test]
fn the_derived_quantities_satisfy_their_identities() {
    for c in all_cases() {
        if c.regimen.kind != RegimenKind::SteadyState {
            continue;
        }
        let name = &c.case.name;
        let tau = c.regimen.tau.unwrap();
        let dose = c.case.dose;
        let p = &c.case.parameters;
        let cl = if c.parameterisation == "clearance" {
            p["cl"]
        } else {
            dose / scalar(&c, "auc_tau_ss").unwrap()
        };
        let cav = scalar(&c, "cav_ss").unwrap();
        let area = scalar(&c, "auc_tau_ss").unwrap();
        let cmax = scalar(&c, "cmax_ss").unwrap();
        let cmin = scalar(&c, "cmin_ss").unwrap();
        close(name, area, dose / cl, 1e-15, 0.0);
        close(name, cav, area / tau, 1e-15, 0.0);
        // the area over one interval, from the profile
        let last = format!("t={}", c.case.times.last().unwrap());
        close(name, at(&c, &last, "auc").unwrap(), area, 1e-12, 0.0);
        // the profile stays between the trough and the peak
        for key in &c.case.time_keys {
            let v = at(&c, key, "conc").unwrap();
            assert!(
                v <= cmax * (1.0 + 1e-12) && v >= cmin * (1.0 - 1e-12),
                "{name}: {key} = {v} outside [{cmin}, {cmax}]"
            );
        }
        // the single-dose peak is lower, so the accumulation of the peak is at least 1
        assert!(scalar(&c, "accum_cmax").unwrap() >= 1.0 - 1e-12, "{name}");
        let lag = p.get("tlag").copied().unwrap_or(0.0);
        match scalar(&c, "accum_auc") {
            Some(r) => assert!(r >= 1.0 - 1e-12 && lag < tau, "{name}"),
            None => assert!(
                lag >= tau,
                "{name}: accum_auc not available although the lag is below tau"
            ),
        }
        // the maximum is attained at every s only for a continuous input (tau equals the duration)
        let dur = p.get("dur").copied();
        let plateau = dur == Some(tau);
        assert_eq!(c.tmax_not_unique, plateau, "{name}");
        assert_eq!(scalar(&c, "tmax_ss").is_none(), plateau, "{name}");
        if plateau {
            close(name, cmax, cmin, 1e-12, 0.0);
            close(name, cmax, dose / tau / cl, 1e-12, 0.0);
        } else {
            let tmax = scalar(&c, "tmax_ss").unwrap();
            assert!((0.0..tau).contains(&tmax), "{name}: Tmax,ss = {tmax}");
        }
        // the one-compartment bolus: the accumulation is 1 / (1 - exp(-k tau)) three times
        if c.case.model == "pk1.iv_bolus" {
            let k = cl / p["v"];
            let r = 1.0 / (1.0 - (-k * tau).exp());
            close(name, scalar(&c, "accum_cmax").unwrap(), r, 1e-12, 0.0);
            close(name, scalar(&c, "accum_auc").unwrap(), r, 1e-12, 0.0);
            for key in &c.case.time_keys {
                close(name, at(&c, key, "accum_c").unwrap(), r, 1e-12, 0.0);
            }
            // C_ss(0) - C_ss(tau) = D / V
            let first = format!("t={}", c.case.times[0]);
            close(
                name,
                at(&c, &first, "conc").unwrap() - at(&c, &last, "conc").unwrap(),
                dose / p["v"],
                1e-9,
                0.0,
            );
        }
        // the ratio to the single dose is undefined (a not-available row) exactly where it is 0
        for key in &c.case.time_keys {
            let s: f64 = key.trim_start_matches("t=").parse().unwrap();
            let lag = p.get("tlag").copied().unwrap_or(0.0);
            let delayed = kind_of(&c) != "bolus" && s <= lag;
            let starts_at_zero = kind_of(&c) != "bolus" && s == 0.0;
            if delayed || starts_at_zero {
                assert!(at(&c, key, "accum_c").is_none(), "{name}: {key}");
            } else {
                assert!(at(&c, key, "accum_c").is_some(), "{name}: {key}");
            }
        }
    }
}

#[test]
fn n_regular_doses_approach_the_steady_state_of_the_same_interval() {
    // MOD-MD-07: after n = 40 doses the last interval is the steady state to within the fraction
    // exp(-n k tau) of the slowest exponent; compare at the same s for the one-compartment bolus.
    let n40 = load_md_case("model_md_pk1_iv_bolus_regular_n40").unwrap();
    let ss = load_md_case("model_md_pk1_iv_bolus_ss_tau_6").unwrap();
    let k = 0.2f64;
    let tau = 6.0f64;
    let frac = 1.0 - (-40.0 * k * tau).exp();
    for (key, &t) in n40.case.time_keys.iter().zip(&n40.case.times) {
        let s = t - 39.0 * tau;
        if (0.0..=tau).contains(&s) {
            if let Some(css) = ss
                .case
                .times
                .iter()
                .position(|&x| x == s)
                .and_then(|i| at(&ss, &ss.case.time_keys[i], "conc"))
            {
                close(
                    &format!("n = 40 at s = {s}"),
                    at(&n40, key, "conc").unwrap(),
                    css * frac,
                    1e-12,
                    0.0,
                );
            }
        }
    }
}

// ---------------------------------------------------------------- worked examples of section 12.2

#[test]
fn worked_example_p1_one_compartment_steady_state() {
    let g = |name: &str, key: &str, q: &str| -> Option<f64> {
        let c = load_md_case(name).unwrap();
        if key == "scalar" {
            scalar(&c, q)
        } else {
            at(&c, key, q)
        }
    };
    // IV bolus, tau = 6
    let b = "model_md_pk1_iv_bolus_ss_tau_6";
    digits("bolus Cmax,ss", g(b, "scalar", "cmax_ss"), 14.3101276069);
    digits("bolus Cmin,ss", g(b, "scalar", "cmin_ss"), 4.3101276069);
    digits(
        "bolus accumulation",
        g(b, "scalar", "accum_cmax"),
        1.4310127607,
    );
    digits("bolus Cav,ss", g(b, "scalar", "cav_ss"), 8.3333333333);
    // infusion over 2 h
    let i = "model_md_pk1_iv_infusion_ss_tau_6";
    digits("infusion C(0)", g(i, "t=0", "conc"), 5.2995680177);
    digits("infusion C(1)", g(i, "t=1", "conc"), 8.8706504872);
    digits("infusion C(2)", g(i, "t=2", "conc"), 11.7944055267);
    digits("infusion C(4)", g(i, "t=4", "conc"), 7.9060264556);
    digits("infusion Tmax,ss", g(i, "scalar", "tmax_ss"), 2.0);
    digits("infusion R_max", g(i, "scalar", "accum_cmax"), 1.4310127607);
    digits("infusion area", g(i, "scalar", "auc_tau_ss"), 50.0);
    // first-order absorption ka = 1
    let o = "model_md_pk1_oral_1_ss_ka_1_tau_6";
    digits("oral Cmin,ss", g(o, "scalar", "cmin_ss"), 5.3565981130);
    digits("oral C(1)", g(o, "t=1", "conc"), 10.0352570768);
    digits("oral C(2)", g(o, "t=2", "conc"), 10.2945620021);
    digits("oral Tmax,ss", g(o, "scalar", "tmax_ss"), 1.5669216549);
    digits("oral Cmax,ss", g(o, "scalar", "cmax_ss"), 10.4602585873);
    digits("oral R_max", g(o, "scalar", "accum_cmax"), 1.5641734930);
    digits("oral R_AUC", g(o, "scalar", "accum_auc"), 1.6022382033);
    // ka = k
    let k = "model_md_pk1_oral_1_ss_ka_k_tau_6";
    digits("ka = k C(0)", g(k, "t=0", "conc"), 7.4014171269);
    digits("ka = k C(1)", g(k, "t=1", "conc"), 8.4029961286);
    digits("ka = k Tmax,ss", g(k, "scalar", "tmax_ss"), 2.4139234358);
    digits("ka = k Cmax,ss", g(k, "scalar", "cmax_ss"), 8.8302455575);
    // flip-flop
    let f = "model_md_pk1_oral_1_ss_ka_0p1_tau_6";
    digits("flip-flop Tmax,ss", g(f, "scalar", "tmax_ss"), 2.5565923007);
    digits("flip-flop Cmax,ss", g(f, "scalar", "cmax_ss"), 8.5818460758);
    // the plateau at tau = T = 2
    let p = "model_md_pk1_iv_infusion_ss_tau_2";
    digits("plateau", g(p, "t=1", "conc"), 25.0);
}

#[test]
fn worked_example_p2_two_compartment_steady_state() {
    let g = |name: &str, key: &str, q: &str| -> Option<f64> {
        let c = load_md_case(name).unwrap();
        if key == "scalar" {
            scalar(&c, q)
        } else {
            at(&c, key, q)
        }
    };
    let b = "model_md_pk2_iv_bolus_ss_tau_12";
    digits("bolus Cmax,ss", g(b, "scalar", "cmax_ss"), 11.9156464045);
    digits("bolus Cmin,ss", g(b, "scalar", "cmin_ss"), 1.9156464045);
    digits("bolus Cav,ss", g(b, "scalar", "cav_ss"), 4.1666666667);
    let o = "model_md_pk2_oral_1_ss_ka_2_tau_12";
    digits("oral Cmin,ss", g(o, "scalar", "cmin_ss"), 2.0165022370);
    digits("oral C(1)", g(o, "t=1", "conc"), 8.0084039847);
    digits("oral C(4)", g(o, "t=4", "conc"), 4.6858678966);
    // The reader printed 0.9158084340; the root of the derivative, found here by bisection at 256 bits and
    // by an independent double-precision root finder, is 0.91580843457 (a gap of 6e-10, a question for the
    // reader: the section 12.2 digit string is a rounding of a coarser root).
    let tmax = g(o, "scalar", "tmax_ss").unwrap();
    assert!(
        (tmax - 0.9158084346).abs() < 1e-10 && (tmax - 0.9158084340).abs() < 1e-9,
        "{tmax}"
    );
    digits("oral Cmax,ss", g(o, "scalar", "cmax_ss"), 8.0267804471);
}

#[test]
fn worked_example_p3_schedules() {
    let b = load_md_case("model_md_pk1_iv_bolus_sched_n3").unwrap();
    digits("three boluses C(12)", at(&b, "t=12", "conc"), 8.6198625832);
    let i = load_md_case("model_md_pk1_iv_infusion_sched_n3").unwrap();
    digits(
        "two overlapping infusions C(4)",
        at(&i, "t=4", "conc"),
        11.6513623821,
    );
}

#[test]
fn worked_example_p4_lag_at_steady_state_and_the_printed_branch() {
    let l = load_md_case("model_md_pk1_oral_1_lag_ss_tlag_0p5_tau_6").unwrap();
    digits("lag 0.5 C(0)", at(&l, "t=0", "conc"), 5.9030730217);
    digits("lag 0.5 C(0.2)", at(&l, "t=0.2", "conc"), 5.6788852681);
    digits("lag 0.5 C(0.5)", at(&l, "t=0.5", "conc"), 5.3565981130);
    digits("lag 0.5 C(1)", at(&l, "t=1", "conc"), 8.5849507093);
    // a lag longer than the interval: the printed branch needs a negative elapsed time
    let g = load_md_case("model_md_pk1_oral_1_lag_conv_tlag_gt_tau").unwrap();
    digits("lag 8.5 C(0)", at(&g, "t=0", "conc"), 8.5043435656);
    let pf = g.printed_form.as_ref().unwrap();
    assert!(pf.differs_from_oracle);
    let i = pf.times_that_differ.iter().position(|&t| t == 0.0).unwrap();
    assert!(
        (pf.printed_values[i] - -123.17).abs() < 5e-3,
        "{}",
        pf.printed_values[i]
    );
    // zero-order absorption, T = 2
    let z = load_md_case("model_md_pk1_oral_0_lag_conv_tau_2p5_tlag_1").unwrap();
    digits(
        "tau 2.5, tlag 1: C(0)",
        at(&z, "t=0", "conc"),
        20.5207326577,
    );
    let pf = z.printed_form.as_ref().unwrap();
    assert!(pf.differs_from_oracle);
    let i = pf.times_that_differ.iter().position(|&t| t == 0.0).unwrap();
    digits(
        "printed branch (tau 2.5, tlag 1)",
        Some(pf.printed_values[i]),
        23.1500056096,
    );
    let z = load_md_case("model_md_pk1_oral_0_lag_conv_tau_3_tlag_2").unwrap();
    digits("tau 3, tlag 2: C(0)", at(&z, "t=0", "conc"), 16.7766769428);
    let pf = z.printed_form.as_ref().unwrap();
    let i = pf.times_that_differ.iter().position(|&t| t == 0.0).unwrap();
    digits(
        "printed branch (tau 3, tlag 2)",
        Some(pf.printed_values[i]),
        22.3117458968,
    );
    // tau at least tlag + T: the printed form is exact
    let z = load_md_case("model_md_pk1_oral_0_lag_ss_tau_6_tlag_1").unwrap();
    digits("tau 6, tlag 1: C(0)", at(&z, "t=0", "conc"), 6.4729069939);
    assert!(!z.printed_form.as_ref().unwrap().differs_from_oracle);
}

#[test]
fn the_convention_cases_say_where_the_printed_branch_fails() {
    // OM-17 (e): the printed branch fails exactly when the interval is shorter than the lag (first-order
    // input) or than tlag + T (zero-order input), on the times at or below the lag
    for c in all_cases() {
        let Some(pf) = &c.printed_form else { continue };
        let p = &c.case.parameters;
        let tau = c.regimen.tau.unwrap();
        let lag = p["tlag"];
        let needed = match kind_of(&c) {
            "zero" => lag + p["dur"],
            _ => lag,
        };
        if tau >= needed * (1.0 + 1e-6) {
            assert!(
                !pf.differs_from_oracle,
                "{}: the printed branch differs although tau >= tlag (+ T)",
                c.case.name
            );
        }
        if tau < needed * (1.0 - 1e-6) && kind_of(&c) == "first" {
            assert!(
                pf.differs_from_oracle,
                "{}: the printed branch should fail, tau < tlag",
                c.case.name
            );
        }
    }
}
