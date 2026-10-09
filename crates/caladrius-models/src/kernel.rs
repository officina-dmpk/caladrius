//! The one-compartment kernel Φ(λ, s) of `specs/models.md` MOD-2C-06: the concentration that the
//! one-compartment model with volume 1 and elimination rate constant λ (so CL = λ) gives for the
//! same dose and input parameters, s the time since the start of the input. Every two-compartment
//! quantity is the positive combination (1/Vc)·[wα·Φ(α) + wβ·Φ(β)] of these kernels: the
//! concentration and AUC (the stable forms of `curves`, MOD-AB1-03), the first moment AUMC(0, s)
//! (MOD-2C-14) and the partial derivatives (MOD-2C-18).
//!
//! Every form below has no positive exponent and no difference of nearly equal numbers, except
//! where a derivative really changes sign; a product with an exponential that has underflowed is
//! 0, never inf·0 (MOD-NUM-01, MOD-2C-22).

use crate::curves::{self, g, q};
use crate::model::Input;
use crate::params::Params;

/// g'(z) = (e^−z − g(z))/z for z >= 0, with g'(0) = −1/2; the series
/// Σ_{n>=1} n·(−1)ⁿ·z^(n−1)/(n+1)! below 0.1, where the two terms nearly cancel.
pub(crate) fn dg(z: f64) -> f64 {
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

/// ψ(y) = q(y)/y² = ∫₀¹ x·e^(−y·x) dx for y > 0, by its series below 0.1.
fn psi(y: f64) -> f64 {
    if y < 0.1 {
        // Σ_{n>=2} (−1)ⁿ (n − 1) y^(n−2)/n!
        let (mut sum, mut power) = (0.0, 0.5);
        for n in 2..=22 {
            sum += f64::from(n - 1) * power;
            power *= -y / f64::from(n + 1);
        }
        sum
    } else {
        q(y) / (y * y)
    }
}

/// ρ(y) = (y²/2 − q(y))/y³ = Σ_{n>=3} (−1)^(n+1) (n − 1) y^(n−3)/n!, for 0 <= y <= 1.
fn rho(y: f64) -> f64 {
    let (mut sum, mut power) = (0.0, 1.0 / 6.0);
    for n in 3..=26 {
        sum += f64::from(n - 1) * power;
        power *= -y / f64::from(n + 1);
    }
    sum
}

/// h(λ, s) = ∫₀ˢ u·e^(−λu) du = q(λs)/λ², as s²·ψ(λs) for λs <= 1 (no overflow of 1/λ²).
fn moment_of_exponential(rate: f64, s: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    let y = rate * s;
    if y <= 1.0 {
        s * s * psi(y)
    } else {
        q(y) / rate / rate
    }
}

/// ψ_n(x) = ∫₀¹ vⁿ·e^(−x·v) dv for n = 2..=N+2 and 0 <= x < 3 (index i holds ψ_{i+2}): the
/// highest by its series of positive terms e^−x/(n+1)·Σ_k x^k/((n+2)…(n+1+k)), the others by the
/// backward recurrence ψ_(n−1) = (e^−x + x·ψ_n)/n (positive terms, stable).
fn psi_n(x: f64) -> [f64; MOMENT_TERMS] {
    let top = (MOMENT_TERMS + 1) as f64; // n of the last entry
    let decay = (-x).exp();
    let (mut sum, mut term) = (0.0, 1.0);
    for k in 0..80 {
        sum += term;
        term *= x / (top + 2.0 + f64::from(k));
        if term < 1e-18 * sum {
            break;
        }
    }
    let mut out = [0.0; MOMENT_TERMS];
    let mut current = decay / (top + 1.0) * sum;
    let mut n = top;
    for slot in out.iter_mut().rev() {
        *slot = current;
        current = (decay + x * current) / n;
        n -= 1.0;
    }
    out
}

/// Number of terms of the series in δ of [`first_order_moment`] (and of ψ_n).
const MOMENT_TERMS: usize = 21;

/// K(s) = ∫₀ˢ u·f(u) du with f(u) = (e^−au − e^−bu)/(b − a) = u·e^−au·g(δu), a = min(k, ka),
/// b = max(k, ka), δ = b − a: the first moment of the first-order kernel divided by D·ka.
/// Three regimes, each without cancellation of more than a few bits (x = a·s, z = δ·s):
/// x >= 3: K = [(a + b)(1 − e^−x) − e^−x·x·b − e^−x·g(z)·(x·a + x²·b)]/(ab)² (the moment to
/// infinity minus the tail, which is at most 42 % of it);
/// x < 3 and z >= 1: the divided difference [h(a, s) − h(b, s)]/δ of the exponential moments;
/// x < 3 and z < 1: s³·Σ_m (−z)^m/(m + 1)!·ψ_(m+2)(x), the expansion of g(δu) in δ.
fn first_order_moment(k: f64, ka: f64, s: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    let (a, b) = (k.min(ka), k.max(ka));
    let delta = b - a;
    let (x, z) = (a * s, delta * s);
    if x >= 3.0 {
        moment_by_tail(a, b, s)
    } else if z >= 1.0 {
        moment_by_difference(a, b, s)
    } else {
        moment_by_series(a, b, s)
    }
}

/// The regime x = a·s >= 3 of [`first_order_moment`].
fn moment_by_tail(a: f64, b: f64, s: f64) -> f64 {
    let (x, z) = (a * s, (b - a) * s);
    let decay = (-x).exp();
    let ab = a * b;
    if decay == 0.0 {
        return (a + b) / ab / ab;
    }
    let numerator = (a + b) * -(-x).exp_m1() - decay * x * b - decay * g(z) * (x * a + x * x * b);
    numerator / ab / ab
}

/// The regime x < 3, z = δ·s >= 1 of [`first_order_moment`].
fn moment_by_difference(a: f64, b: f64, s: f64) -> f64 {
    (moment_of_exponential(a, s) - moment_of_exponential(b, s)) / (b - a)
}

/// The regime x < 3, z < 1 of [`first_order_moment`].
fn moment_by_series(a: f64, b: f64, s: f64) -> f64 {
    let (x, z) = (a * s, (b - a) * s);
    let psis = psi_n(x);
    let (mut sum, mut factor) = (0.0, 1.0);
    for (m, psi_m) in psis.iter().enumerate() {
        sum += factor * psi_m;
        factor *= -z / (m as f64 + 2.0);
    }
    s * s * s * sum
}

/// One-compartment response with volume 1 for one dose and set of input parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Kernel {
    pub input: Input,
    pub dose: f64,
    pub ka: f64,
    pub dur: f64,
}

