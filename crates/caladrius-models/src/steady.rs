//! The steady-state kernel Φ_ss(λ, u) = Σ over j >= 0 of Φ(λ, u + j·τ) (`specs/models.md` MOD-MD-03,
//! 04, 06): the periodic profile of the one-compartment kernel of `kernel` (volume 1, rate λ) under
//! a dose D every τ, u the time since the start of the last input, 0 <= u <= τ, without lag (the lag
//! is a phase shift, MOD-MD-05, applied by the caller). Every two-compartment steady state is the
//! positive combination (1/Vc)·[wα·Φ_ss(α) + wβ·Φ_ss(β)], as for the single dose.
//!
//! The geometric sums are arranged so that no term is negative and no difference of nearly equal
//! numbers is formed (MOD-MD-12): 1 − e^(−λτ) is −expm1(−λτ); for first-order input, with
//! a = min(λ, ka), b = max(λ, ka), δ = b − a,
//! Φ_ss(u) = D·ka·[u·e^(−au)·g(δu)/(1 − e^(−aτ)) + K·e^(−bu)], K = e^(−aτ)·τ·g(δτ)/((1 − e^(−aτ))(1 − e^(−bτ))),
//! which is the textbook difference of two geometric sums divided by ka − λ rewritten with g, and
//! continuous at ka = λ (MOD-MD-04: the limit is the same expression with δ = 0). The areas are
//! exact integrals of these forms over any sub-interval [u1, u1 + Δ], written the same way, so that
//! a short interval keeps its relative precision.

use crate::curves::{g, one_minus_g};
use crate::kernel::dg;
use crate::model::Input;
use crate::two::{GL_NODES, GL_WEIGHTS};

/// h(z) = (1 − g(z))/z for z >= 0, with h(0) = 1/2.
fn h(z: f64) -> f64 {
    if z == 0.0 {
        0.5
    } else {
        one_minus_g(z) / z
    }
}

/// (g(x) − g(y))/(y − x) for 0 <= x <= y, minus the mean of g′ over [x, y], always > 0: the mean
/// by 8-point Gauss–Legendre when the interval is short on the scale of g (where the difference
/// would cancel), the difference itself otherwise (it then keeps all but a few bits).
fn g_drop(x: f64, y: f64) -> f64 {
    let len = y - x;
    if len <= 0.0 {
        return -dg(x);
    }
    if len <= 1.0_f64.max(0.25 * x) {
        let (center, half) = (x + len / 2.0, len / 2.0);
        let mut sum = 0.0;
        for (node, weight) in GL_NODES.iter().zip(GL_WEIGHTS) {
            sum += weight * (dg(center + half * node) + dg(center - half * node));
        }
        -sum / 2.0
    } else {
        (g(x) - g(y)) / len
    }
}

/// One dose D every τ into the kernel of volume 1 (input parameters `ka`, `dur` as in `kernel`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Steady {
    pub input: Input,
    pub dose: f64,
    pub ka: f64,
    pub dur: f64,
    pub tau: f64,
}

/// The constants of the first-order form at one rate: a, b, δ, 1 − e^(−aτ), K.
struct FirstOrder {
    a: f64,
    b: f64,
    delta: f64,
    den_a: f64,
    k: f64,
}

impl Steady {
    /// 1 − e^(−λτ).
    fn den(&self, rate: f64) -> f64 {
        -(-rate * self.tau).exp_m1()
    }

    fn first_order(&self, rate: f64) -> FirstOrder {
        let (a, b) = (rate.min(self.ka), rate.max(self.ka));
        let delta = b - a;
        let tau = self.tau;
        let (den_a, den_b) = (self.den(a), self.den(b));
        let k = (-a * tau).exp() * tau * g(delta * tau) / den_a / den_b;
        FirstOrder {
            a,
            b,
            delta,
            den_a,
            k,
        }
    }

    /// The zero-order constants: R = D/T, K1 = D·g(λT)/(1 − e^(−λτ)) (the earlier doses at the end
    /// of the last input, summed) and K2 = K1·e^(−λ(τ − T)) (their sum at u = 0).
    fn zero_order(&self, rate: f64) -> (f64, f64, f64) {
        let t = self.dur;
        let k1 = self.dose * g(rate * t) / self.den(rate);
        (self.dose / t, k1, k1 * (-rate * (self.tau - t)).exp())
    }

