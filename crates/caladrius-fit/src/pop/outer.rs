//! The outer optimiser (POP-EST-03): BFGS on φ with the active set of FIT-BND-01, backtracking as
//! in FIT-ML-07 without Levenberg damping, a central-difference gradient of the OFV, and the stop
//! by the Newton decrement of a confirmation Hessian.
//!
//! Every evaluation re-solves the inner problems to the polish floor, warm-started from the η̂ of
//! the last accepted iterate (POP-INN-02, POP-COV-02 requirement 1). The OFV that is differenced is
//! the `no_2pi` sum: the constant N·ln 2π only adds rounding (POP-OBJ-05).
//!
//! **The constants of OP-11** below are working values chosen from measurements of this card; each
//! waits for Q-026, which sets them from the engine's own measurements.

use crate::linalg::Mat;

use super::dense;
use super::evaluation::{Evaluation, Failure, Settings};
use super::phi::{Layout, Part, at, put};
use super::{OfvConvention, Setup};

/// ε_out, the outer stop on the predicted decrease and on the confirmation decrement ½·gᵀR⁻¹g, in
/// OFV units, absolute (POP-EST-03). OP-11, waits for Q-026. Measured (T-154, the ignored test
/// `pop::measure::measure_the_exact_fits`): with 1e-8 the twelve exact fits (six cases, two starts)
/// end within 0.25 of their tolerance on every labelled value and within 1.9e-8 of the exact OFV
/// (the tolerance is 1e-7). The binding cases are those whose variance is 0 at the maximum: the fit
/// stops at ω̂² = 1.0e-9 (`fit_q1_boundary`, tolerance 5.4e-9) and 6.5e-11 (`fit_q2_diag_boundary`,
/// tolerance 1.0e-9), where the gradient of the OFV in the variance is 18.6 and 99.3 and the OFV
/// excess is that gradient times ω̂² (1.9e-8 and 6.5e-9, as measured). A larger value was not
/// tried. The lower limit is the noise of the OFV: before the polish fix of `inner::H_ROUNDING`
/// that noise was 4.9e-8 on the first replicate of the sealed design `accurate` (one subject of
/// 40), five times this value; after it the noise is the rounding of the OFV, 1.8e-13.
pub(crate) const EPS_OUT: f64 = 1e-8;

/// K_max, the limit of accepted outer iterations. OP-11, waits for Q-026. Measured (T-154): the
/// 12 exact fit cases take 5 to 40 iterations (40 on `fit_q2_block_boundary` from the far start,
/// 39 on `fit_q1_boundary`: a variance that collapses loses about 0.35 of ln ω per iteration);
/// the 200 replicates of the sealed design `accurate` take at most 19 and those of `limits` at
/// most 35 (replicate 191, where ω_cl collapses). 200 leaves a factor 5 over the largest.
pub(crate) const K_MAX: usize = 200;

/// The step of the central differences of the gradient (POP-EST-03, POP-COV-02 requirement 3): a
/// fraction of max(|φ_j|, 1) for θ and Ω, and of |ξ| for an error parameter (see
/// [`XI_STEP_FLOOR`]). OP-11, waits for Q-026. Measured (T-154, the ignored test
/// `pop::measure::measure_the_difference_steps`, at the end of the fit of the exact case
/// `fit_q1_boundary` and of the first replicate of the sealed design `accurate`): the noise of one
/// OFV is its rounding, 9.8e-15 and 1.8e-13 (a warm and a cold inner start give the same OFV once
/// the polish runs to its floor, see `inner::H_ROUNDING`); the fraction that balances truncation
/// h²·|∂³OFV|/6 and noise/h is 5.6e-4 to 7.7e-3 on `fit_q1_boundary` and 3.1e-6 to 8.4e-5 on the
/// sealed replicate for θ and Ω, 5.1e-6 (relative to ξ = 1.11) and 9.7e-6 to 2.0e-4 (relative to
/// ξ = 1.05e-2 and 8.5e-5) for ξ. What matters for the stop is the error e_j of a component (its
/// truncation plus its noise) weighed by the curvature, ½·e_j²/R_jj: at 1e-5 it is at most 6e-14
/// over the coordinates of the two cases (ln ω of `fit_q1_boundary`: e = 9.5e-11, R = 7.7e-8; ξ =
/// 8.5e-5 of the sealed replicate: e = 2.7e-4, R = 1.8e7), far below [`EPS_OUT`].
pub(crate) const H_GRADIENT: f64 = 1e-5;