/// Φ and its partial derivatives at one (λ, s).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Partials {
    pub value: f64,
    /// ∂Φ/∂λ.
    pub rate: f64,
    pub ka: f64,
    pub dur: f64,
    /// ∂Φ/∂s.
    pub s: f64,
}

impl Kernel {
    fn params(&self, rate: f64) -> Params {
        Params {
            v: 1.0,
            k: rate,
            cl: rate,
            ka: self.ka,
            dur: self.dur,
            tlag: 0.0,
        }
    }

    /// Φ(λ, s) and its area from the start of the input (`curves`, MOD-AB1-03).
    pub fn conc_auc(&self, rate: f64, s: f64) -> (f64, f64) {
        curves::at(self.input, self.dose, &self.params(rate), s)
    }

    /// ∫₀ˢ u·Φ(λ, u) du, the first moment from the start of the input (MOD-2C-14).
    pub fn moment(&self, rate: f64, s: f64) -> f64 {
        if s <= 0.0 {
            return 0.0;
        }
        let dose = self.dose;
        match self.input {
            Input::Bolus => dose * moment_of_exponential(rate, s),
            Input::FirstOrder => dose * self.ka * first_order_moment(rate, self.ka, s),
            Input::ZeroOrder => {
                let t = self.dur;
                // During the input: (D/T)·∫₀ˢ u·(1 − e^−λu)/λ du = (D/T)·s³·ρ(λs).
                let during = |s: f64| {
                    let y = rate * s;
                    if y <= 1.0 {
                        dose / t * s * s * s * rho(y)
                    } else {
                        dose / t * ((y * y / 2.0 - q(y)) / rate / rate / rate)
                    }
                };
                if s <= t {
                    return during(s);
                }
                // After: M(T) + C(T)·[h(λ, τ) + T·τ·g(λτ)], τ = s − T, C(T) = D·g(λT).
                let tau = s - t;
                let c_end = dose * g(rate * t);
                during(t) + c_end * (moment_of_exponential(rate, tau) + t * tau * g(rate * tau))
            }
        }
    }