    /// Φ_ss(λ, u) for 0 <= u <= τ. The bolus value at u = 0 is the one just after the dose
    /// (MOD-MD-03).
    pub fn conc(&self, rate: f64, u: f64) -> f64 {
        match self.input {
            Input::Bolus => self.dose * (-rate * u).exp() / self.den(rate),
            Input::ZeroOrder => {
                let (r, k1, k2) = self.zero_order(rate);
                if u <= self.dur {
                    r * u * g(rate * u) + k2 * (-rate * u).exp()
                } else {
                    k1 * (-rate * (u - self.dur)).exp()
                }
            }
            Input::FirstOrder => {
                let f = self.first_order(rate);
                let decay = (-f.a * u).exp();
                let u_decay = if decay == 0.0 { 0.0 } else { u * decay };
                self.dose
                    * self.ka
                    * (u_decay * g(f.delta * u) / f.den_a + f.k * (-f.b * u).exp())
            }
        }
    }

    /// ∂Φ_ss/∂u for first-order input (0 otherwise; their peak times are known, MOD-MD-10).
    pub fn slope(&self, rate: f64, u: f64) -> f64 {
        if self.input != Input::FirstOrder {
            return 0.0;
        }
        let f = self.first_order(rate);
        let (x, z) = (f.a * u, f.delta * u);
        let build = (-f.a * u).exp() * ((-z).exp() - x * g(z)) / f.den_a;
        self.dose * self.ka * (build - f.b * f.k * (-f.b * u).exp())
    }

    /// ∫ Φ_ss(λ, v) dv over [u1, u1 + du], with 0 <= u1 and u1 + du <= τ.
    pub fn area(&self, rate: f64, u1: f64, du: f64) -> f64 {
        if du <= 0.0 {
            return 0.0;
        }
        // ∫ e^(−λv) dv over [u1, u1 + d] = e^(−λ·u1)·d·g(λd).
        let exp_area = |rate: f64, u1: f64, d: f64| (-rate * u1).exp() * d * g(rate * d);
        match self.input {
            Input::Bolus => self.dose * exp_area(rate, u1, du) / self.den(rate),
            Input::ZeroOrder => {
                let t = self.dur;
                let (r, k1, k2) = self.zero_order(rate);
                // Split at the end of the input; the lengths are formed from du and t − u1, never
                // from u1 + du, so that a short interval keeps its digits.
                if u1 < t {
                    let to_end = t - u1;
                    let d = du.min(to_end);
                    // The last input building up, R·(1 − e^(−λv))/λ, and the earlier doses, K2·e^(−λv).
                    let build = r
                        * d
                        * (u1 * g(rate * u1) + (-rate * u1).exp() * d * h(rate * d));
                    let mut total = build + k2 * exp_area(rate, u1, d);
                    if du > to_end {
                        total += k1 * exp_area(rate, 0.0, du - to_end);
                    }
                    total
                } else {
                    k1 * exp_area(rate, u1 - t, du)
                }
            }
            Input::FirstOrder => {
                let f = self.first_order(rate);
                // ∫ v·e^(−av)·g(δv) dv = Δ·e^(−a·u1)·[Δ·(g(aΔ) − g(bΔ))/(δΔ) + u1·g(δ·u1)·g(bΔ)].
                let decay = (-f.a * u1).exp();
                let built = if decay == 0.0 {
                    0.0
                } else {
                    du * decay
                        * (du * g_drop(f.a * du, f.b * du)
                            + u1 * g(f.delta * u1) * g(f.b * du))
                };
                self.dose * self.ka * (built / f.den_a + f.k * exp_area(f.b, u1, du))
            }
        }
    }

    /// Time u of the maximum of Φ_ss(λ, ·) for first-order input (MOD-MD-10), by bisection on the
    /// sign of the slope over [0, τ].
    pub fn first_order_peak(&self, rate: f64) -> f64 {
        bisect(0.0, self.tau, |u| self.slope(rate, u))
    }
}