/// The step of the second differences of the confirmation Hessian R and of the covariance, as a
/// fraction of max(|φ_j|, 1), for θ and Ω. OP-11, waits for Q-026. Measured (T-154, same test and
/// cases): the fraction that balances truncation k²·|∂⁴OFV|/12 and noise 4·noise/k² is 3.1e-3 to
/// 4.6e-3 on `fit_q1_boundary` and 9.6e-5 to 2.0e-3 on the sealed replicate. At 1e-3 the relative
/// error of the diagonal of R stays below 1e-6 on every θ and Ω coordinate of the sealed replicate
/// (the largest, ln ω₂: truncation 5.7e-5 on R = 94), and on `fit_q1_boundary` the noise on the curvature
/// in ln ω (R = 7.7e-8) is 3.7e-10 at 1e-3 but 3.7e-6, larger than R itself, at 1e-5.
pub(crate) const H_HESSIAN: f64 = 1e-3;

/// The same for an error parameter, as a fraction of |ξ| (decision taken meanwhile, 2026-11-08,
/// reversible by Abdou: POP-COV-02 asks for a fraction of max(|φ_j|, 1), but ξ lives on the
/// variance scale and is fitted near 0, where no fraction of max(|ξ|, 1) satisfies both conditions
/// of POP-EST-03 for θ and ξ at once). OP-11, waits for Q-026. Measured (T-154, same test): the
/// balancing fraction is 1.4e-4 relative to ξ = 1.11 on `fit_q1_boundary`, 2.4e-4 and 3.2e-3
/// relative to ξ = 1.05e-2 and 8.5e-5 on the sealed replicate; at 1e-4 the relative error of R_ξξ
/// is at most 7e-4 (ξ = 8.5e-5: noise 1.2e4 on R = 1.8e7). With steps of 1e-5 of max(|ξ|, 1) on
/// the gradient and the Hessian, the gradient in ξ = 8.5e-5 came out as −0.68 where smaller steps
/// gave +1.42 to +1.45, and the fit of that replicate ended in `no_decrease` after 14 iterations;
/// with the relative steps it converges in 15.
pub(crate) const H_HESSIAN_XI: f64 = 1e-4;

/// The smallest scale of a step on ξ, for an error parameter at or very near its bound 0, where a
/// step relative to |ξ| would vanish. OP-11, waits for Q-026. Decision taken meanwhile
/// (2026-11-08), reversible by Abdou: 1e-6, the floor of the generated bounds of FIT-BND-03. Where
/// it acts (T-154): the additive ξ_a held at 0 on 88 of the 200 replicates of `accurate` and 145
/// of `limits`; there the one-sided gradient step is 1e-5 · 1e-6 = 1e-11, the gradient is of
/// order 1e3 (+1023 on replicate 3 of `accurate`, with R_AA ≈ 3.9e6), the noise term 1.8e-13/1e-11
/// is about 0.02 and the truncation term R·h about 4e-5, both far below the gradient. A value
/// near the floor that is not on it (ξ < 1e-6) was not met.
const XI_STEP_FLOOR: f64 = 1e-6;

/// Halvings of the backtracking (ν = 1 down to 2⁻¹⁰, FIT-ML-07).
const MAX_HALVINGS: i32 = 10;

/// How a fit ended (POP-EST-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Converged,
    MaxIterations,
    NoDecrease,
    Singular,
    NonFinite,
    AtBound,
    InnerFailed,
}

impl Status {
    pub fn id(self) -> &'static str {
        match self {
            Status::Converged => "converged",
            Status::MaxIterations => "max_iterations",
            Status::NoDecrease => "no_decrease",
            Status::Singular => "singular",
            Status::NonFinite => "non_finite",
            Status::AtBound => "at_bound",
            Status::InnerFailed => "inner_failed",
        }
    }

    fn of_failure(f: &Failure) -> Self {
        if f.is_inner() {
            Status::InnerFailed
        } else {
            Status::NonFinite
        }
    }
}

