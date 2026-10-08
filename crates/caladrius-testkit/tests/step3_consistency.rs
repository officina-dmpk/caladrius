#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose

//! Cross-checks the step-3 oracle (task T-009) against independent, naive arithmetic in Rust
//! (AGENTS.md section 5, source 3), so that the expected files are tested by something other than
//! the R script that wrote them:
//!
//! - models: the textbook closed forms evaluated in double precision (the formulas of
//!   `specs/models.md` written again here, without its numerical safeguards) reproduce the exact
//!   values, and the secondary parameters satisfy their relations;
//! - fits: from the oracle's own estimates, the weights, residual sum of squares, weighted sums,
//!   correlation, AIC, SBC, the first-order optimality condition `J' W r = 0`, the covariance
//!   `S^2 (J' W J)^-1`, the standard errors, the correlation matrix, the eigenvalue identities and
//!   the delta-method standard errors are recomputed.
//!
//! This tests the oracle and the testkit, not the engines.

use caladrius_testkit::{
    FitCase, ModelCase, Tolerance, list_fit_cases, list_model_cases, load_fit_case, load_model_case,
};

// ---------------------------------------------------------------- models

fn close(what: &str, actual: f64, expected: f64, rel: f64) {
    let allowed = if expected == 0.0 {
        0.0
    } else {
        rel * expected.abs()
    };
    assert!(
        (actual - expected).abs() <= allowed,
        "{what}: independent value {actual:e}, oracle {expected:e}"
    );
}

/// Textbook concentration and AUC(0, t) of a model in double precision.
fn naive_model(case: &ModelCase, t: f64) -> (f64, f64) {
    let p = |n: &str| case.parameters.get(n).copied();
    let v = p("v").unwrap();
    let k = p("k").unwrap_or_else(|| p("cl").unwrap() / v);
    let d = case.dose;
    let tlag = p("tlag").unwrap_or(0.0);
    let u = t - tlag;
    if case.model == "pk1.iv_bolus" {
        return if t < 0.0 {
            (0.0, 0.0)
        } else {
            (d / v * (-k * t).exp(), d / (v * k) * (1.0 - (-k * t).exp()))
        };
    }
    if u <= 0.0 {
        return (0.0, 0.0);
    }
    match case.model.as_str() {
        "pk1.iv_infusion" | "pk1.oral_0" | "pk1.oral_0_lag" => {
            let dur = p("dur").unwrap();
            let r = d / dur;
            if u <= dur {
                (
                    r / (v * k) * (1.0 - (-k * u).exp()),
                    r / (v * k) * (u - (1.0 - (-k * u).exp()) / k),
                )
            } else {
                let c_end = r / (v * k) * (1.0 - (-k * dur).exp());
                let auc_end = r / (v * k) * (dur - (1.0 - (-k * dur).exp()) / k);
                (
                    c_end * (-k * (u - dur)).exp(),
                    auc_end + c_end / k * (1.0 - (-k * (u - dur)).exp()),
                )
            }
        }
        "pk1.oral_1" | "pk1.oral_1_lag" => {
            let ka = p("ka").unwrap();
            if ka == k {
                (
                    d * k * u * (-k * u).exp() / v,
                    d / (v * k) * (1.0 - (-k * u).exp() * (1.0 + k * u)),
                )
            } else {
                (
                    d * ka / (v * (ka - k)) * ((-k * u).exp() - (-ka * u).exp()),
                    d / (v * (ka - k))
                        * (ka / k * (1.0 - (-k * u).exp()) - (1.0 - (-ka * u).exp())),
                )
            }
        }
        other => panic!("unknown model {other}"),
    }
}

/// The cases whose textbook form keeps all its digits (ka - k not tiny).
fn well_conditioned(case: &ModelCase) -> bool {
    match (case.parameters.get("ka"), case.parameters.get("k")) {
        (Some(ka), Some(k)) => (ka - k).abs() > 1e-2 * k || ka == k,
        (Some(ka), None) => {
            let k = case.parameters["cl"] / case.parameters["v"];
            (ka - k).abs() > 1e-2 * k
        }
        _ => true,
    }
}