/// The root of a slope that is positive before it and negative after it, in [lo, hi], to two
/// adjacent doubles (as the single-dose peak search of MOD-2C-15).
pub(crate) fn bisect(mut lo: f64, mut hi: f64, slope: impl Fn(f64) -> f64) -> f64 {
    for _ in 0..2200 {
        let mid = lo + (hi - lo) / 2.0;
        if mid <= lo || mid >= hi {
            break;
        }
        let d = slope(mid);
        if d > 0.0 {
            lo = mid;
        } else if d < 0.0 {
            hi = mid;
        } else {
            return mid;
        }
    }
    lo + (hi - lo) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::Kernel;

    /// Φ_ss against the plain sum of the single-dose kernel, Φ_ss′ against a central difference,
    /// and the area against Simpson's rule, for every input, both sides of ka = λ and at it.
    #[test]
    fn steady_kernel_matches_the_sum_of_single_doses() {
        let cases = [
            (Input::Bolus, 0.2, 0.0, 0.0, 6.0),
            (Input::ZeroOrder, 0.2, 0.0, 2.0, 6.0),
            (Input::ZeroOrder, 0.2, 0.0, 2.0, 2.0),
            (Input::FirstOrder, 0.2, 1.0, 0.0, 6.0),
            (Input::FirstOrder, 0.2, 0.2, 0.0, 6.0),
            (Input::FirstOrder, 0.2, 0.2000002, 0.0, 6.0),
            (Input::FirstOrder, 0.2, 0.1, 0.0, 6.0),
            (Input::FirstOrder, 1.0, 500.0, 0.0, 0.5),
        ];
        for (input, rate, ka, dur, tau) in cases {
            let steady = Steady {
                input,
                dose: 100.0,
                ka,
                dur,
                tau,
            };
            let single = Kernel {
                input,
                dose: 100.0,
                ka,
                dur,
            };
            for u in [0.0, 0.3, 1.0, 2.0, 3.5, tau] {
                let sum: f64 = (0..4000)
                    .map(|j| single.conc_auc(rate, u + f64::from(j) * tau).0)
                    .sum();
                let got = steady.conc(rate, u);
                assert!(
                    (got - sum).abs() <= 1e-13 * sum,
                    "{input:?} {ka} {tau} u = {u}: {got} against {sum}"
                );
                if input == Input::FirstOrder && u > 0.0 && u < tau {
                    let h = 1e-6;
                    let numeric = (steady.conc(rate, u + h) - steady.conc(rate, u - h)) / (2.0 * h);
                    let exact = steady.slope(rate, u);
                    assert!(
                        (numeric - exact).abs() <= 1e-6 * exact.abs().max(1.0),
                        "slope {ka} u = {u}: {exact} against {numeric}"
                    );
                }
            }
            // Areas: the whole interval is D/λ (MOD-MD-09), and a few pieces against Simpson.
            let whole = steady.area(rate, 0.0, tau);
            assert!(
                (whole - 100.0 / rate).abs() <= 1e-13 * whole,
                "{input:?} {ka}: {whole}"
            );
            for (u1, du) in [(0.0, 0.7), (0.4, 1e-6), (1.5, 0.5), (2.5, tau - 2.5)] {
                if du <= 0.0 || u1 + du > tau {
                    continue;
                }
                let n = 20_000;
                let step = du / f64::from(n);
                let mut sum = steady.conc(rate, u1) + steady.conc(rate, u1 + du);
                for i in 1..n {
                    let w = if i % 2 == 0 { 2.0 } else { 4.0 };
                    sum += w * steady.conc(rate, u1 + step * f64::from(i));
                }
                let simpson = sum * step / 3.0;
                let got = steady.area(rate, u1, du);
                // Simpson is exact to its own error only where the integrand is smooth.
                let kink = input == Input::ZeroOrder && u1 < dur && u1 + du > dur;
                if !kink {
                    assert!(
                        (got - simpson).abs() <= 1e-10 * simpson,
                        "{input:?} {ka} [{u1}, +{du}]: {got} against {simpson}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_drop_of_g_is_continuous_across_its_two_forms() {
        for (x, y) in [(0.5, 1.5), (4.0, 5.0), (8.0, 10.0), (40.0, 50.0), (0.0, 1.0)] {
            let quadrature = {
                let len = y - x;
                let (center, half) = (x + len / 2.0, len / 2.0);
                let mut sum = 0.0;
                for (node, weight) in GL_NODES.iter().zip(GL_WEIGHTS) {
                    sum += weight * (dg(center + half * node) + dg(center - half * node));
                }
                -sum / 2.0
            };
            let difference = (g(x) - g(y)) / (y - x);
            assert!(
                (quadrature - difference).abs() <= 1e-14 * difference,
                "[{x}, {y}]: {quadrature} against {difference}"
            );
        }
        assert_eq!(g_drop(2.0, 2.0), -dg(2.0));
    }
}