/// One row of the trace: one per accepted iteration, iteration 0 included (POP-EST-03).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TraceRow {
    pub iteration: usize,
    /// OFV in the convention of the answer.
    pub ofv: f64,
    /// φ in the user's units ([`Layout::user_values`]).
    pub values: Vec<f64>,
    /// The step length ν of the accepted step (none at iteration 0).
    pub nu: Option<f64>,
    pub halvings: usize,
    /// The predicted decrease of the step taken, or the confirmation decrement when R was computed.
    pub decrement: Option<f64>,
    /// On the last row.
    pub status: Option<Status>,
}

/// The gradient of the OFV and of each OFV_i.
#[derive(Debug, Clone)]
pub(crate) struct Gradient {
    pub total: Vec<f64>,
    /// g_i, one row per subject in dataset order.
    pub per_subject: Vec<Vec<f64>>,
}

/// The end of a fit.
#[derive(Debug, Clone)]
pub(crate) struct Outcome {
    pub status: Status,
    pub phi: Vec<f64>,
    pub evaluation: Evaluation,
    /// The gradient at `phi`, when it could be computed.
    pub gradient: Option<Gradient>,
    /// R at `phi` on the coordinates listed, when the confirmation computed it there.
    pub hessian: Option<(Vec<usize>, Mat)>,
    pub trace: Vec<TraceRow>,
    pub iterations: usize,
}

/// The outer problem: the objective of `setup` as a function of φ of `layout`.
pub(crate) struct Problem<'a> {
    pub setup: &'a Setup,
    pub layout: &'a Layout,
}

/// −H·g restricted to `set` (zero elsewhere), and ½·gᵀH·g over `set`.
fn direction(h: &Mat, g: &[f64], set: &[usize]) -> (Vec<f64>, f64) {
    let mut d = vec![0.0; g.len()];
    let mut pred = 0.0;
    for &a in set {
        let hg: f64 = set.iter().map(|&b| h.at(a, b) * at(g, b)).sum();
        put(&mut d, a, -hg);
        pred += at(g, a) * hg;
    }
    (d, 0.5 * pred)
}

/// The inverse of a symmetric positive-definite matrix from its Cholesky factor; `None` when it is
/// not positive definite.
pub(crate) fn spd_inverse(a: &Mat) -> Option<Mat> {
    let n = a.rows();
    let l = dense::cholesky(a)?;
    let mut inv = Mat::zeros(n, n);
    for j in 0..n {
        let mut e = vec![0.0; n];
        put(&mut e, j, 1.0);
        let x = dense::solve(&l, &e);
        for (i, v) in x.into_iter().enumerate() {
            if !v.is_finite() {
                return None;
            }
            inv.set(i, j, v);
        }
    }
    Some(inv)
}

/// A step of the backtracking: the accepted trial, or why none was.
enum Search {
    Accepted {
        phi: Vec<f64>,
        evaluation: Evaluation,
        nu: f64,
        halvings: usize,
    },
    Failed {
        all_inner: bool,
        all_non_finite: bool,
    },
}

