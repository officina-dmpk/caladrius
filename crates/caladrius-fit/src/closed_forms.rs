//! Closed-form partial derivatives of the zero-order input models (`pk1.iv_infusion`,
//! `pk1.oral_0`, `pk1.oral_0_lag`) and of first-order absorption (`pk1.oral_1`,
//! `pk1.oral_1_lag`) (FIT-JAC-02, task T-030), in the (v, k) parameterisation. D: calculus on the closed forms of `specs/models.md`.
//!
//! Notation: s = t − tlag is the time since the start of the input, T = `dur`, and
//! g(z) = (1 − e^−z)/z with g(0) = 1, the function `caladrius-models` uses for the curves. Every
//! form below is written with g and g' so that it has no difference of nearly equal terms and no
//! division by a small number: g is computed with `exp_m1`, g' with a series for small z.
//! A lag only shifts the time: ∂C/∂tlag = −∂C/∂s. Everything is 0 before the input starts
//! (s <= 0), as the concentration is.

use std::collections::BTreeMap;

/// g(z) = (1 − e^−z)/z for z >= 0, with g(0) = 1.
fn g(z: f64) -> f64 {
    if z == 0.0 { 1.0 } else { -(-z).exp_m1() / z }
}

/// g'(z) = (e^−z − g(z))/z for z >= 0, with g'(0) = −1/2. For z < 0.1 the two terms nearly
/// cancel, so the series Σ_{n>=1} n·(−1)ⁿ·z^(n−1)/(n+1)! is used instead.
fn dg(z: f64) -> f64 {
    if z < 0.1 {
        let (mut sum, mut power, mut factorial) = (0.0, 1.0, 2.0);
        for n in 1..=16 {
            let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
            sum += sign * f64::from(n) * power / factorial;
            power *= z;
            factorial *= f64::from(n + 2);
        }
        sum
    } else {
        ((-z).exp() - g(z)) / z
    }
}

/// Partial derivatives of the concentration of the zero-order input model at one time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ZeroOrder {
    pub v: f64,
    pub k: f64,
    pub dur: f64,
    /// ∂C/∂s, the derivative with respect to the time since the start of the input.
    pub s: f64,
}

/// MOD-IVI-01, AB0-01 with CL = V·k. During the input (0 < s <= T):
/// C = (D/(T·V))·s·g(ks), so ∂C/∂V = −C/V, ∂C/∂k = (D/(T·V))·s²·g'(ks), ∂C/∂T = −C/T and
/// ∂C/∂s = (D/(T·V))·e^−ks.
/// After it (s > T): C = (D/V)·g(kT)·e^−k(s−T), so ∂C/∂V = −C/V,
/// ∂C/∂k = (D/V)·e^−k(s−T)·[T·g'(kT) − (s − T)·g(kT)], ∂C/∂T = (D/V)·e^−k(s−T)·k·[g'(kT) + g(kT)]
/// (g' + g = (1 − g)/z > 0, no cancellation) and ∂C/∂s = −k·C.
/// The derivative with respect to T has a kink at s = T (the input stops): the side used is the
/// one of the model, s <= T counts as during the input.
pub(crate) fn zero_order(dose: f64, v: f64, k: f64, dur: f64, s: f64) -> ZeroOrder {
    if s <= 0.0 {
        return ZeroOrder {
            v: 0.0,
            k: 0.0,
            dur: 0.0,
            s: 0.0,
        };
    }
    if s <= dur {
        let base = dose / (dur * v);
        let z = k * s;
        let c = base * s * g(z);
        return ZeroOrder {
            v: -c / v,
            k: base * s * s * dg(z),
            dur: -c / dur,
            s: base * (-z).exp(),
        };
    }
    let scale = dose / v;
    let z = k * dur;
    let (g_z, dg_z) = (g(z), dg(z));
    let decay = (-k * (s - dur)).exp();
    let c = scale * g_z * decay;
    // Once the exponential has underflowed every term is 0; (s − T)·0 must not become inf·0.
    let k_term = if decay == 0.0 {
        0.0
    } else {
        scale * decay * (dur * dg_z - (s - dur) * g_z)
    };
    ZeroOrder {
        v: -c / v,
        k: k_term,
        dur: scale * decay * k * (dg_z + g_z),
        s: -k * c,
    }
}

