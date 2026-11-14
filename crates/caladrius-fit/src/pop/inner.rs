//! The inner problem of one subject (POP-INN-01, POP-INN-02): the conditional mode
//! η̂ = argmin h(η), h(η) = Σ_j [ln σ_j(η)² + r_j(η)²/σ_j(η)²] + ηᵀΩ⁻¹η (FOCEI), or, with σ held at
//! the population prediction (method `foce`, POP-OBJ-06), h⁰(η) = Σ_j r_j(η)²/(σ⁰_j)² + ηᵀΩ⁻¹η.
//!
//! The step is the scoring step of the stacked system of POP-INN-02 (rows u_j with residual z_j;
//! rows √2·l_j with residual (z_j² − 1)/√2, left out when σ is held; rows L⁻¹ with residual −L⁻¹η),
//! solved by QR on unit-length columns. 𝒥ᵀ𝒥 is the expected information plus the prior precision,
//! positive definite for every η, so the step is a descent direction wherever h is defined
//! (requirement 1 of POP-INN-02). Each step is protected by halving (ν = 1 down to 2⁻¹⁰, FIT-ML-07)
//! and then by Levenberg damping (FIT-ALG-04) (requirement 2). The test is the Newton decrement
//! Δ = g·δ ≤ ε_in with g = 𝒥ᵀR and δ = (𝒥ᵀ𝒥)⁻¹g (the convention of H37), and every converged solve
//! is polished by Newton steps on the observed Hessian (requirement 4). A start that fails is
//! followed by the start ladder (requirement 3).

use std::f64::consts::SQRT_2;

use crate::ErrorModel;
use crate::engine::{LAMBDA_FACTOR, LAMBDA_MAX, LAMBDA_START, MIN_STEP, damped};
use crate::linalg::{Mat, least_squares};

use super::dense::{cholesky, solve};
use super::structure::{Evaluation, SubjectModel};

/// ε_in: the default ε of FIT-ML-08, on the decrement in the convention of H37.
pub(crate) const EPS_IN: f64 = 1e-10;
/// The iteration limit of an inner solve (the default limit of `objective = ml`, FIT-ML-08).
const MAX_ITERATIONS: usize = 500;
/// Newton steps of the polish (FIT-ML-08).
const POLISH_STEPS: usize = 12;
/// The polish stops below this decrement (FIT-ML-08).
const POLISH_FLOOR: f64 = 1e-26;
/// The rounding allowed on h by the polish condition "h does not increase", in units of
/// `f64::EPSILON · max(Σ|terms of h|, 1)` (the terms ln σ², z² and (L⁻¹η)², whose sum h is: the
/// rounding of a sum follows the size of its terms, not of the sum, and h near 0 is a sum of
/// terms of a few units that cancel). OP-11, waits for Q-026. Measured (T-154): near the mode a
/// Newton step that lowers the decrement from about 1e-15 to about 1e-29 raised h by up to 8.1 of
/// these units (replicate 1 of the sealed design `accurate`; 6.3 on replicate 83 of `limits`) and
/// by up to 757 units of `f64::EPSILON · max(|h|, 1)` (h = 0.66 there). With that second scale
/// the polish stopped early (Δ = 3.8e-15 left on subject 16 of `limits` replicate 83 at one point
/// of a difference quotient), which made a jump of 2.8e-8 in the OFV, a gradient in ξ_a of +3.57
/// where it is −0.068, and the fit ended `no_decrease` (replicates 54, 83 and 97 of `limits`);
/// with this scale the three converge. Decision taken meanwhile (2026-11-08), reversible by Abdou:
/// an increase of h within this rounding does not count as an increase; Δ must still strictly
/// decrease.
const H_ROUNDING: f64 = 16.0;
/// The step of the central differences of the score, relative to the prior standard deviation √Ω_kk
/// of each random effect.
pub(crate) const POLISH_H: f64 = 1e-5;
/// The non-zero starts of the ladder, on each coordinate in turn (POP-INN-02 requirement 3).
const LADDER: [f64; 4] = [2.0, -2.0, 4.0, -4.0];

/// The data of one inner problem: everything but η is held.
pub(crate) struct InnerProblem<'a> {
    pub model: &'a SubjectModel<'a>,
    pub y: &'a [f64],
    pub error_model: ErrorModel,
    /// The moved error variables (FIT-ML-05).
    pub xi: &'a [f64],
    /// σ⁰ held at the population prediction (method `foce`); `None` for FOCEI.
    pub frozen: Option<&'a [f64]>,
    /// L⁻¹, with Ω = L·Lᵀ.
    pub l_inv: &'a Mat,
    /// The steps of the central differences of the polish, one per random effect.
    pub steps: &'a [f64],
    /// The decrement below which the scoring iteration stops and the polish starts ([`EPS_IN`]).
    pub eps: f64,
}

