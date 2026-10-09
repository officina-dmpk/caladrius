//! Concentration and AUC(0, t) of the one-compartment models, in forms that keep full double
//! precision (MOD-AB1-03, MOD-NUM-01): exponentials of non-positive arguments only, `expm1` for
//! `1 - e^-x`, and series where two terms of nearly equal size would cancel.
//!
//! Notation: `s` is the time since the start of the input (time since the dose minus the lag).

use crate::model::Input;
use crate::params::Params;

/// g(z) = (1 − e^−z)/z for z ≥ 0, with g(0) = 1.
pub(crate) fn g(z: f64) -> f64 {
    if z == 0.0 { 1.0 } else { -(-z).exp_m1() / z }
}

/// 1 − g(z) for z ≥ 0 = z/2! − z²/3! + z³/4! − …, without cancellation for small z.
pub(crate) fn one_minus_g(z: f64) -> f64 {
    if z < 0.1 {
        let (mut sum, mut term) = (0.0, z / 2.0);
        for n in 1..=20 {
            sum += term;
            term *= -z / f64::from(n + 2);
        }
        sum
    } else {
        1.0 - g(z)
    }
}

/// q(y) = 1 − e^−y·(1 + y) for y ≥ 0 = Σ_{n≥2} (−1)^n (n − 1) yⁿ/n!, without cancellation.
pub(crate) fn q(y: f64) -> f64 {
    if y.is_infinite() {
        return 1.0;
    }
    if y < 0.1 {
        let (mut sum, mut power) = (0.0, y * y / 2.0);
        for n in 2..=22 {
            sum += f64::from(n - 1) * power;
            power *= -y / f64::from(n + 1);
        }
        sum
    } else {
        -(-y).exp_m1() - y * (-y).exp()
    }
}

/// Concentration and AUC(0, t) at time `t` since the dose.
pub(crate) fn at(input: Input, dose: f64, p: &Params, t: f64) -> (f64, f64) {
    match input {
        Input::Bolus => bolus(dose, p, t),
        Input::ZeroOrder => zero_order(dose, p, t - p.tlag),
        Input::FirstOrder => first_order(dose, p, t - p.tlag),
    }
}

/// MOD-IVB-01, IVB-02: C = (D/V)·e^−kt, AUC = (D/CL)·(1 − e^−kt). Nothing before the dose.
fn bolus(dose: f64, p: &Params, t: f64) -> (f64, f64) {
    if t < 0.0 {
        return (0.0, 0.0);
    }
    (
        dose / p.v * (-p.k * t).exp(),
        dose / p.cl * -(-p.k * t).exp_m1(),
    )
}

/// MOD-IVI-01, AB0-01: input at rate D/T during T, then free decay.
/// During: C = (D/(T·CL))·(1 − e^−ks), AUC = (D/(T·CL))·s·(1 − g(ks)).
/// After: C = C(T)·e^−k(s−T), AUC = AUC(T) + C(T)·(1 − e^−k(s−T))/k.
fn zero_order(dose: f64, p: &Params, s: f64) -> (f64, f64) {
    if s <= 0.0 {
        return (0.0, 0.0);
    }
    let plateau = dose / (p.dur * p.cl);
    let during = |s: f64| {
        (
            plateau * -(-p.k * s).exp_m1(),
            plateau * s * one_minus_g(p.k * s),
        )
    };
    if s <= p.dur {
        return during(s);
    }
    let (c_end, auc_end) = during(p.dur);
    let after = s - p.dur;
    (
        c_end * (-p.k * after).exp(),
        auc_end + c_end * -(-p.k * after).exp_m1() / p.k,
    )
}

/// MOD-AB1-01 to 03, symmetric in k and ka: with a = min(k, ka) and δ = |ka − k|,
/// C = (D/V)·ka·s·e^−as·g(δs) and AUC = (D/CL)·[q(as) + as·e^−as·(1 − g(δs))].
/// Both are exact rewritings of the textbook forms, continuous at ka = k (g(0) = 1), with no
/// difference of nearly equal terms and no positive exponent.
fn first_order(dose: f64, p: &Params, s: f64) -> (f64, f64) {
    if s <= 0.0 {
        return (0.0, 0.0);
    }
    let a = p.k.min(p.ka);
    let delta = (p.ka - p.k).abs();
    let (y, z) = (a * s, delta * s);
    let decay = (-y).exp();
    // s·e^−as and as·e^−as first, and 0 once the exponential has underflowed: s or as may be huge
    // (or infinite) when e^−as is 0, and their product must not become inf·0 (MOD-NUM-01).
    let (s_decay, y_decay) = if decay == 0.0 {
        (0.0, 0.0)
    } else {
        (s * decay, y * decay)
    };
    (
        dose / p.v * p.ka * s_decay * g(z),
        dose / p.cl * (q(y) + y_decay * one_minus_g(z)),
    )
}

/// Time of the peak after the start of first-order absorption (MOD-AB1-04):
/// ln(ka/k)/(ka − k), with ln(ka/k) as ln_1p((ka − k)/k) near ka = k (where ka/k loses digits)
/// and as ln(ka/k) when ka ≪ k (where ln_1p of a number near −1 loses them); 1/k when ka = k.
pub(crate) fn first_order_peak_time(p: &Params) -> f64 {
    let d = p.ka - p.k;
    let ratio = p.ka / p.k;
    if d == 0.0 {
        1.0 / p.k
    } else if ratio < 0.5 {
        ratio.ln() / d
    } else {
        (d / p.k).ln_1p() / d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs()
    }

    #[test]
    fn series_match_the_closed_forms_where_both_are_accurate() {
        for z in [0.05, 0.099, 0.1, 0.11, 0.3] {
            assert!(close(one_minus_g(z), 1.0 - g(z), 1e-13), "{z}");
            assert!(close(q(z), -(-z).exp_m1() - z * (-z).exp(), 1e-13), "{z}");
        }
        // MOD-AB1-03: g(x) against its series 1 − x/2 + x²/6 − x³/24 for |x| <= 1e-3.
        for x in [1e-3, 1e-6, 1e-9] {
            let series = 1.0 - x / 2.0 + x * x / 6.0 - x * x * x / 24.0;
            assert!(close(g(x), series, 1e-12), "{x}");
        }
        assert_eq!(g(0.0), 1.0);
        assert_eq!(one_minus_g(0.0), 0.0);
        assert_eq!(q(0.0), 0.0);
    }
}