/// Partial derivatives of the concentration of first-order absorption at one time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FirstOrder {
    pub v: f64,
    pub k: f64,
    pub ka: f64,
    /// ∂C/∂s, the derivative with respect to the time since the start of the input.
    pub s: f64,
}

/// MOD-AB1-01 in the symmetric form of `caladrius-models`: C = (D/V)·ka·f with
/// f = s·e^−as·g(δs), a = min(k, ka), b = max(k, ka), δ = b − a, z = δs. Then
/// ∂f/∂b = s²·e^−as·g'(z), ∂f/∂a = −s²·e^−as·(g(z) + g'(z)) (g + g' = (1 − g)/z > 0), and k, ka
/// take the roles of a, b in their order (f is symmetric in k and ka, so both sides agree at
/// ka = k); ∂C/∂V = −C/V, ∂C/∂k = (D/V)·ka·∂f/∂k, ∂C/∂ka = (D/V)·(f + ka·∂f/∂ka);
/// ∂f/∂s = e^−as·(g + z·g') − a·s·e^−as·g = e^−bs − a·s·e^−as·g(z), since g(z) + z·g'(z) = e^−z.
/// No division by ka − k anywhere, so no loss of digits near ka = k (the form
/// (e^−kt − e^−ka·t)/(ka − k) loses them twice in its derivatives).
pub(crate) fn first_order(dose: f64, v: f64, k: f64, ka: f64, s: f64) -> FirstOrder {
    if s <= 0.0 {
        return FirstOrder {
            v: 0.0,
            k: 0.0,
            ka: 0.0,
            s: 0.0,
        };
    }
    let (a, b) = (k.min(ka), k.max(ka));
    let z = (b - a) * s;
    let (g_z, dg_z) = (g(z), dg(z));
    let decay = (-a * s).exp();
    // s·e^−as and s²·e^−as are 0 once the exponential has underflowed (never inf·0).
    let (s_decay, s2_decay) = if decay == 0.0 {
        (0.0, 0.0)
    } else {
        (s * decay, s * s * decay)
    };
    let f = s_decay * g_z;
    let df_db = s2_decay * dg_z;
    let df_da = -s2_decay * (g_z + dg_z);
    let (df_dk, df_dka) = if k <= ka {
        (df_da, df_db)
    } else {
        (df_db, df_da)
    };
    let scale = dose / v;
    let c = scale * ka * f;
    FirstOrder {
        v: -c / v,
        k: scale * ka * df_dk,
        ka: scale * (f + ka * df_dka),
        s: scale * ka * ((-b * s).exp() - a * s_decay * g_z),
    }
}

