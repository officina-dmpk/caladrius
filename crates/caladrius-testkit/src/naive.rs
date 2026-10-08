//! Naive, independent computations used to cross-check the oracle itself and, later, cases that have
//! no reference (AGENTS.md section 5, source 3). They are written for clarity, not speed, and share
//! no code with `caladrius-nca`: the engine is never tested against itself.

/// Trapezoid rule for the area under the curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Linear interpolation everywhere.
    Linear,
    /// Linear while the concentration rises or stays equal, logarithmic while it falls (both positive).
    LinUpLogDown,
}

/// Area of one trapezoid between `(t1, c1)` and `(t2, c2)`.
fn segment_auc(rule: Rule, t1: f64, c1: f64, t2: f64, c2: f64) -> f64 {
    let dt = t2 - t1;
    match rule {
        Rule::LinUpLogDown if c2 < c1 && c1 > 0.0 && c2 > 0.0 => dt * (c1 - c2) / (c1 / c2).ln(),
        _ => dt * (c1 + c2) / 2.0,
    }
}

/// Area of one trapezoid of the first moment curve `t * c(t)`.
fn segment_aumc(rule: Rule, t1: f64, c1: f64, t2: f64, c2: f64) -> f64 {
    let dt = t2 - t1;
    match rule {
        Rule::LinUpLogDown if c2 < c1 && c1 > 0.0 && c2 > 0.0 => {
            // Integral of t * c(t) with c(t) = c1 * exp(-k (t - t1)), k = ln(c1 / c2) / dt:
            // [-(t / k + 1 / k^2) c(t)] between t1 and t2.
            let k = (c1 / c2).ln() / dt;
            (t1 * c1 - t2 * c2) / k + (c1 - c2) / (k * k)
        }
        _ => dt * (t1 * c1 + t2 * c2) / 2.0,
    }
}

/// Pairs of consecutive points, or an empty list when the lengths differ.
fn pairs<'a>(
    time: &'a [f64],
    conc: &'a [f64],
) -> impl Iterator<Item = ((f64, f64), (f64, f64))> + 'a {
    let n = if time.len() == conc.len() {
        time.len()
    } else {
        0
    };
    time.iter().copied().zip(conc.iter().copied()).take(n).zip(
        time.iter()
            .copied()
            .zip(conc.iter().copied())
            .take(n)
            .skip(1),
    )
}

/// Area under the curve from the first to the last point of the profile.
/// Returns `None` if `time` and `conc` differ in length or hold fewer than two points.
pub fn auc(rule: Rule, time: &[f64], conc: &[f64]) -> Option<f64> {
    if time.len() != conc.len() || time.len() < 2 {
        return None;
    }
    Some(
        pairs(time, conc)
            .map(|((t1, c1), (t2, c2))| segment_auc(rule, t1, c1, t2, c2))
            .sum(),
    )
}

/// Area under the first moment curve (`t * c(t)`) from the first to the last point.
/// Returns `None` if `time` and `conc` differ in length or hold fewer than two points.
pub fn aumc(rule: Rule, time: &[f64], conc: &[f64]) -> Option<f64> {
    if time.len() != conc.len() || time.len() < 2 {
        return None;
    }
    Some(
        pairs(time, conc)
            .map(|((t1, c1), (t2, c2))| segment_aumc(rule, t1, c1, t2, c2))
            .sum(),
    )
}

/// Least-squares line `y = intercept + slope * x`, with the coefficient of determination.
/// Returns `None` for fewer than two points, mismatched lengths or constant `x`.
pub fn linear_regression(x: &[f64], y: &[f64]) -> Option<LineFit> {
    let n = x.len();
    if n < 2 || y.len() != n {
        return None;
    }
    let nf = n as f64;
    let mx = x.iter().sum::<f64>() / nf;
    let my = y.iter().sum::<f64>() / nf;
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let syy: f64 = y.iter().map(|v| (v - my).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let r_squared = if syy == 0.0 {
        1.0
    } else {
        sxy * sxy / (sxx * syy)
    };
    Some(LineFit {
        slope,
        intercept,
        r_squared,
    })
}

/// Result of [`linear_regression`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineFit {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_rule_on_a_triangle() {
        let a = auc(Rule::Linear, &[0.0, 1.0, 2.0], &[0.0, 2.0, 0.0]).unwrap();
        assert!((a - 2.0).abs() < 1e-15);
    }

    #[test]
    fn log_down_is_exact_for_a_mono_exponential() {
        // c(t) = 8 * exp(-0.5 t) between t = 1 and t = 5: the integral is 16 (exp(-0.5) - exp(-2.5)).
        let (c1, c2) = (8.0 * (-0.5f64).exp(), 8.0 * (-2.5f64).exp());
        let a = auc(Rule::LinUpLogDown, &[1.0, 5.0], &[c1, c2]).unwrap();
        let exact = 16.0 * ((-0.5f64).exp() - (-2.5f64).exp());
        assert!((a - exact).abs() < 1e-12, "{a} vs {exact}");
    }

    #[test]
    fn log_down_aumc_is_exact_for_a_mono_exponential() {
        // Integral of t * 8 exp(-0.5 t) from 1 to 5 = [-(2t + 4) * 8 exp(-0.5 t) / ... ]: use the
        // antiderivative F(t) = -16 (t + 2) exp(-0.5 t).
        let f = |t: f64| -16.0 * (t + 2.0) * (-0.5 * t).exp();
        let exact = f(5.0) - f(1.0);
        let (c1, c2) = (8.0 * (-0.5f64).exp(), 8.0 * (-2.5f64).exp());
        let a = aumc(Rule::LinUpLogDown, &[1.0, 5.0], &[c1, c2]).unwrap();
        assert!((a - exact).abs() < 1e-10, "{a} vs {exact}");
    }

    #[test]
    fn rising_segments_stay_linear_and_zero_falls_do_not_blow_up() {
        let lin = auc(Rule::Linear, &[0.0, 1.0], &[1.0, 3.0]).unwrap();
        let mixed = auc(Rule::LinUpLogDown, &[0.0, 1.0], &[1.0, 3.0]).unwrap();
        assert_eq!(lin, mixed);
        let fall_to_zero = auc(Rule::LinUpLogDown, &[0.0, 1.0], &[2.0, 0.0]).unwrap();
        assert!((fall_to_zero - 1.0).abs() < 1e-15);
    }

    #[test]
    fn degenerate_inputs_return_none() {
        assert_eq!(auc(Rule::Linear, &[0.0], &[1.0]), None);
        assert_eq!(auc(Rule::Linear, &[0.0, 1.0], &[1.0]), None);
        assert_eq!(aumc(Rule::Linear, &[], &[]), None);
        assert_eq!(linear_regression(&[1.0], &[1.0]), None);
        assert_eq!(linear_regression(&[1.0, 1.0], &[1.0, 2.0]), None);
    }

    #[test]
    fn regression_recovers_a_line() {
        let f = linear_regression(&[0.0, 1.0, 2.0, 3.0], &[1.0, 3.0, 5.0, 7.0]).unwrap();
        assert!((f.slope - 2.0).abs() < 1e-12 && (f.intercept - 1.0).abs() < 1e-12);
        assert!((f.r_squared - 1.0).abs() < 1e-12);
    }
}
