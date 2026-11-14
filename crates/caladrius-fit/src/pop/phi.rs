//! The outer vector φ and its transforms (POP-EST-01, POP-EST-02).
//!
//! φ holds, in this order, the free coordinates of:
//!
//! 1. the typical values: ln θ_k for a parameter that carries a random effect, θ_k itself otherwise;
//! 2. Ω in log-Cholesky form, Ω = L·Lᵀ with L lower triangular: the logarithms λ_k = ln L_kk of the
//!    diagonal (for a diagonal Ω, λ_k = ln ω_k, the logarithm of the standard deviation), then, for a
//!    block, the entries of L below the diagonal, row by row (the order of `pdLogChol`, S-48);
//! 3. the moved error variables of FIT-ML-05 (A = a², B = b² for `combined`; a, b themselves for
//!    `combined_sd`), with the lower bound 0.
//!
//! A fixed parameter is not a coordinate: its value stays in [`Layout::base`]. The bounds of
//! FIT-BND-01 and FIT-BND-03 are carried to the scale of φ: a typical value with a random effect has
//! its bounds on the log scale, and a lower bound at or below 0 there is the open domain.

use std::collections::BTreeMap;

use crate::ErrorModel;
use crate::linalg::Mat;

use super::dense;

/// A value of a slice, NaN outside it (never panics).
pub(crate) fn at(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(f64::NAN)
}

/// Writes a value into a slice; a write outside it is ignored.
pub(crate) fn put(v: &mut [f64], i: usize, x: f64) {
    if let Some(slot) = v.get_mut(i) {
        *slot = x;
    }
}

/// The parameters of the objective, every part in full (fixed and free).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Params {
    /// Every typical value by name.
    pub theta: BTreeMap<String, f64>,
    /// The lower Cholesky factor of Ω (positive diagonal).
    pub l: Mat,
    /// The moved error variables (FIT-ML-05).
    pub xi: Vec<f64>,
}

impl Params {
    /// Ω = L·Lᵀ.
    pub fn omega(&self) -> Mat {
        let q = self.l.rows();
        let mut omega = Mat::zeros(q, q);
        for i in 0..q {
            for j in 0..=i {
                let s: f64 = (0..=j).map(|k| self.l.at(i, k) * self.l.at(j, k)).sum();
                omega.set(i, j, s);
                omega.set(j, i, s);
            }
        }
        omega
    }

    /// L⁻¹, the block of prior rows of the inner problem.
    pub fn l_inv(&self) -> Mat {
        dense::lower_inverse(&self.l)
    }
}

/// Which block of φ a coordinate belongs to (the step sizes of the differences depend on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Theta,
    Omega,
    Xi,
}

/// One free coordinate of φ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Coord {
    /// A typical value, on the log scale when it carries a random effect.
    Theta { name: String, log: bool },
    /// λ_k = ln L_kk.
    OmegaLog(usize),
    /// L_ij, i > j (block only).
    OmegaOff(usize, usize),
    /// The moved error variable k.
    Xi(usize),
}

impl Coord {
    pub fn part(&self) -> Part {
        match self {
            Coord::Theta { .. } => Part::Theta,
            Coord::OmegaLog(_) | Coord::OmegaOff(..) => Part::Omega,
            Coord::Xi(_) => Part::Xi,
        }
    }
}

/// The free coordinates, their bounds on the scale of φ, and the values of everything held.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub coords: Vec<Coord>,
    pub lower: Vec<f64>,
    pub upper: Vec<f64>,
    /// The start: the free parts are overwritten by φ, the fixed parts are kept.
    pub base: Params,
    pub error_model: ErrorModel,
    pub block: bool,
}

/// What is held and how each typical value is bounded, from the request.
pub(crate) struct Holds<'a> {
    /// Typical values held at their value (they are in `base.theta` already).
    pub fixed_theta: &'a [String],
    /// The random-effect parameters (catalogue models; a free label of the test model is not a
    /// typical value).
    pub with_random_effect: &'a [String],
    /// Bounds on the natural scale, by typical value: (lower, upper), ±∞ when there is none.
    pub bounds: &'a BTreeMap<String, (f64, f64)>,
    /// Diagonal Ω: which ω_k are held. Block: all true or all false.
    pub fixed_omega: &'a [bool],
    /// Which moved error variables are held.
    pub fixed_xi: &'a [bool],
}