/// A point of the inner problem with its stacked system.
#[derive(Debug, Clone)]
pub(crate) struct Point {
    pub eta: Vec<f64>,
    pub ev: Evaluation,
    /// σ_j at this point (σ⁰_j when σ is held).
    pub sigma: Vec<f64>,
    /// h(η).
    pub h: f64,
    /// Σ of the absolute values of the terms of h (the scale of its rounding, [`H_ROUNDING`]).
    h_terms: f64,
    a: Mat,
    r: Vec<f64>,
    /// g = 𝒥ᵀR, half the negative gradient of h.
    g: Vec<f64>,
    /// The scoring step δ = (𝒥ᵀ𝒥)⁻¹g.
    delta: Vec<f64>,
    /// Δ = g·δ.
    pub decrement: f64,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The least-squares direction of A·δ ≈ r on unit-length columns, damped by λ when λ > 0.
fn direction(a: &Mat, r: &[f64], lambda: f64) -> Option<Vec<f64>> {
    let norms: Vec<f64> = (0..a.cols()).map(|j| a.column_norm(j)).collect();
    if norms.iter().any(|n| !(n.is_finite() && *n > 0.0)) {
        return None;
    }
    let mut scaled = a.clone();
    for (j, n) in norms.iter().enumerate() {
        for i in 0..a.rows() {
            scaled.set(i, j, a.at(i, j) / n);
        }
    }
    let step = if lambda > 0.0 {
        damped(&scaled, r, lambda)?
    } else {
        least_squares(&scaled, r)?.0
    };
    let step: Vec<f64> = step.iter().zip(&norms).map(|(s, n)| s / n).collect();
    step.iter().all(|x| x.is_finite()).then_some(step)
}

impl InnerProblem<'_> {
    fn q(&self) -> usize {
        self.l_inv.rows()
    }

    /// The point η, or `None` when the model or σ is not usable there (a rejected trial).
    pub fn point(&self, eta: Vec<f64>) -> Option<Point> {
        let ev = self.model.evaluate(&eta)?;
        let n = self.y.len();
        let q = self.q();
        if ev.pred.len() != n || eta.len() != q {
            return None;
        }
        let sigma: Vec<f64> = match self.frozen {
            Some(s) => s.to_vec(),
            None => ev
                .pred
                .iter()
                .map(|&f| self.error_model.sigma(self.xi, f))
                .collect(),
        };
        if sigma.len() != n || sigma.iter().any(|s| !(s.is_finite() && *s > 0.0)) {
            return None;
        }
        let both = self.frozen.is_none();
        let rows = if both { 2 * n + q } else { n + q };
        let mut a = Mat::zeros(rows, q);
        let mut r = vec![0.0; rows];
        let mut h = 0.0;
        // Σ of the absolute values of the terms of h, the scale of its rounding.
        let mut h_terms = 0.0;
        for (j, ((&y, &f), &s)) in self.y.iter().zip(&ev.pred).zip(&sigma).enumerate() {
            let z = (y - f) / s;
            if both {
                let log_var = libm::log(s * s);
                h += log_var;
                h_terms += log_var.abs();
            }
            h += z * z;
            h_terms += z * z;
            if let Some(slot) = r.get_mut(j) {
                *slot = z;
            }
            let dl = if both {
                self.error_model.dln_sigma_dyhat(self.xi, f, s)
            } else {
                0.0
            };
            for (k, col) in ev.g.iter().enumerate() {
                let gjk = col.get(j).copied().unwrap_or(f64::NAN);
                a.set(j, k, gjk / s);
                if both {
                    a.set(n + j, k, SQRT_2 * dl * gjk);
                }
            }
            if both {
                if let Some(slot) = r.get_mut(n + j) {
                    *slot = (z * z - 1.0) / SQRT_2;
                }
            }
        }
        let offset = if both { 2 * n } else { n };
        for i in 0..q {
            let mut li_eta = 0.0;
            for (k, e) in eta.iter().enumerate() {
                let lik = self.l_inv.at(i, k);
                a.set(offset + i, k, lik);
                li_eta += lik * e;
            }
            h += li_eta * li_eta;
            h_terms += li_eta * li_eta;
            if let Some(slot) = r.get_mut(offset + i) {
                *slot = -li_eta;
            }
        }
        if !h.is_finite() {
            return None;
        }
        let g: Vec<f64> = (0..q)
            .map(|k| {
                (0..rows)
                    .map(|i| a.at(i, k) * r.get(i).copied().unwrap_or(0.0))
                    .sum()
            })
            .collect();
        let delta = direction(&a, &r, 0.0)?;
        let decrement = dot(&g, &delta);
        if !decrement.is_finite() {
            return None;
        }
        Some(Point {
            eta,
            ev,
            sigma,
            h,
            h_terms,
            a,
            r,
            g,
            delta,
            decrement,
        })
    }

    fn moved(eta: &[f64], step: &[f64], nu: f64) -> Vec<f64> {
        eta.iter().zip(step).map(|(e, s)| e + nu * s).collect()
    }