#[test]
fn the_textbook_formulas_reproduce_the_exact_model_values() {
    let names = list_model_cases().unwrap();
    assert!(names.len() >= 20);
    let mut near_degenerate_skipped = 0;
    for name in names {
        let case = load_model_case(&name).unwrap();
        // ka = k is evaluated by its own limit formula and is exact in double precision.
        if !well_conditioned(&case) {
            near_degenerate_skipped += 1;
            continue;
        }
        for (t, key) in case.times.iter().zip(&case.time_keys) {
            let (c, a) = naive_model(&case, *t);
            let ec = case.expected.get(key, "conc").flatten().unwrap();
            let ea = case.expected.get(key, "auc").flatten().unwrap();
            // The textbook forms of the sum of two exponentials lose a few digits to cancellation
            // at late times: 1e-10 relative is the honest bound for an independent double
            // precision implementation (the oracle itself is exact to 1e-16).
            close(&format!("{name} conc at {t}"), c, ec, 1e-10);
            close(&format!("{name} auc at {t}"), a, ea, 1e-10);
        }
    }
    assert_eq!(
        near_degenerate_skipped, 3,
        "the cases with ka - k of 1e-3, 1e-6 and 1e-9 are the ones the textbook form cannot do"
    );
}

#[test]
fn the_secondary_parameters_satisfy_their_relations() {
    for name in list_model_cases().unwrap() {
        let case = load_model_case(&name).unwrap();
        let s = |n: &str| case.expected.get("scalar", n).flatten();
        let v = s("v").unwrap();
        let k = s("k").unwrap();
        let cl = s("cl").unwrap();
        close(&format!("{name} cl"), cl, v * k, 1e-14);
        close(
            &format!("{name} half_life"),
            s("half_life").unwrap(),
            std::f64::consts::LN_2 / k,
            1e-14,
        );
        close(
            &format!("{name} auc_inf"),
            s("auc_inf").unwrap(),
            case.dose / cl,
            1e-14,
        );
        close(
            &format!("{name} mrt_system"),
            s("mrt_system").unwrap(),
            1.0 / k,
            1e-14,
        );
        close(&format!("{name} vss"), s("vss").unwrap(), v, 1e-14);
        let tlag = case.parameters.get("tlag").copied().unwrap_or(0.0);
        let mrt = match case.model.as_str() {
            "pk1.iv_bolus" => 1.0 / k,
            "pk1.iv_infusion" | "pk1.oral_0" | "pk1.oral_0_lag" => {
                1.0 / k + case.parameters["dur"] / 2.0 + tlag
            }
            _ => 1.0 / k + 1.0 / case.parameters["ka"] + tlag,
        };
        close(&format!("{name} mrt"), s("mrt").unwrap(), mrt, 1e-13);
        if case.model == "pk1.iv_bolus" {
            close(
                &format!("{name} c0"),
                s("c0").unwrap(),
                case.dose / v,
                1e-14,
            );
        }
        // AUC at the last grid time cannot exceed AUC to infinity, and is monotone.
        let mut previous = 0.0;
        for key in &case.time_keys {
            let a = case.expected.get(key, "auc").flatten().unwrap();
            assert!(
                a >= previous - 1e-15 && a <= s("auc_inf").unwrap() * (1.0 + 1e-12),
                "{name} {key}"
            );
            previous = a;
        }
        // The peak is not below any grid value.
        if let Some(cmax) = s("cmax_pred") {
            for key in &case.time_keys {
                let c = case.expected.get(key, "conc").flatten().unwrap();
                assert!(
                    c <= cmax * (1.0 + 1e-12),
                    "{name} {key}: {c} above Cmax {cmax}"
                );
            }
        }
    }
}

#[test]
fn parameterisations_and_the_flip_flop_pair_give_identical_values() {
    let same = |a: &str, b: &str| {
        let (a, b) = (load_model_case(a).unwrap(), load_model_case(b).unwrap());
        for key in &a.time_keys {
            for q in ["conc", "auc"] {
                let (x, y) = (
                    a.expected.get(key, q).flatten().unwrap(),
                    b.expected.get(key, q).flatten().unwrap(),
                );
                close(
                    &format!("{} against {} {key} {q}", a.name, b.name),
                    x,
                    y,
                    1e-14,
                );
            }
        }
    };
    same("model_iv_bolus_cl", "model_iv_bolus_k");
    same("model_oral_1", "model_oral_1_k");
    // The flip-flop partner has the same concentrations (MOD-AB1-06); its AUC is the same too.
    same("model_oral_1_k", "model_oral_1_flip_flop");
}