impl Problem<'_> {
    fn q(&self) -> usize {
        self.setup.random_effects.len()
    }

    fn settings(&self) -> Settings<'static> {
        Settings {
            eps: super::inner::EPS_IN,
            keep: false,
            fail_inner: &|_| false,
        }
    }

    /// The objective at φ, warm-started.
    pub fn eval(&self, phi: &[f64], warm: &[Vec<f64>]) -> Result<Evaluation, Failure> {
        self.setup
            .evaluate_params(&self.layout.params(phi), warm, self.settings())
    }

    /// OFV in the convention of the answer, from the `no_2pi` sum.
    pub fn ofv(&self, core: f64) -> f64 {
        match self.setup.convention {
            OfvConvention::Full => {
                core + self.setup.n_observations() as f64 * libm::log(std::f64::consts::TAU)
            }
            OfvConvention::No2Pi => core,
        }
    }

    /// The difference step on coordinate j: `fraction · max(|φ_j|, 1)` for θ and Ω, and
    /// `fraction · max(|ξ|, XI_STEP_FLOOR)` for an error parameter (see [`H_GRADIENT`]).
    fn step_of(&self, phi: &[f64], j: usize, fraction: f64) -> f64 {
        let x = at(phi, j).abs();
        match self.layout.coords.get(j).map(|c| c.part()) {
            Some(Part::Xi) => fraction * x.max(XI_STEP_FLOOR),
            _ => fraction * x.max(1.0),
        }
    }

    fn hessian_fraction(&self, j: usize) -> f64 {
        match self.layout.coords.get(j).map(|c| c.part()) {
            Some(Part::Xi) => H_HESSIAN_XI,
            _ => H_HESSIAN,
        }
    }

    fn moved(phi: &[f64], j: usize, by: f64) -> Vec<f64> {
        let mut x = phi.to_vec();
        put(&mut x, j, at(phi, j) + by);
        x
    }

    /// The gradient by central differences of the OFV and of each OFV_i (POP-COV-02), with a
    /// one-sided three-point stencil (−3f₀ + 4f₁ − f₂)/(2h) where a bound leaves no room on one side
    /// or where the evaluation on one side fails.
    pub fn gradient(&self, phi: &[f64], base: &Evaluation) -> Result<Gradient, Failure> {
        let p = phi.len();
        let m = base.subjects.len();
        let warm = base.warm(self.q());
        let mut total = vec![0.0; p];
        let mut per_subject = vec![vec![0.0; p]; m];
        for j in 0..p {
            let h = self.step_of(phi, j, H_GRADIENT);
            let x = at(phi, j);
            let lo = at(&self.layout.lower, j);
            let hi = at(&self.layout.upper, j);
            let room_down = x - h >= lo;
            let room_up = x + h <= hi;
            let up = if room_up {
                Some(self.eval(&Self::moved(phi, j, h), &warm))
            } else {
                None
            };
            let down = if room_down {
                Some(self.eval(&Self::moved(phi, j, -h), &warm))
            } else {
                None
            };
            let quotient: Vec<f64> = match (up, down) {
                (Some(Ok(u)), Some(Ok(d))) => u
                    .subjects
                    .iter()
                    .zip(&d.subjects)
                    .map(|(a, b)| (a.core - b.core) / (2.0 * h))
                    .collect(),
                (Some(Ok(u)), other) => {
                    let failure = match other {
                        Some(Err(e)) => Some(e),
                        _ => None,
                    };
                    let far = if x + 2.0 * h <= hi {
                        self.eval(&Self::moved(phi, j, 2.0 * h), &warm)
                    } else {
                        Err(failure.unwrap_or(Failure::Covariance(String::new())))
                    }?;
                    one_sided(base, &u, &far, h)
                }
                (other, Some(Ok(d))) => {
                    let failure = match other {
                        Some(Err(e)) => Some(e),
                        _ => None,
                    };
                    let far = if x - 2.0 * h >= lo {
                        self.eval(&Self::moved(phi, j, -2.0 * h), &warm)
                    } else {
                        Err(failure.unwrap_or(Failure::Covariance(String::new())))
                    }?;
                    one_sided(base, &d, &far, -h)
                }
                (Some(Err(e)), _) | (_, Some(Err(e))) => return Err(e),
                (None, None) => return Err(Failure::Covariance(String::new())),
            };
            let mut sum = 0.0;
            for (i, d) in quotient.iter().enumerate() {
                if let Some(row) = per_subject.get_mut(i) {
                    put(row, j, *d);
                }
                sum += d;
            }
            if !sum.is_finite() {
                return Err(Failure::Covariance(String::new()));
            }
            put(&mut total, j, sum);
        }
        Ok(Gradient { total, per_subject })
    }

    /// R, the Hessian of the OFV over the coordinates `set`, by second differences of the OFV
    /// (POP-COV-02): (f₊ − 2f₀ + f₋)/k² on the diagonal and the four-point formula off it, with the
    /// steps of [`H_HESSIAN`] and [`H_HESSIAN_XI`]. Where a bound is closer than the step, the
    /// centre is moved inside by the step on that coordinate (decision taken meanwhile, 2026-11-08,
    /// reversible by Abdou).
    pub fn hessian(&self, phi: &[f64], set: &[usize], warm: &[Vec<f64>]) -> Result<Mat, Failure> {
        let n = set.len();
        let mut centre = phi.to_vec();
        let steps: Vec<f64> = set
            .iter()
            .map(|&j| self.step_of(phi, j, self.hessian_fraction(j)))
            .collect();
        for (&j, &k) in set.iter().zip(&steps) {
            let lo = at(&self.layout.lower, j);
            let hi = at(&self.layout.upper, j);
            let x = at(phi, j);
            if hi - lo > 2.0 * k {
                put(&mut centre, j, x.max(lo + k).min(hi - k));
            }
        }
        let f = |x: &[f64]| -> Result<f64, Failure> { Ok(self.eval(x, warm)?.core) };
        let f0 = f(&centre)?;
        let mut r = Mat::zeros(n, n);
        let mut plus = vec![0.0; n];
        let mut minus = vec![0.0; n];
        for (a, (&j, &k)) in set.iter().zip(&steps).enumerate() {
            let fp = f(&Self::moved(&centre, j, k))?;
            let fm = f(&Self::moved(&centre, j, -k))?;
            put(&mut plus, a, fp);
            put(&mut minus, a, fm);
            r.set(a, a, (fp - 2.0 * f0 + fm) / (k * k));
        }
        for (a, (&ja, &ka)) in set.iter().zip(&steps).enumerate() {
            for (b, (&jb, &kb)) in set.iter().zip(&steps).enumerate().take(a) {
                let corner = |sa: f64, sb: f64| -> Result<f64, Failure> {
                    let x = Self::moved(&Self::moved(&centre, ja, sa * ka), jb, sb * kb);
                    f(&x)
                };
                let fpp = corner(1.0, 1.0)?;
                let fpm = corner(1.0, -1.0)?;
                let fmp = corner(-1.0, 1.0)?;
                let fmm = corner(-1.0, -1.0)?;
                let d = (fpp - fpm - fmp + fmm) / (4.0 * ka * kb);
                r.set(a, b, d);
                r.set(b, a, d);
            }
        }
        for a in 0..n {
            for b in 0..n {
                if !r.at(a, b).is_finite() {
                    return Err(Failure::Covariance(String::new()));
                }
            }
        }
        Ok(r)
    }

    /// The backtracking of FIT-ML-07 along `d` from `phi`: ν = 1 halved down to 2⁻¹⁰ while the
    /// trial does not lower the OFV. A trial outside the bounds is projected onto them; a trial
    /// whose inner problem fails or whose OFV is not finite is rejected.
    fn search(&self, phi: &[f64], current: &Evaluation, d: &[f64]) -> Search {
        let warm = current.warm(self.q());
        let mut all_inner = true;
        let mut all_non_finite = true;
        let mut tried = false;
        for halvings in 0..=MAX_HALVINGS {
            let nu = libm::ldexp(1.0, -halvings);
            let mut trial: Vec<f64> = phi.iter().zip(d).map(|(x, s)| x + nu * s).collect();
            self.layout.project(&mut trial);
            if trial.iter().zip(phi).all(|(a, b)| a == b) {
                break;
            }
            tried = true;
            match self.eval(&trial, &warm) {
                Ok(e) if e.core < current.core => {
                    return Search::Accepted {
                        phi: trial,
                        evaluation: e,
                        nu,
                        halvings: usize::try_from(halvings).unwrap_or(0),
                    };
                }
                Ok(_) => {
                    all_inner = false;
                    all_non_finite = false;
                }
                Err(f) => {
                    if f.is_inner() {
                        all_non_finite = false;
                    } else {
                        all_inner = false;
                    }
                }
            }
        }
        Search::Failed {
            all_inner: tried && all_inner,
            all_non_finite: tried && all_non_finite,
        }
    }

    /// The coordinates held by the active set at `phi`: on a bound with the gradient pointing out.
    fn free_set(&self, phi: &[f64], g: &[f64]) -> Vec<usize> {
        (0..phi.len())
            .filter(|&j| {
                let x = at(phi, j);
                let gj = at(g, j);
                let held_low = x == at(&self.layout.lower, j) && gj > 0.0;
                let held_high = x == at(&self.layout.upper, j) && gj < 0.0;
                !(held_low || held_high)
            })
            .collect()
    }

    fn row(
        &self,
        iteration: usize,
        evaluation: &Evaluation,
        phi: &[f64],
        nu: Option<f64>,
        halvings: usize,
        decrement: Option<f64>,
    ) -> TraceRow {
        TraceRow {
            iteration,
            ofv: self.ofv(evaluation.core),
            values: self.layout.user_values(&self.layout.params(phi)),
            nu,
            halvings,
            decrement,
            status: None,
        }
    }

    /// The fit from `phi0`, whose evaluation `start` succeeded.
    pub fn minimise(&self, phi0: Vec<f64>, start: Evaluation) -> Outcome {
        let p = phi0.len();
        let mut phi = phi0;
        let mut current = start;
        let mut trace = vec![self.row(0, &current, &phi, None, 0, None)];
        let finish = |status: Status,
                      phi: Vec<f64>,
                      evaluation: Evaluation,
                      gradient: Option<Gradient>,
                      hessian: Option<(Vec<usize>, Mat)>,
                      mut trace: Vec<TraceRow>,
                      iterations: usize| {
            if let Some(last) = trace.last_mut() {
                last.status = Some(status);
            }
            Outcome {
                status,
                phi,
                evaluation,
                gradient,
                hessian,
                trace,
                iterations,
            }
        };
        let mut grad = match self.gradient(&phi, &current) {
            Ok(g) => g,
            Err(f) => {
                return finish(Status::of_failure(&f), phi, current, None, None, trace, 0);
            }
        };
        // H₀ = R₀⁻¹ when R₀ is positive definite, else the inverse of |diag R₀|.
        let mut h = Mat::zeros(p, p);
        for j in 0..p {
            h.set(j, j, 1.0);
        }
        let set0 = self.free_set(&phi, &grad.total);
        if let Ok(r0) = self.hessian(&phi, &set0, &current.warm(self.q())) {
            install(&mut h, &set0, &r0);
        }
        let mut fresh = true;
        // Whether the search from this point already fell back to the scaled steepest descent.
        let mut scaled = false;
        let mut last_singular: Option<Vec<f64>> = None;
        let mut iterations = 0;
        loop {
            if iterations >= K_MAX {
                return finish(
                    Status::MaxIterations,
                    phi,
                    current,
                    Some(grad),
                    None,
                    trace,
                    iterations,
                );
            }
            let set = self.free_set(&phi, &grad.total);
            let (mut d, mut decrement) = direction(&h, &grad.total, &set);
            let mut confirmed: Option<(Vec<usize>, Mat)> = None;
            let mut steepest = false;
            if decrement <= EPS_OUT || decrement.is_nan() {
                // The candidate: confirm by the Hessian.
                let r = match self.hessian(&phi, &set, &current.warm(self.q())) {
                    Ok(r) => r,
                    Err(f) => {
                        return finish(
                            Status::of_failure(&f),
                            phi,
                            current,
                            Some(grad),
                            None,
                            trace,
                            iterations,
                        );
                    }
                };
                match spd_inverse(&r) {
                    Some(r_inv) => {
                        let g_local: Vec<f64> = set.iter().map(|&j| at(&grad.total, j)).collect();
                        let (dn, delta) = direction(&r_inv, &g_local, &set_local(&set));
                        let dn = scatter(&dn, &set, p);
                        if delta <= EPS_OUT {
                            if let Some(last) = trace.last_mut() {
                                last.decrement = Some(delta);
                            }
                            let on_bound = (0..p).any(|j| self.layout.on_bound(&phi, j));
                            let status = if on_bound {
                                Status::AtBound
                            } else {
                                Status::Converged
                            };
                            return finish(
                                status,
                                phi,
                                current,
                                Some(grad),
                                Some((set, r)),
                                trace,
                                iterations,
                            );
                        }
                        install_inverse(&mut h, &set, &r_inv);
                        d = dn;
                        decrement = delta;
                    }
                    None => {
                        let close = last_singular.as_ref().is_some_and(|prev| {
                            set.iter().all(|&j| {
                                let k = self.step_of(&phi, j, self.hessian_fraction(j));
                                (at(prev, j) - at(&phi, j)).abs() <= k
                            })
                        });
                        if close {
                            return finish(
                                Status::Singular,
                                phi,
                                current,
                                Some(grad),
                                None,
                                trace,
                                iterations,
                            );
                        }
                        last_singular = Some(phi.clone());
                        d = vec![0.0; p];
                        for (a, &j) in set.iter().enumerate() {
                            let rjj = r.at(a, a).abs();
                            let scale = if rjj.is_finite() && rjj > 0.0 {
                                1.0 / rjj
                            } else {
                                1.0
                            };
                            put(&mut d, j, -scale * at(&grad.total, j));
                        }
                        steepest = true;
                    }
                }
                confirmed = Some((set.clone(), r));
                fresh = true;
            }
            let (new_phi, new_eval, nu, halvings) = match self.search(&phi, &current, &d) {
                Search::Accepted {
                    phi,
                    evaluation,
                    nu,
                    halvings,
                } => (phi, evaluation, nu, halvings),
                Search::Failed {
                    all_inner,
                    all_non_finite,
                } => {
                    if !fresh {
                        // A stale BFGS matrix: rebuild it from R at this point and try again.
                        if let Ok(r) = self.hessian(&phi, &set, &current.warm(self.q())) {
                            match spd_inverse(&r) {
                                Some(r_inv) => install_inverse(&mut h, &set, &r_inv),
                                None => install_diagonal(&mut h, &set, &r),
                            }
                        }
                        fresh = true;
                        continue;
                    }
                    if !steepest && !scaled {
                        // A fresh matrix R⁻¹ whose step does not lower the OFV within the halvings:
                        // the steepest descent scaled by 1/|diag R|, the fallback POP-EST-03 gives
                        // for an R that is not positive definite (decision taken meanwhile,
                        // 2026-11-08, reversible by Abdou). Measured (T-154): at the two-stage start
                        // of replicates 46 and 97 of the sealed design `accurate`, R is positive
                        // definite but its Newton step moves ln ω by up to 13 and ξ_A by 2.0 from
                        // 6.25e-4, and at ν = 2⁻¹⁰ the OFV is still 1.8 above the start.
                        if let Ok(r) = self.hessian(&phi, &set, &current.warm(self.q())) {
                            install_diagonal(&mut h, &set, &r);
                            scaled = true;
                            continue;
                        }
                    }
                    let status = if steepest {
                        Status::Singular
                    } else if all_inner {
                        Status::InnerFailed
                    } else if all_non_finite {
                        Status::NonFinite
                    } else {
                        Status::NoDecrease
                    };
                    return finish(
                        status,
                        phi,
                        current,
                        Some(grad),
                        confirmed,
                        trace,
                        iterations,
                    );
                }
            };
            let new_grad = match self.gradient(&new_phi, &new_eval) {
                Ok(g) => g,
                Err(f) => {
                    iterations += 1;
                    trace.push(self.row(
                        iterations,
                        &new_eval,
                        &new_phi,
                        Some(nu),
                        halvings,
                        Some(decrement),
                    ));
                    return finish(
                        Status::of_failure(&f),
                        new_phi,
                        new_eval,
                        None,
                        None,
                        trace,
                        iterations,
                    );
                }
            };
            // The pair (s, y) on the free set of the step only: a coordinate held on its bound does
            // not move, and the change of its gradient (1e3 on the additive ξ held at 0 of replicate
            // 3 of the sealed design `accurate`) would otherwise enter yᵀHy and inflate H along s.
            let s: Vec<f64> = new_phi.iter().zip(&phi).map(|(a, b)| a - b).collect();
            let y: Vec<f64> = (0..p)
                .map(|j| {
                    if set.contains(&j) {
                        at(&new_grad.total, j) - at(&grad.total, j)
                    } else {
                        0.0
                    }
                })
                .collect();
            bfgs_update(&mut h, &s, &y);
            iterations += 1;
            trace.push(self.row(
                iterations,
                &new_eval,
                &new_phi,
                Some(nu),
                halvings,
                Some(decrement),
            ));
            phi = new_phi;
            current = new_eval;
            grad = new_grad;
            fresh = false;
            scaled = false;
        }
    }
}