    /// ∂Φ/∂s, for the peak search of first-order input (MOD-2C-15).
    pub fn slope(&self, rate: f64, s: f64) -> f64 {
        self.partials(Rate::plain(rate), s).s
    }

    /// Φ and its partial derivatives (`specs/fit.md` FIT-JAC-02 with V = 1, k = λ): 0 before the
    /// start of the input, as Φ is (the bolus is D at s = 0).
    pub fn partials(&self, rate: Rate, s: f64) -> Partials {
        let dose = self.dose;
        match self.input {
            Input::Bolus => {
                if s < 0.0 {
                    return Partials::default();
                }
                let value = dose * rate.decay(s);
                Partials {
                    value,
                    rate: if value == 0.0 { 0.0 } else { -s * value },
                    s: -rate.value() * value,
                    ..Partials::default()
                }
            }
            Input::ZeroOrder => self.zero_order(rate, s),
            Input::FirstOrder => self.first_order(rate, s),
        }
    }

    /// During the input (0 < s <= T): Φ = (D/T)·s·g(λs), ∂λ = (D/T)·s²·g'(λs), ∂T = −Φ/T,
    /// ∂s = (D/T)·e^−λs. After it: Φ = D·g(λT)·e^−λ(s−T), ∂λ = D·e^−λ(s−T)·[T·g'(λT) − (s − T)·g(λT)],
    /// ∂T = D·e^−λ(s−T)·λ·(g' + g)(λT) (g + g' = (1 − g)/z > 0), ∂s = −λ·Φ.
    fn zero_order(&self, rate: Rate, s: f64) -> Partials {
        if s <= 0.0 {
            return Partials::default();
        }
        let (dose, t, lambda) = (self.dose, self.dur, rate.value());
        if s <= t {
            let base = dose / t;
            let z = lambda * s;
            let value = base * s * g(z);
            return Partials {
                value,
                rate: base * s * s * dg(z),
                dur: -value / t,
                s: base * rate.decay(s),
                ka: 0.0,
            };
        }
        let z = lambda * t;
        let (g_z, dg_z) = (g(z), dg(z));
        let decay = rate.decay(s - t);
        let value = dose * g_z * decay;
        let rate_term = if decay == 0.0 {
            0.0
        } else {
            dose * decay * (t * dg_z - (s - t) * g_z)
        };
        Partials {
            value,
            rate: rate_term,
            dur: dose * decay * lambda * (dg_z + g_z),
            s: -lambda * value,
            ka: 0.0,
        }
    }