// ---------------------------------------------------------------- fits

/// Concentration of a fitted model at `t`. `dur` is the fixed duration of the infusion and of the
/// zero-order input (ignored by the other models). Textbook forms (`specs/models.md` MOD-IVI-01,
/// MOD-AB0-01, MOD-AB1-01, MOD-AB1-05), written again here, valid for `t` after the lag.
fn model_fn(model: &str, d: f64, dur: f64, th: &[f64], t: f64) -> f64 {
    match model {
        "pk1.iv_bolus" => d / th[0] * (-th[1] * t).exp(),
        "pk1.oral_1" => {
            let (v, k, ka) = (th[0], th[1], th[2]);
            d * ka / (v * (ka - k)) * ((-k * t).exp() - (-ka * t).exp())
        }
        "pk1.iv_infusion" | "pk1.oral_0" => {
            let (v, k) = (th[0], th[1]);
            let r = d / dur;
            if t <= dur {
                r / (v * k) * (1.0 - (-k * t).exp())
            } else {
                r / (v * k) * (1.0 - (-k * dur).exp()) * (-k * (t - dur)).exp()
            }
        }
        "pk1.oral_1_lag" => {
            let (v, k, ka, tlag) = (th[0], th[1], th[2], th[3]);
            assert!(
                t > tlag,
                "a sample at or before the lag has no smooth model"
            );
            let s = t - tlag;
            d * ka / (v * (ka - k)) * ((-k * s).exp() - (-ka * s).exp())
        }
        other => panic!("unknown model {other}"),
    }
}

/// Central-difference Jacobian row (an implementation independent of the analytic one in R).
fn jacobian_row(model: &str, d: f64, dur: f64, th: &[f64], t: f64) -> Vec<f64> {
    (0..th.len())
        .map(|j| {
            let h = 1e-6 * th[j];
            let (mut up, mut dn) = (th.to_vec(), th.to_vec());
            up[j] += h;
            dn[j] -= h;
            (model_fn(model, d, dur, &up, t) - model_fn(model, d, dur, &dn, t)) / (2.0 * h)
        })
        .collect()
}

fn invert(m: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut r = row.clone();
            r.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            r
        })
        .collect();
    for c in 0..n {
        let pivot = (c..n)
            .max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))
            .unwrap();
        a.swap(c, pivot);
        let p = a[c][c];
        for x in a[c].iter_mut() {
            *x /= p;
        }
        for r in 0..n {
            if r != c {
                let f = a[r][c];
                let pivot_row = a[c].clone();
                for (x, y) in a[r].iter_mut().zip(pivot_row) {
                    *x -= f * y;
                }
            }
        }
    }
    a.into_iter().map(|r| r[n..].to_vec()).collect()
}

/// Determinant by Gaussian elimination with partial pivoting.
fn determinant(m: &[Vec<f64>]) -> f64 {
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut det = 1.0;
    for c in 0..n {
        let pivot = (c..n)
            .max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))
            .unwrap();
        if a[pivot][c] == 0.0 {
            return 0.0;
        }
        if pivot != c {
            a.swap(pivot, c);
            det = -det;
        }
        det *= a[c][c];
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            let pivot_row = a[c].clone();
            for (x, y) in a[r].iter_mut().zip(pivot_row) {
                *x -= f * y;
            }
        }
    }
    det
}

fn parameter_names(model: &str) -> &'static [&'static str] {
    match model {
        "pk1.iv_bolus" | "pk1.iv_infusion" | "pk1.oral_0" => &["v", "k"],
        "pk1.oral_1" => &["v", "k", "ka"],
        "pk1.oral_1_lag" => &["v", "k", "ka", "tlag"],
        other => panic!("unknown model {other}"),
    }
}

/// Student quantiles t(0.975; df) for the degrees of freedom present in the oracle.
fn student_975(df: usize) -> f64 {
    match df {
        3 => 3.1824463053,
        7 => 2.3646242510,
        9 => 2.2621571628,
        10 => 2.2281388520,
        other => panic!("no tabulated quantile for {other} degrees of freedom"),
    }
}