/// The indices 0..n of a set of n coordinates (a matrix restricted to the set is indexed locally).
fn set_local(set: &[usize]) -> Vec<usize> {
    (0..set.len()).collect()
}

/// A vector over the set, carried to the full length (zero elsewhere); the caller gathers the
/// gradient on the set first and calls [`direction`] on the local indices.
fn scatter(local: &[f64], set: &[usize], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; p];
    for (a, &j) in set.iter().enumerate() {
        put(&mut out, j, at(local, a));
    }
    out
}

/// H₀ from R₀ on the set: R₀⁻¹ when positive definite, else the inverse of |diag R₀|.
fn install(h: &mut Mat, set: &[usize], r: &Mat) {
    match spd_inverse(r) {
        Some(inv) => install_inverse(h, set, &inv),
        None => install_diagonal(h, set, r),
    }
}

/// H_FF = R⁻¹ on the set, the cross terms with the other coordinates set to 0.
fn install_inverse(h: &mut Mat, set: &[usize], r_inv: &Mat) {
    let p = h.rows();
    for &a in set {
        for b in 0..p {
            h.set(a, b, 0.0);
            h.set(b, a, 0.0);
        }
    }
    for (x, &a) in set.iter().enumerate() {
        for (y, &b) in set.iter().enumerate() {
            h.set(a, b, r_inv.at(x, y));
        }
    }
}