    /// Φ = D·ka·f with f = s·e^−as·g(z), a = min(λ, ka), b = max(λ, ka), z = (b − a)·s, x = a·s:
    /// ∂f/∂b = s²·e^−as·g'(z), ∂f/∂a = −s²·e^−as·(g + g')(z) (g + g' = (1 − g)/z > 0);
    /// ∂Φ/∂λ = D·ka·∂f/∂λ; ∂Φ/∂s = D·ka·e^−as·(e^−z − x·g(z)). For ∂Φ/∂ka = D·(f + ka·∂f/∂ka):
    /// when ka = b, f + b·∂f/∂b = s·e^−as·(g + (x + z)·g') = s·e^−as·(e^−z + x·g'(z)) (since
    /// g + z·g' = e^−z), which does not cancel when ka ≫ λ; when ka = a,
    /// s·e^−as·(g − x·(g + g')). No division by ka − λ anywhere.
    fn first_order(&self, rate: Rate, s: f64) -> Partials {
        if s <= 0.0 {
            return Partials::default();
        }
        let (dose, ka, lambda) = (self.dose, self.ka, rate.value());
        let rate_is_slower = lambda <= ka;
        let (a, b) = if rate_is_slower {
            (lambda, ka)
        } else {
            (ka, lambda)
        };
        let (x, z) = (a * s, (b - a) * s);
        let (g_z, dg_z) = (g(z), dg(z));
        let decay = if rate_is_slower {
            rate.decay(s)
        } else {
            (-ka * s).exp()
        };
        let (s_decay, s2_decay) = if decay == 0.0 {
            (0.0, 0.0)
        } else {
            (s * decay, s * s * decay)
        };
        let f = s_decay * g_z;
        let (df_drate, ka_bracket) = if rate_is_slower {
            (-s2_decay * (g_z + dg_z), (-z).exp() + x * dg_z)
        } else {
            (s2_decay * dg_z, g_z - x * (g_z + dg_z))
        };
        Partials {
            value: dose * ka * f,
            rate: dose * ka * df_drate,
            ka: dose * s_decay * ka_bracket,
            s: dose * ka * decay * ((-z).exp() - x * g_z),
            dur: 0.0,
        }
    }
}

/// A rate constant λ = center + offset. The exponentials e^(−λ·u) are formed as
/// e^(−center·u)·e^(−offset·u): when α and β are close, both are written about their mean, so the
/// rounding of the large argument center·u is common to every term of a derivative and cancels
/// out of the differences between the terms (MOD-2C-19, 2C-21); the small offsets keep their
/// digits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rate {
    pub center: f64,
    pub offset: f64,
}

impl Rate {
    /// λ itself, with no offset.
    pub fn plain(rate: f64) -> Rate {
        Rate {
            center: rate,
            offset: 0.0,
        }
    }

    fn value(&self) -> f64 {
        self.center + self.offset
    }

    /// e^(−λ·u).
    fn decay(&self, u: f64) -> f64 {
        let main = (-self.center * u).exp();
        if self.offset == 0.0 || main == 0.0 {
            main
        } else {
            main * (-self.offset * u).exp()
        }
    }
}