impl Layout {
    pub fn new(base: Params, error_model: ErrorModel, block: bool, holds: &Holds<'_>) -> Self {
        let mut coords = Vec::new();
        let mut lower = Vec::new();
        let mut upper = Vec::new();
        for name in base.theta.keys() {
            if holds.fixed_theta.contains(name) {
                continue;
            }
            let log = holds.with_random_effect.contains(name);
            let (lo, hi) = holds
                .bounds
                .get(name)
                .copied()
                .unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
            let (lo, hi) = if log {
                let lo = if lo > 0.0 {
                    libm::log(lo)
                } else {
                    f64::NEG_INFINITY
                };
                let hi = if hi.is_finite() && hi > 0.0 {
                    libm::log(hi)
                } else if hi <= 0.0 {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
                (lo, hi)
            } else {
                (lo, hi)
            };
            coords.push(Coord::Theta {
                name: name.clone(),
                log,
            });
            lower.push(lo);
            upper.push(hi);
        }
        let q = base.l.rows();
        for k in 0..q {
            if !holds.fixed_omega.get(k).copied().unwrap_or(false) {
                coords.push(Coord::OmegaLog(k));
                lower.push(f64::NEG_INFINITY);
                upper.push(f64::INFINITY);
            }
        }
        if block && !holds.fixed_omega.iter().any(|&f| f) {
            for i in 0..q {
                for j in 0..i {
                    coords.push(Coord::OmegaOff(i, j));
                    lower.push(f64::NEG_INFINITY);
                    upper.push(f64::INFINITY);
                }
            }
        }
        for k in 0..base.xi.len() {
            if !holds.fixed_xi.get(k).copied().unwrap_or(false) {
                coords.push(Coord::Xi(k));
                lower.push(0.0);
                upper.push(f64::INFINITY);
            }
        }
        Self {
            coords,
            lower,
            upper,
            base,
            error_model,
            block,
        }
    }

    pub fn len(&self) -> usize {
        self.coords.len()
    }

    /// φ at the parameters `p`.
    pub fn phi(&self, p: &Params) -> Vec<f64> {
        self.coords
            .iter()
            .map(|c| match c {
                Coord::Theta { name, log } => {
                    let x = p.theta.get(name).copied().unwrap_or(f64::NAN);
                    if *log { libm::log(x) } else { x }
                }
                Coord::OmegaLog(k) => libm::log(p.l.at(*k, *k)),
                Coord::OmegaOff(i, j) => p.l.at(*i, *j),
                Coord::Xi(k) => at(&p.xi, *k),
            })
            .collect()
    }

    /// The parameters at φ.
    pub fn params(&self, phi: &[f64]) -> Params {
        let mut p = self.base.clone();
        for (c, &x) in self.coords.iter().zip(phi) {
            match c {
                Coord::Theta { name, log } => {
                    let v = if *log { libm::exp(x) } else { x };
                    if let Some(slot) = p.theta.get_mut(name) {
                        *slot = v;
                    }
                }
                Coord::OmegaLog(k) => p.l.set(*k, *k, libm::exp(x)),
                Coord::OmegaOff(i, j) => p.l.set(*i, *j, x),
                Coord::Xi(k) => put(&mut p.xi, *k, x),
            }
        }
        p
    }

    /// φ moved into the box of the bounds (FIT-ALG-03 shortens, FIT-ML-05 projects to 0).
    pub fn project(&self, phi: &mut [f64]) {
        for (j, x) in phi.iter_mut().enumerate() {
            let lo = at(&self.lower, j);
            let hi = at(&self.upper, j);
            if *x < lo {
                *x = lo;
            }
            if *x > hi {
                *x = hi;
            }
        }
    }

    /// Whether coordinate j of φ sits on a bound.
    pub fn on_bound(&self, phi: &[f64], j: usize) -> bool {
        let x = at(phi, j);
        x == at(&self.lower, j) || x == at(&self.upper, j)
    }

    /// The names of the quantities on the user's scale, in the order of [`Layout::user_values`]:
    /// every typical value, the elements of Ω (the diagonal for a diagonal Ω, the lower triangle
    /// row by row for a block), then the error parameters by key.
    pub fn user_names(&self, random_effects: &[String], error_keys: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = self.base.theta.keys().cloned().collect();
        for (i, a) in random_effects.iter().enumerate() {
            for (j, b) in random_effects.iter().enumerate().take(i + 1) {
                if self.block || i == j {
                    names.push(format!("omega[{a},{b}]"));
                }
            }
        }
        names.extend(error_keys.iter().map(|k| format!("xi.{k}")));
        names
    }

    /// The quantities on the user's scale at `p` (the trace and the delta method).
    pub fn user_values(&self, p: &Params) -> Vec<f64> {
        let mut out: Vec<f64> = p.theta.values().copied().collect();
        let omega = p.omega();
        let q = omega.rows();
        for i in 0..q {
            for j in 0..=i {
                if self.block || i == j {
                    out.push(omega.at(i, j));
                }
            }
        }
        out.extend(self.error_model.user_values(&p.xi));
        out
    }

    /// ∂(user values)/∂φ at `p`, (number of user values) × (length of φ), in closed form: θ for a
    /// log-scale typical value, the product rule on Ω = L·Lᵀ (with ∂L_kk/∂λ_k = L_kk), and the
    /// factor of FIT-ML-17 for the error parameters (`ErrorModel::delta_factor`).
    pub fn user_jacobian(&self, p: &Params) -> Mat {
        let theta_names: Vec<&String> = p.theta.keys().collect();
        let q = p.l.rows();
        let omega_index: Vec<(usize, usize)> = (0..q)
            .flat_map(|i| (0..=i).map(move |j| (i, j)))
            .filter(|(i, j)| self.block || i == j)
            .collect();
        let n_theta = theta_names.len();
        let n_omega = omega_index.len();
        let user_xi = self.error_model.user_values(&p.xi);
        let rows = n_theta + n_omega + user_xi.len();
        let mut jac = Mat::zeros(rows, self.len());
        let l = &p.l;
        for (c, coord) in self.coords.iter().enumerate() {
            match coord {
                Coord::Theta { name, log } => {
                    if let Some(r) = theta_names.iter().position(|n| *n == name) {
                        let x = p.theta.get(name).copied().unwrap_or(f64::NAN);
                        jac.set(r, c, if *log { x } else { 1.0 });
                    }
                }
                Coord::OmegaLog(a) => {
                    // ∂Ω_ij/∂L_aa = δ_ia·L_ja + δ_ja·L_ia, times ∂L_aa/∂λ_a = L_aa.
                    let laa = l.at(*a, *a);
                    for (r, &(i, j)) in omega_index.iter().enumerate() {
                        let d = omega_partial(l, i, j, *a, *a) * laa;
                        jac.set(n_theta + r, c, d);
                    }
                }
                Coord::OmegaOff(a, b) => {
                    for (r, &(i, j)) in omega_index.iter().enumerate() {
                        jac.set(n_theta + r, c, omega_partial(l, i, j, *a, *b));
                    }
                }
                Coord::Xi(k) => {
                    let factor = self.error_model.delta_factor(at(&user_xi, *k));
                    jac.set(n_theta + n_omega + k, c, factor);
                }
            }
        }
        jac
    }
}

/// ∂Ω_ij/∂L_ab for Ω = L·Lᵀ: δ_ia·L_jb + δ_ja·L_ib.
fn omega_partial(l: &Mat, i: usize, j: usize, a: usize, b: usize) -> f64 {
    let mut d = 0.0;
    if i == a {
        d += l.at(j, b);
    }
    if j == a {
        d += l.at(i, b);
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(block: bool) -> (Params, Layout) {
        let mut l = Mat::zeros(2, 2);
        l.set(0, 0, 0.5);
        l.set(1, 1, 0.3);
        if block {
            l.set(1, 0, 0.1);
        }
        let p = Params {
            theta: [("cl".to_string(), 2.0), ("tlag".to_string(), 0.4)]
                .into_iter()
                .collect(),
            l,
            xi: vec![0.04, 0.01],
        };
        let bounds = BTreeMap::new();
        let layout = Layout::new(
            p.clone(),
            ErrorModel::Combined,
            block,
            &Holds {
                fixed_theta: &[],
                with_random_effect: &["cl".to_string()],
                bounds: &bounds,
                fixed_omega: &[false, false],
                fixed_xi: &[false, false],
            },
        );
        (p, layout)
    }

    #[test]
    fn the_transform_round_trips() {
        for block in [false, true] {
            let (p, layout) = base(block);
            let phi = layout.phi(&p);
            assert_eq!(phi.len(), if block { 7 } else { 6 });
            let back = layout.params(&phi);
            for (a, b) in p.theta.values().zip(back.theta.values()) {
                assert!((a - b).abs() < 1e-14);
            }
            assert!((back.omega().at(1, 0) - p.omega().at(1, 0)).abs() < 1e-15);
        }
    }

    #[test]
    fn the_jacobian_matches_differences() {
        for block in [false, true] {
            let (p, layout) = base(block);
            let phi = layout.phi(&p);
            let jac = layout.user_jacobian(&p);
            for c in 0..phi.len() {
                let h = 1e-6;
                let mut up = phi.clone();
                let mut down = phi.clone();
                up[c] += h;
                down[c] -= h;
                let u = layout.user_values(&layout.params(&up));
                let d = layout.user_values(&layout.params(&down));
                for r in 0..u.len() {
                    let fd = (u[r] - d[r]) / (2.0 * h);
                    assert!((fd - jac.at(r, c)).abs() < 1e-7, "{block} {r} {c}");
                }
            }
        }
    }
}
