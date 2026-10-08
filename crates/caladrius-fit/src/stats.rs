//! Quantiles of Student's t and Fisher's F for the confidence intervals (FIT-OUT-03, OUT-04),
//! through the regularized incomplete beta function. Standard numerical analysis: the Lanczos
//! approximation of ln Γ, a continued fraction for the incomplete beta (modified Lentz), and
//! bisection for the inverse, to about 1e-14 relative.

/// ln Γ(x) for x > 0 (Lanczos, g = 7, nine coefficients).
fn ln_gamma(x: f64) -> f64 {
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection: Γ(x)Γ(1 − x) = π / sin(πx).
        let pi = std::f64::consts::PI;
        return (pi / (pi * x).sin()).abs().ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = 0.0;
    for (i, c) in C.iter().enumerate().rev() {
        a += if i == 0 { *c } else { c / (x + i as f64) };
    }
    let t = x + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Continued fraction of the incomplete beta function (modified Lentz).
fn beta_continued_fraction(a: f64, b: f64, x: f64) -> f64 {
    const TINY: f64 = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=500 {
        let m = f64::from(m);
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h
}

/// Regularized incomplete beta I_x(a, b) for a, b > 0 and 0 <= x <= 1.
pub(crate) fn incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front =
        (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_continued_fraction(a, b, x) / a
    } else {
        1.0 - front * beta_continued_fraction(b, a, 1.0 - x) / b
    }
}

/// The x in (0, 1) with I_x(a, b) = p, by bisection.
fn inverse_incomplete_beta(p: f64, a: f64, b: f64) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if incomplete_beta(mid, a, b) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-17 * hi {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Quantile t(q; df) of Student's t for 0.5 < q < 1 and df > 0.
pub(crate) fn student_t_quantile(q: f64, df: f64) -> f64 {
    // P(|T| > t) = I_{df/(df + t²)}(df/2, 1/2) = 2(1 − q).
    let x = inverse_incomplete_beta(2.0 * (1.0 - q), df / 2.0, 0.5);
    (df * (1.0 - x) / x).sqrt()
}

/// Quantile F(q; d1, d2) of Fisher's F for 0 < q < 1.
pub(crate) fn f_quantile(q: f64, d1: f64, d2: f64) -> f64 {
    // P(F <= f) = I_{d1 f / (d1 f + d2)}(d1/2, d2/2).
    let x = inverse_incomplete_beta(q, d1 / 2.0, d2 / 2.0);
    d2 * x / (d1 * (1.0 - x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_of_worked_example_f2() {
        // specs/fit.md section 11: t(0.975; 3) = 3.1824463053, F(0.95; 2, 3) = 9.5520944959
        // (closed form (3/2)(0.05^(−2/3) − 1) for two numerator degrees of freedom).
        assert!((student_t_quantile(0.975, 3.0) - 3.182_446_305_284_263).abs() < 1e-10);
        let f = f_quantile(0.95, 2.0, 3.0);
        let exact = 1.5 * (0.05f64.powf(-2.0 / 3.0) - 1.0);
        assert!((f - exact).abs() < 1e-9 * exact, "{f} against {exact}");
        // Normal limit and a textbook value.
        assert!((student_t_quantile(0.975, 1e7) - 1.959_963_984_540_054).abs() < 1e-5);
        assert!((student_t_quantile(0.975, 10.0) - 2.228_138_851_986_274).abs() < 1e-10);
        assert!((ln_gamma(5.0) - 24f64.ln()).abs() < 1e-13);
    }
}