fn check_subject(case: &FitCase, subject: &str) {
    let name = &case.name;
    let get = |n: &str| {
        case.expected
            .get(subject, n)
            .flatten()
            .unwrap_or_else(|| panic!("{name} {subject} {n}"))
    };
    let names = parameter_names(&case.model);
    let th: Vec<f64> = names
        .iter()
        .map(|p| get(&format!("estimate.{p}")))
        .collect();
    let profile = case.profile(subject).unwrap();
    let (t, y) = case.fitted_observations(profile);
    let d = profile.dose;
    let dur = case.fixed.get("dur").copied().unwrap_or(f64::NAN);
    let (n, p) = (y.len(), th.len());
    assert_eq!(get("n") as usize, n);
    assert_eq!(get("p") as usize, p);
    let df = n - p;
    assert_eq!(get("df") as usize, df);
    let yhat: Vec<f64> = t
        .iter()
        .map(|&x| model_fn(&case.model, d, dur, &th, x))
        .collect();
    let w: Vec<f64> = y
        .iter()
        .zip(&yhat)
        .map(|(&obs, &pred)| match case.weighting.as_str() {
            "uniform" => 1.0,
            "inv_y" => 1.0 / obs,
            "inv_y2" => 1.0 / (obs * obs),
            "inv_yhat" => 1.0 / pred,
            "inv_yhat2" => 1.0 / (pred * pred),
            other => panic!("unknown weighting {other}"),
        })
        .collect();
    let r: Vec<f64> = y.iter().zip(&yhat).map(|(a, b)| a - b).collect();
    let wrss: f64 = w.iter().zip(&r).map(|(w, r)| w * r * r).sum();
    close(&format!("{name} {subject} wrss"), wrss, get("wrss"), 1e-9);
    close(
        &format!("{name} {subject} s"),
        (wrss / df as f64).sqrt(),
        get("s"),
        1e-9,
    );
    let sw: f64 = w.iter().sum();
    close(
        &format!("{name} {subject} ss_weighted"),
        w.iter().zip(&y).map(|(w, y)| w * y * y).sum(),
        get("ss_weighted"),
        1e-9,
    );
    let ybar = w.iter().zip(&y).map(|(w, y)| w * y).sum::<f64>() / sw;
    let fbar = w.iter().zip(&yhat).map(|(w, f)| w * f).sum::<f64>() / sw;
    let syy: f64 = w.iter().zip(&y).map(|(w, y)| w * (y - ybar).powi(2)).sum();
    let sff: f64 = w
        .iter()
        .zip(&yhat)
        .map(|(w, f)| w * (f - fbar).powi(2))
        .sum();
    let syf: f64 = (0..n)
        .map(|i| w[i] * (y[i] - ybar) * (yhat[i] - fbar))
        .sum();
    close(
        &format!("{name} {subject} ss_corrected"),
        syy,
        get("ss_corrected"),
        1e-9,
    );
    close(
        &format!("{name} {subject} corr_obs_pred"),
        syf / (syy * sff).sqrt(),
        get("corr_obs_pred"),
        1e-9,
    );
    close(
        &format!("{name} {subject} aic"),
        n as f64 * wrss.ln() + 2.0 * p as f64,
        get("aic"),
        1e-9,
    );
    close(
        &format!("{name} {subject} sbc"),
        n as f64 * wrss.ln() + p as f64 * (n as f64).ln(),
        get("sbc"),
        1e-9,
    );

    // Weighted Jacobian, first-order optimality and covariance.
    let jac: Vec<Vec<f64>> = t
        .iter()
        .map(|&x| jacobian_row(&case.model, d, dur, &th, x))
        .collect();
    for j in 0..p {
        let gradient: f64 = (0..n).map(|i| jac[i][j] * w[i] * r[i]).sum();
        // The sum is zero to the cancellation of its own terms. (Not relative to a scale that
        // contains the weights: a degenerate point with predictions of 1e-90 and weights of 1e90
        // passed that test in the first version of this check, task T-018.)
        let terms: f64 = (0..n).map(|i| (jac[i][j] * w[i] * r[i]).abs()).sum();
        assert!(
            gradient.abs() <= 1e-6 * terms,
            "{name} {subject}: J'Wr for {} is {gradient:e} against terms of size {terms:e}: not a stationary point",
            names[j]
        );
    }
    let jtj: Vec<Vec<f64>> = (0..p)
        .map(|a| {
            (0..p)
                .map(|b| (0..n).map(|i| w[i] * jac[i][a] * jac[i][b]).sum())
                .collect()
        })
        .collect();
    let inv = invert(&jtj);
    let s2 = wrss / df as f64;
    let se: Vec<f64> = (0..p).map(|j| (s2 * inv[j][j]).sqrt()).collect();
    for a in 0..p {
        for b in a..p {
            close(
                &format!("{name} {subject} covariance.{}.{}", names[a], names[b]),
                s2 * inv[a][b],
                get(&format!("covariance.{}.{}", names[a], names[b])),
                1e-5,
            );
            if b > a {
                close(
                    &format!("{name} {subject} correlation.{}.{}", names[a], names[b]),
                    inv[a][b] / (inv[a][a] * inv[b][b]).sqrt(),
                    get(&format!("correlation.{}.{}", names[a], names[b])),
                    1e-5,
                );
            }
        }
        close(
            &format!("{name} {subject} se.{}", names[a]),
            se[a],
            get(&format!("se.{}", names[a])),
            1e-5,
        );
        close(
            &format!("{name} {subject} cv_percent.{}", names[a]),
            100.0 * se[a] / th[a].abs(),
            get(&format!("cv_percent.{}", names[a])),
            1e-5,
        );
        // Intervals: estimate -/+ t * SE, with the tabulated Student quantile (10 digits): compared
        // to 1e-9 of the size of the estimate plus the half-width, since a limit can be near zero.
        let q = student_975(df);
        let half = q * get(&format!("se.{}", names[a]));
        for (side, sign) in [("lo", -1.0), ("hi", 1.0)] {
            let key = format!("ci_{side}.{}", names[a]);
            let independent = th[a] + sign * half;
            assert!(
                (independent - get(&key)).abs() <= 1e-9 * (th[a].abs() + half),
                "{name} {subject} {key}: independent value {independent:e}, oracle {:e}",
                get(&key)
            );
        }
        // Planar interval: a constant multiple of the SE, wider than the univariate one.
        let planar =
            (get(&format!("planar_hi.{}", names[a])) - th[a]) / get(&format!("se.{}", names[a]));
        assert!(
            planar > q,
            "{name} {subject}: planar factor {planar} not above {q}"
        );
        close(
            &format!("{name} {subject} planar symmetry {}", names[a]),
            th[a] - get(&format!("planar_lo.{}", names[a])),
            get(&format!("planar_hi.{}", names[a])) - th[a],
            1e-9,
        );
    }
    // The planar factor is the same for every parameter of the subject; for P = 2 and 3 degrees of
    // freedom it is sqrt(2 F(0.95; 2, 3)) = 4.3706 (specs/fit.md section 11).
    if p == 2 && df == 3 {
        let factor = (get("planar_hi.v") - th[0]) / get("se.v");
        close(&format!("{name} planar factor"), factor, 4.3706, 1e-4);
    }
    // Eigenvalues of the correlation matrix: trace P, product equal to its determinant,
    // condition number = largest / smallest.
    let ev: Vec<f64> = (1..=p).map(|i| get(&format!("eigenvalue.{i}"))).collect();
    assert!(
        ev.windows(2).all(|x| x[0] >= x[1]),
        "{name} {subject}: eigenvalues not decreasing"
    );
    close(
        &format!("{name} {subject} eigenvalue sum"),
        ev.iter().sum(),
        p as f64,
        1e-9,
    );
    close(
        &format!("{name} {subject} condition_number"),
        ev[0] / ev[p - 1],
        get("condition_number"),
        1e-9,
    );
    let corr = |a: usize, b: usize| {
        if a == b {
            1.0
        } else {
            let (a, b) = (a.min(b), a.max(b));
            get(&format!("correlation.{}.{}", names[a], names[b]))
        }
    };
    let det = determinant(
        &(0..p)
            .map(|a| (0..p).map(|b| corr(a, b)).collect())
            .collect::<Vec<Vec<f64>>>(),
    );
    close(
        &format!("{name} {subject} eigenvalue product"),
        ev.iter().product(),
        det,
        1e-8,
    );

    // Secondary parameters by the delta method.
    let (v, k) = (th[0], th[1]);
    let cov = |a: usize, b: usize| {
        get(&format!(
            "covariance.{}.{}",
            names[a.min(b)],
            names[a.max(b)]
        ))
    };
    let quad = |g: &[f64]| {
        let mut sum = 0.0;
        for a in 0..g.len() {
            for b in 0..g.len() {
                sum += g[a] * g[b] * cov(a, b);
            }
        }
        sum.sqrt()
    };
    let pad = |mut g: Vec<f64>| {
        g.resize(p, 0.0);
        g
    };
    close(
        &format!("{name} {subject} cl"),
        v * k,
        get("estimate.cl"),
        1e-12,
    );
    close(
        &format!("{name} {subject} half_life"),
        std::f64::consts::LN_2 / k,
        get("estimate.half_life"),
        1e-12,
    );
    close(
        &format!("{name} {subject} auc_inf"),
        d / (v * k),
        get("estimate.auc_inf"),
        1e-12,
    );
    close(
        &format!("{name} {subject} se.cl"),
        quad(&pad(vec![k, v])),
        get("se.cl"),
        1e-9,
    );
    close(
        &format!("{name} {subject} se.half_life"),
        quad(&pad(vec![0.0, -std::f64::consts::LN_2 / (k * k)])),
        get("se.half_life"),
        1e-9,
    );
    close(
        &format!("{name} {subject} se.auc_inf"),
        quad(&pad(vec![-d / (v * v * k), -d / (v * k * k)])),
        get("se.auc_inf"),
        1e-9,
    );
}