fn install_diagonal(h: &mut Mat, set: &[usize], r: &Mat) {
    let p = h.rows();
    for &a in set {
        for b in 0..p {
            h.set(a, b, 0.0);
            h.set(b, a, 0.0);
        }
    }
    for (x, &a) in set.iter().enumerate() {
        let d = r.at(x, x).abs();
        h.set(
            a,
            a,
            if d.is_finite() && d > 0.0 {
                1.0 / d
            } else {
                1.0
            },
        );
    }
}

/// The BFGS update of the inverse Hessian, H ← (I − ρ·s·yᵀ)·H·(I − ρ·y·sᵀ) + ρ·s·sᵀ with
/// ρ = 1/(yᵀs), made only when yᵀs > 0 (which keeps H positive definite).
fn bfgs_update(h: &mut Mat, s: &[f64], y: &[f64]) {
    let p = s.len();
    let ys: f64 = s.iter().zip(y).map(|(a, b)| a * b).sum();
    if !(ys > 0.0 && ys.is_finite()) {
        return;
    }
    let rho = 1.0 / ys;
    let hy: Vec<f64> = (0..p)
        .map(|i| (0..p).map(|k| h.at(i, k) * at(y, k)).sum())
        .collect();
    let yhy: f64 = y.iter().zip(&hy).map(|(a, b)| a * b).sum();
    let mut next = h.clone();
    for i in 0..p {
        for j in 0..p {
            let v = h.at(i, j) - rho * (at(s, i) * at(&hy, j) + at(&hy, i) * at(s, j))
                + (rho * rho * yhy + rho) * at(s, i) * at(s, j);
            next.set(i, j, v);
        }
    }
    if (0..p).all(|i| (0..p).all(|j| next.at(i, j).is_finite())) {
        *h = next;
    }
}

/// (−3f₀ + 4f₁ − f₂)/(2h) per subject, with f₁ at φ + h and f₂ at φ + 2h (h < 0 for a backward
/// stencil).
fn one_sided(base: &Evaluation, near: &Evaluation, far: &Evaluation, h: f64) -> Vec<f64> {
    base.subjects
        .iter()
        .zip(&near.subjects)
        .zip(&far.subjects)
        .map(|((f0, f1), f2)| (-3.0 * f0.core + 4.0 * f1.core - f2.core) / (2.0 * h))
        .collect()
}