/// Time of the peak of the first-order kernel (MOD-AB1-04), the ends of the bracket of
/// MOD-2C-15.
pub(crate) fn first_order_peak(rate: f64, ka: f64) -> f64 {
    curves::first_order_peak_time(&Params {
        v: 1.0,
        k: rate,
        cl: rate,
        ka,
        dur: 0.0,
        tlag: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Composite Simpson's rule with many panels on a smooth integrand.
    fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
        let h = (b - a) / n as f64;
        let mut sum = f(a) + f(b);
        for i in 1..n {
            let x = a + h * i as f64;
            sum += if i % 2 == 0 { 2.0 } else { 4.0 } * f(x);
        }
        sum * h / 3.0
    }

    /// The first moment of every input against a numerical integral of u·Φ(u), across the
    /// regimes of `first_order_moment` (x below and above 3, z below and above 1) and of the
    /// zero-order forms (λs below and above 1, during and after the input).
    #[test]
    fn moments_match_numerical_integrals() {
        let cases = [
            (Input::FirstOrder, 0.2, 2.0, 0.0),
            (Input::FirstOrder, 0.2, 0.2, 0.0),
            (Input::FirstOrder, 0.2, 0.2001, 0.0),
            (Input::FirstOrder, 1.0, 0.1, 0.0),
            (Input::FirstOrder, 0.1, 500.0, 0.0),
            (Input::ZeroOrder, 0.3, 0.0, 2.0),
            (Input::ZeroOrder, 0.01, 0.0, 5.0),
            (Input::Bolus, 0.7, 0.0, 0.0),
        ];
        for (input, rate, ka, dur) in cases {
            let k = Kernel {
                input,
                dose: 100.0,
                ka,
                dur,
            };
            for s in [0.003, 0.05, 0.4, 1.0, 2.0, 3.5, 7.0, 15.0, 40.0] {
                let exact = k.moment(rate, s);
                // Split at the end of the input, where the integrand has a kink.
                let integrand = |u: f64| u * k.conc_auc(rate, u).0;
                let numeric = if input == Input::ZeroOrder && s > dur {
                    simpson(integrand, 0.0, dur, 20_000) + simpson(integrand, dur, s, 20_000)
                } else {
                    simpson(integrand, 0.0, s, 20_000)
                };
                assert!(
                    (exact - numeric).abs() <= 1e-9 * numeric.abs(),
                    "{input:?} rate {rate} ka {ka} s {s}: {exact} against {numeric}"
                );
            }
        }
    }

    /// The regimes of the first-order moment join without a jump (x = 3, z = 1).
    #[test]
    fn the_regimes_of_the_first_order_moment_join() {
        // Each pair of regimes at a point near their common boundary, where both are accurate.
        for (a, b, s) in [(1.0, 1.5, 3.0), (1.0, 1.2, 3.0), (1.0, 4.0, 3.0)] {
            let (x, y) = (moment_by_tail(a, b, s), moment_by_difference(a, b, s));
            assert!(
                (x - y).abs() <= 1e-14 * x,
                "tail/difference {a} {b} {s}: {x} {y}"
            );
        }
        for (a, b, s) in [(1.0, 1.25, 4.0), (1.0, 1.1, 2.9), (0.1, 0.6, 2.0)] {
            let (x, y) = (moment_by_series(a, b, s), moment_by_difference(a, b, s));
            assert!(
                (x - y).abs() <= 1e-14 * x,
                "series/difference {a} {b} {s}: {x} {y}"
            );
        }
        for (a, b, s) in [(1.0, 1.2, 3.0), (1.0, 1.0, 3.0), (0.5, 0.7, 4.0)] {
            let (x, y) = (moment_by_series(a, b, s), moment_by_tail(a, b, s));
            assert!(
                (x - y).abs() <= 1e-14 * x,
                "series/tail {a} {b} {s}: {x} {y}"
            );
        }
        // At ka = k the moment is ∫ u² e^−ku du = (2/k³)[1 − e^−y(1 + y + y²/2)].
        for s in [1.0_f64, 2.9, 3.1, 30.0] {
            let y = 0.2 * s;
            let exact = 2.0 / 0.008 * (1.0 - (-y).exp() * (1.0 + y + y * y / 2.0));
            let got = first_order_moment(0.2, 0.2, s);
            assert!((got - exact).abs() <= 1e-12 * exact, "{s}: {got} {exact}");
        }
        // Small s, where that closed form cancels: its series s³/3 − k·s⁴/4 + k²·s⁵/10 − k³·s⁶/36.
        let (k, s) = (0.2_f64, 0.01_f64);
        let series = s.powi(3) / 3.0 - k * s.powi(4) / 4.0 + k * k * s.powi(5) / 10.0
            - k.powi(3) * s.powi(6) / 36.0;
        let got = first_order_moment(k, k, s);
        assert!((got - series).abs() <= 1e-12 * series, "{got} {series}");
    }

    #[test]
    fn huge_times_give_finite_moments_and_zero_derivatives() {
        let k = Kernel {
            input: Input::FirstOrder,
            dose: 1.0,
            ka: 1.0,
            dur: 0.0,
        };
        assert!(k.moment(0.5, 1e300).is_finite());
        let p = k.partials(Rate::plain(0.5), 1e300);
        assert_eq!((p.value, p.rate, p.ka, p.s), (0.0, 0.0, 0.0, 0.0));
        let z = Kernel {
            input: Input::ZeroOrder,
            dose: 1.0,
            ka: 0.0,
            dur: 2.0,
        };
        assert!(z.moment(0.5, 1e300).is_finite());
        assert!(psi(0.0) == 0.5 && (rho(0.0) - 1.0 / 6.0 * 2.0).abs() < 1e-16);
    }
}