#[test]
fn every_reference_fit_is_a_stationary_point_with_the_documented_statistics() {
    let names = list_fit_cases().unwrap();
    assert_eq!(names.len(), 30);
    let mut fits = 0;
    for name in names {
        let case = load_fit_case(&name).unwrap();
        for subject in &case.subjects {
            check_subject(&case, subject);
            fits += 1;
        }
    }
    // 12 Theoph subjects x 5 + 6 Indometh x 5 + 5 spec; T-029: 6 subjects x 5 weightings for each
    // of the synthetic infusion, zero-order and lag datasets.
    assert_eq!(fits, 12 * 5 + 6 * 5 + 5 + 3 * 6 * 5);
}

/// Task T-018: every subject of every case has a fixed point (or a minimum). The first version of
/// the oracle declared five Indometh `inv_yhat2` subjects without one and accepted degenerate
/// points (V of 0.004, k of 28, WRSS of 1e89) for four `inv_yhat` subjects; the stationarity
/// condition `J' W r = 0` now has to hold, to the cancellation of its own terms, at every point.
#[test]
fn every_subject_has_a_genuine_fixed_point_and_the_estimates_are_plausible() {
    for name in list_fit_cases().unwrap() {
        let case = load_fit_case(&name).unwrap();
        assert!(
            case.no_fixed_point.is_empty(),
            "{name}: {:?}",
            case.no_fixed_point
        );
        let in_dataset = case.dataset.profiles.len();
        assert_eq!(case.subjects.len(), in_dataset, "{name}");
        for subject in &case.subjects {
            let v = case.expected.get(subject, "estimate.v").flatten().unwrap();
            let k = case.expected.get(subject, "estimate.k").flatten().unwrap();
            let wrss = case.expected.get(subject, "wrss").flatten().unwrap();
            assert!(
                v > 1.0 && v < 1000.0 && k > 1e-3 && k < 10.0,
                "{name} {subject}: V {v}, k {k}"
            );
            assert!(wrss < 1e3, "{name} {subject}: WRSS {wrss:e}");
        }
    }
}

#[test]
fn the_estimates_differ_between_weightings() {
    // A guard against five cases that test the same thing: the weighting moves the estimates.
    let v = |case: &str| {
        load_fit_case(case)
            .unwrap()
            .expected
            .get("1", "estimate.v")
            .flatten()
            .unwrap()
    };
    let uniform = v("fit_theoph_uniform");
    for other in [
        "fit_theoph_inv_y",
        "fit_theoph_inv_y2",
        "fit_theoph_inv_yhat",
        "fit_theoph_inv_yhat2",
    ] {
        assert!(
            (v(other) - uniform).abs() > 1e-3 * uniform,
            "{other} equals the uniform fit"
        );
    }
    // The tolerances of the fit tests are looser than the weighting effect, not the other way round.
    assert!(!Tolerance::FIT_PARAMETERS.accepts(v("fit_theoph_inv_y2"), uniform));
}