    /// One protected scoring step: halving, then Levenberg damping. `None` when nothing lowers h.
    fn step(&self, current: &Point) -> Option<Point> {
        let mut nu = 1.0;
        while nu >= MIN_STEP {
            if let Some(trial) = self.point(Self::moved(&current.eta, &current.delta, nu)) {
                if trial.h < current.h {
                    return Some(trial);
                }
            }
            nu /= 2.0;
        }
        let mut lambda = LAMBDA_START;
        while lambda <= LAMBDA_MAX {
            if let Some(d) = direction(&current.a, &current.r, lambda) {
                if let Some(trial) = self.point(Self::moved(&current.eta, &d, 1.0)) {
                    if trial.h < current.h {
                        return Some(trial);
                    }
                }
            }
            lambda *= LAMBDA_FACTOR;
        }
        None
    }

    /// The scoring iteration from one start, then the polish; `None` when it does not converge.
    ///
    /// When the scoring stops above `eps` (its iteration limit, or no step that lowers h), the
    /// polish is tried from the last point, and the point is kept if the polish brings the decrement
    /// down to `eps` (decision taken meanwhile, 2026-11-08, reversible by Abdou). Measured (T-154):
    /// on replicate 44 of the sealed design `limits`, subject 2 at the starting values, the scoring
    /// creeps for 500 iterations and ends with a decrement between 1.3e-9 and 4.0e-6 from each of the
    /// nine starts of the ladder, while the Newton polish from the point of the first start reaches
    /// 9.6e-31 with h lower by 1.1e-8; without this the fit was refused with `inner_failed`.
    fn solve_from(&self, start: Vec<f64>) -> Option<Point> {
        let mut current = self.point(start)?;
        for _ in 0..MAX_ITERATIONS {
            if current.decrement <= self.eps {
                return Some(self.polish(current));
            }
            match self.step(&current) {
                Some(next) => current = next,
                None => break,
            }
        }
        let polished = self.polish(current);
        (polished.decrement <= self.eps).then_some(polished)
    }

    /// The conditional mode, from the warm start then down the ladder (POP-INN-02 requirement 3).
    pub fn solve(&self, warm: &[f64]) -> Option<Point> {
        let q = self.q();
        let zero = vec![0.0; q];
        let mut starts: Vec<Vec<f64>> = vec![warm.to_vec()];
        if warm != zero.as_slice() {
            starts.push(zero.clone());
        }
        for k in 0..q {
            for x in LADDER {
                let mut s = zero.clone();
                if let Some(slot) = s.get_mut(k) {
                    *slot = x;
                }
                starts.push(s);
            }
        }
        starts.into_iter().find_map(|s| self.solve_from(s))
    }

    /// The Newton polish of FIT-ML-08 with the condition added by POP-INN-02: a step is taken only if
    /// the observed Hessian is positive definite at the point, h does not increase (up to
    /// [`H_ROUNDING`]) and Δ strictly decreases; at most [`POLISH_STEPS`] steps, down to [`POLISH_FLOOR`].
    fn polish(&self, mut current: Point) -> Point {
        let mut taken = 0;
        while taken < POLISH_STEPS && current.decrement > POLISH_FLOOR {
            let Some(trial) = self.newton_point(&current) else {
                break;
            };
            let rounding = H_ROUNDING * f64::EPSILON * current.h_terms.max(1.0);
            if trial.h <= current.h + rounding && trial.decrement < current.decrement {
                current = trial;
                taken += 1;
            } else {
                break;
            }
        }
        current
    }

    /// The Newton point η − H⁻¹∇h with H the observed Hessian of h, by central differences of the
    /// analytic gradient −2g; `None` when H is not positive definite or a point is not usable.
    fn newton_point(&self, current: &Point) -> Option<Point> {
        let q = self.q();
        let mut hessian = Mat::zeros(q, q);
        for c in 0..q {
            let h = self.steps.get(c).copied()?;
            let mut up = current.eta.clone();
            let mut down = current.eta.clone();
            *up.get_mut(c)? += h;
            *down.get_mut(c)? -= h;
            let g_up = self.point(up)?.g;
            let g_down = self.point(down)?.g;
            for r in 0..q {
                let d = -2.0 * (g_up.get(r)? - g_down.get(r)?) / (2.0 * h);
                hessian.set(r, c, d);
            }
        }
        let mut sym = Mat::zeros(q, q);
        for a in 0..q {
            for b in 0..q {
                sym.set(a, b, 0.5 * (hessian.at(a, b) + hessian.at(b, a)));
            }
        }
        let factor = cholesky(&sym)?;
        // H·s = −∇h = 2g.
        let rhs: Vec<f64> = current.g.iter().map(|g| 2.0 * g).collect();
        let s = solve(&factor, &rhs);
        if s.iter().any(|x| !x.is_finite()) {
            return None;
        }
        self.point(Self::moved(&current.eta, &s, 1.0))
    }
}