/// One column per name in `names`, from the partial derivatives `at(t)` gives by parameter name
/// at each time; `None` when a name has no closed form here.
fn columns<const N: usize>(
    times: &[f64],
    names: &[String],
    at: impl Fn(f64) -> [(&'static str, f64); N],
) -> Option<Vec<Vec<f64>>> {
    let rows: Vec<[(&'static str, f64); N]> = times.iter().map(|&t| at(t)).collect();
    names
        .iter()
        .map(|name| {
            rows.iter()
                .map(|row| row.iter().find(|(n, _)| n == name).map(|(_, x)| *x))
                .collect()
        })
        .collect()
}

/// Columns of the zero-order input models, with or without a lag; `params` holds the fitted and
/// the fixed parameters, `names` the fitted ones only.
pub(crate) fn zero_order_columns(
    dose: f64,
    params: &BTreeMap<String, f64>,
    times: &[f64],
    names: &[String],
    with_lag: bool,
) -> Option<Vec<Vec<f64>>> {
    let get = |n: &str| params.get(n).copied();
    let (v, k, dur) = (get("v")?, get("k")?, get("dur")?);
    if with_lag {
        let tlag = get("tlag")?;
        columns(times, names, |t| {
            let d = zero_order(dose, v, k, dur, t - tlag);
            [("v", d.v), ("k", d.k), ("dur", d.dur), ("tlag", -d.s)]
        })
    } else {
        columns(times, names, |t| {
            let d = zero_order(dose, v, k, dur, t);
            [("v", d.v), ("k", d.k), ("dur", d.dur)]
        })
    }
}

/// Columns of first-order absorption, with or without a lag (s = t − tlag,
/// ∂C/∂tlag = −∂C/∂s).
pub(crate) fn first_order_columns(
    dose: f64,
    params: &BTreeMap<String, f64>,
    times: &[f64],
    names: &[String],
    with_lag: bool,
) -> Option<Vec<Vec<f64>>> {
    let get = |n: &str| params.get(n).copied();
    let (v, k, ka) = (get("v")?, get("k")?, get("ka")?);
    if with_lag {
        let tlag = get("tlag")?;
        columns(times, names, |t| {
            let d = first_order(dose, v, k, ka, t - tlag);
            [("v", d.v), ("k", d.k), ("ka", d.ka), ("tlag", -d.s)]
        })
    } else {
        columns(times, names, |t| {
            let d = first_order(dose, v, k, ka, t);
            [("v", d.v), ("k", d.k), ("ka", d.ka)]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FitModel;
    use caladrius_models::ModelId;

    fn named(params: &[(&str, f64)]) -> BTreeMap<String, f64> {
        params.iter().map(|(n, x)| (n.to_string(), *x)).collect()
    }

    /// Five-point central difference of the model's own predictions, step h·|θ|:
    /// [−f(θ+2h) + 8f(θ+h) − 8f(θ−h) + f(θ−2h)]/(12h), error O(h⁴) (no closed form involved).
    fn numeric(model: ModelId, params: &BTreeMap<String, f64>, name: &str, t: f64) -> f64 {
        let theta = params[name];
        let h = 1e-4 * theta.abs();
        let at = |x: f64| {
            let mut p = params.clone();
            p.insert(name.to_string(), x);
            model.predict(100.0, &p, &[t]).unwrap()[0]
        };
        (-at(theta + 2.0 * h) + 8.0 * at(theta + h) - 8.0 * at(theta - h) + at(theta - 2.0 * h))
            / (12.0 * h)
    }

    /// Every closed form against central differences of `caladrius-models`, on a grid of times
    /// and of parameter sets (fast and slow input, ka = k exactly and nearly, flip-flop ka < k,
    /// k·T small enough for the series of g'), at 1e-8 relative to the natural scale of the
    /// derivative, |∂C/∂θ| + |C/θ| (a derivative that changes sign is not measured against its
    /// own value near zero). Times within 1e-2 of a kink (start and end of the input) are left
    /// out: a difference quotient straddling a kink measures neither side.
    #[test]
    fn closed_forms_match_central_differences_on_a_grid() {
        let grid: Vec<f64> = (1..=200).map(|i| 0.0125 * f64::from(i).powf(1.5)).collect();
        type Case = (ModelId, Vec<(&'static str, f64)>);
        let mut cases: Vec<Case> = Vec::new();
        for (v, k, dur) in [
            (15.0, 0.25, 2.0),
            (40.0, 0.15, 3.0),
            (5.0, 2.0, 0.5),
            (8.0, 1e-3, 4.0),
        ] {
            for model in [ModelId::IvInfusion, ModelId::Oral0] {
                cases.push((model, vec![("v", v), ("k", k), ("dur", dur)]));
            }
            cases.push((
                ModelId::Oral0Lag,
                vec![("v", v), ("k", k), ("dur", dur), ("tlag", 0.4)],
            ));
        }
        for (v, k, ka, tlag) in [
            (30.0, 0.12, 1.2, 0.5),
            (30.0, 0.5, 0.5, 0.25),
            (30.0, 0.5, 0.5 * (1.0 + 1e-9), 0.25),
            (30.0, 0.5, 0.5 * (1.0 + 1e-4), 0.25),
            (10.0, 1.5, 0.1, 1.0),
            (10.0, 0.05, 6.0, 0.1),
        ] {
            cases.push((
                ModelId::Oral1Lag,
                vec![("v", v), ("k", k), ("ka", ka), ("tlag", tlag)],
            ));
            cases.push((ModelId::Oral1, vec![("v", v), ("k", k), ("ka", ka)]));
        }
        let mut checked = 0;
        for (model, list) in cases {
            let params = named(&list);
            let names: Vec<String> = params.keys().cloned().collect();
            let exact = model
                .analytic_derivatives(100.0, &params, &grid, &names)
                .unwrap();
            let conc = model.predict(100.0, &params, &grid).unwrap();
            let tlag = params.get("tlag").copied().unwrap_or(0.0);
            let dur = params.get("dur").copied();
            for (j, name) in names.iter().enumerate() {
                for (i, &t) in grid.iter().enumerate() {
                    let s = t - tlag;
                    let near_kink = s.abs() < 1e-2 || dur.is_some_and(|d| (s - d).abs() < 1e-2);
                    if near_kink {
                        continue;
                    }
                    let a = exact[j][i];
                    let n = numeric(model, &params, name, t);
                    let scale = n.abs() + (conc[i] / params[name]).abs();
                    assert!(
                        (a - n).abs() <= 1e-8 * scale,
                        "{model:?} d/d{name} at t = {t} ({list:?}): {a} against {n}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 15_000, "{checked}");
    }

    /// Before the input starts every derivative is 0, as the concentration is.
    #[test]
    fn derivatives_are_zero_before_the_input_starts() {
        let lag = named(&[("v", 30.0), ("k", 0.1), ("ka", 1.0), ("tlag", 0.5)]);
        let names: Vec<String> = lag.keys().cloned().collect();
        let d = ModelId::Oral1Lag
            .analytic_derivatives(100.0, &lag, &[0.0, 0.25, 0.5], &names)
            .unwrap();
        assert!(d.iter().flatten().all(|x| *x == 0.0), "{d:?}");
        let zero = named(&[("v", 30.0), ("k", 0.1), ("dur", 2.0), ("tlag", 0.5)]);
        let names: Vec<String> = zero.keys().cloned().collect();
        let d = ModelId::Oral0Lag
            .analytic_derivatives(100.0, &zero, &[0.0, 0.5], &names)
            .unwrap();
        assert!(d.iter().flatten().all(|x| *x == 0.0), "{d:?}");
    }

    /// Fixed parameters are values, not columns: only the fitted names get a column, in their
    /// order, equal to the column of the full set.
    #[test]
    fn only_the_fitted_parameters_get_a_column() {
        let p = named(&[("v", 30.0), ("k", 0.1), ("ka", 1.0), ("tlag", 0.5)]);
        let times = [1.0, 2.0, 8.0];
        let all: Vec<String> = p.keys().cloned().collect();
        let full = ModelId::Oral1Lag
            .analytic_derivatives(100.0, &p, &times, &all)
            .unwrap();
        let fitted = vec!["ka".to_string(), "v".to_string()];
        let some = ModelId::Oral1Lag
            .analytic_derivatives(100.0, &p, &times, &fitted)
            .unwrap();
        assert_eq!(some, vec![full[1].clone(), full[3].clone()]);
        // A name the model does not have has no closed form.
        let wrong = vec!["dur".to_string()];
        assert!(
            ModelId::Oral1Lag
                .analytic_derivatives(100.0, &p, &times, &wrong)
                .is_none()
        );
        // The clearance parameterisation has none either (the fit refuses it with an error).
        let cl = named(&[("v", 30.0), ("cl", 3.0), ("dur", 2.0)]);
        let names = vec!["v".to_string(), "cl".to_string()];
        assert!(
            ModelId::IvInfusion
                .analytic_derivatives(100.0, &cl, &times, &names)
                .is_none()
        );
    }

    #[test]
    fn the_series_of_g_prime_matches_the_closed_form_where_both_are_accurate() {
        for z in [0.05_f64, 0.099, 0.1, 0.101, 0.3] {
            let closed = ((-z).exp() - g(z)) / z;
            let series = {
                // dg uses the series below 0.1 only; evaluate it directly here.
                let (mut sum, mut power, mut factorial) = (0.0, 1.0, 2.0);
                for n in 1..=16 {
                    let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
                    sum += sign * f64::from(n) * power / factorial;
                    power *= z;
                    factorial *= f64::from(n + 2);
                }
                sum
            };
            assert!((closed - series).abs() <= 1e-13 * series.abs(), "{z}");
        }
        assert_eq!(dg(0.0), -0.5);
        // g' = −q/z² with q(z) = 1 − e^−z·(1 + z) = z²/2 − z³/3 + z⁴/8 − …
        for z in [1e-3, 1e-6, 1e-9] {
            let series = -0.5 + z / 3.0 - z * z / 8.0 + z * z * z / 30.0;
            assert!((dg(z) - series).abs() <= 1e-14, "{z}");
        }
        // Large z: g ~ 1/z, g' ~ −1/z², no NaN once e^−z underflows.
        assert!((dg(1e3) + 1e-6).abs() <= 1e-18);
        assert_eq!(dg(f64::INFINITY), 0.0);
    }

    #[test]
    fn huge_times_give_zero_derivatives_not_nan() {
        let d = zero_order(100.0, 10.0, 1.0, 2.0, 1e6);
        assert_eq!((d.v, d.k, d.s), (0.0, 0.0, 0.0));
        assert!(d.dur.abs() == 0.0);
        let d = first_order(100.0, 10.0, 1.0, 1.0, 1e6);
        assert_eq!((d.v, d.k, d.ka, d.s), (0.0, 0.0, 0.0, 0.0));
        // A very slow and a fast rate at a very late time: e^−as is not small, s² is large.
        let d = first_order(100.0, 10.0, 1e-6, 1.0, 1e6);
        assert!(d.v.is_finite() && d.k.is_finite() && d.ka.is_finite() && d.s.is_finite());
        let d = zero_order(100.0, 10.0, 1e-300, 2.0, 1e300);
        assert!(d.v.is_finite() && d.k.is_finite() && d.dur.is_finite() && d.s.is_finite());
    }

    fn exact_fit(
        model: ModelId,
        truth: &[(&str, f64)],
        initial: &[(&str, f64)],
        fixed: &[(&str, f64)],
    ) -> Result<crate::FitResult, crate::FitError> {
        let time = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 24.0];
        let conc = model.predict(100.0, &named(truth), &time).unwrap();
        crate::run(&crate::FitInput {
            model,
            dose: 100.0,
            time: time.to_vec(),
            conc,
            weighting: crate::Weighting::Uniform,
            initial: named(initial),
            options: crate::FitOptions {
                derivatives: crate::Derivatives::Analytic,
                convergence: 1e-12,
                max_iterations: 200,
                fixed: named(fixed),
                ..crate::FitOptions::default()
            },
        })
    }

    /// The fit with closed forms recovers exact data, with a parameter fixed in the oracle
    /// fitted instead (`dur`) and a fitted one fixed instead (`tlag`).
    #[test]
    fn analytic_fits_recover_exact_data_whatever_is_fixed() {
        let close = |r: &crate::FitResult, name: &str, x: f64| {
            let got = r.get(&format!("estimate.{name}")).unwrap();
            assert!((got - x).abs() <= 1e-7 * x, "{name}: {got} against {x}");
        };
        let truth = [("v", 40.0), ("k", 0.15), ("dur", 3.0)];
        let r = exact_fit(
            ModelId::Oral0,
            &truth,
            &[("v", 30.0), ("k", 0.2), ("dur", 2.5)],
            &[],
        )
        .unwrap();
        for (name, x) in truth {
            close(&r, name, x);
        }
        let truth = [("v", 30.0), ("k", 0.12), ("ka", 1.2), ("tlag", 0.4)];
        let r = exact_fit(
            ModelId::Oral1Lag,
            &truth,
            &[("v", 25.0), ("k", 0.1), ("ka", 1.0)],
            &[("tlag", 0.4)],
        )
        .unwrap();
        for (name, x) in &truth[..3] {
            close(&r, name, *x);
        }
        assert_eq!(r.get("estimate.tlag"), None);
        let r = exact_fit(
            ModelId::Oral1Lag,
            &truth,
            &[("v", 25.0), ("k", 0.1), ("ka", 1.0), ("tlag", 0.3)],
            &[],
        )
        .unwrap();
        for (name, x) in truth {
            close(&r, name, x);
        }
    }

    /// Without a closed form (clearance parameterisation) `analytic` is refused with a readable
    /// error, never silently replaced by finite differences.
    #[test]
    fn the_clearance_parameterisation_is_refused_with_a_readable_error() {
        let e = exact_fit(
            ModelId::IvInfusion,
            &[("v", 15.0), ("k", 0.25), ("dur", 2.0)],
            &[("v", 10.0), ("cl", 3.0)],
            &[("dur", 2.0)],
        )
        .unwrap_err();
        assert!(
            matches!(&e, crate::FitError::AnalyticDerivativesUnavailable { model } if model == "pk1.iv_infusion"),
            "{e}"
        );
        assert!(e.to_string().contains("forward differences"), "{e}");
    }
}
