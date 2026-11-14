//! The objective at an arbitrary point (POP-OBJ-02, -03, -06): every subject's inner problem, then
//! the linearised OFV_i, with a warm start per subject. This is the one function the evaluation, the
//! outer iteration and the finite differences of the covariance all call.
//!
//! The per-subject [`Linearisation`] (E_i, the Cholesky factor of C_i, G, σ̂, PRED and IPRED) is
//! kept on request: the diagnostics of this card use it, and the CWRES of T-155 are built from it.

use std::collections::BTreeMap;

use crate::linalg::Mat;

use super::dense;
use super::inner::{self, InnerProblem};
use super::phi::{Layout, Params};
use super::structure::SubjectModel;
use super::{Method, PopError, Setup, covariance_failed, fo_derivatives_failed, inner_failed};

/// The linearisation of one subject at its conditional mode (at η = 0 for FO), POP-OBJ-02 and
/// POP-DIA-02: y_i ~ N(E_i, C_i) with E_i = ŷ(η̂) − G·η̂ and C_i = G·Ω·Gᵀ + diag(σ²).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Linearisation {
    /// E_i.
    pub e: Vec<f64>,
    /// The lower Cholesky factor of C_i.
    pub c_factor: Mat,
    /// G = ∂ŷ/∂η at η̂, one column per random effect.
    pub g: Vec<Vec<f64>>,
    /// σ̂_j at the individual prediction (σ⁰_j at the population prediction for `foce`).
    pub sigma: Vec<f64>,
    /// PRED = ŷ(0).
    pub pred: Vec<f64>,
    /// IPRED = ŷ(η̂) (PRED after FO).
    pub ipred: Vec<f64>,
    /// ln|C_i| + (y − E)ᵀC_i⁻¹(y − E): OFV_i in the convention `no_2pi`.
    pub core: f64,
}

/// One subject at one point.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SubjectValue {
    /// OFV_i in the convention `no_2pi` (0 for a subject with no observation used).
    pub core: f64,
    /// η̂_i; `None` after FO and for a subject with no observation.
    pub eta: Option<Vec<f64>>,
    /// The Newton decrement of η̂_i.
    pub decrement: Option<f64>,
    /// Kept when asked for.
    pub linearisation: Option<Linearisation>,
}

/// The objective at one point.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Evaluation {
    /// Every subject of the run, in dataset order.
    pub subjects: Vec<SubjectValue>,
    /// Σ_i OFV_i in the convention `no_2pi`, summed in subject order (POP-DET-01).
    pub core: f64,
}

impl Evaluation {
    /// The modes, as warm starts for the next evaluation (zeros where there is none).
    pub fn warm(&self, q: usize) -> Vec<Vec<f64>> {
        self.subjects
            .iter()
            .map(|s| s.eta.clone().unwrap_or_else(|| vec![0.0; q]))
            .collect()
    }
}

/// Why a point has no objective.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Failure {
    /// The inner problem of this subject failed (POP-INN-03 item 4).
    Inner(String),
    /// FO: G is not finite at η = 0 for this subject.
    FoDerivatives(String),
    /// C_i is not positive definite, or OFV_i is not finite, for this subject.
    Covariance(String),
    /// The parameters are not usable (a subject's own value collides with a typical value).
    Invalid(PopError),
}

impl Failure {
    /// The refusal of an evaluation at the starting values.
    pub fn error(self, method: Method) -> PopError {
        match self {
            Failure::Inner(id) => inner_failed(&id),
            Failure::FoDerivatives(id) => fo_derivatives_failed(&id),
            Failure::Covariance(id) => covariance_failed(&id, method),
            Failure::Invalid(e) => e,
        }
    }

    /// The inner problem failed (the status `inner_failed` of POP-EST-03), as opposed to a value
    /// that is not finite.
    pub fn is_inner(&self) -> bool {
        matches!(self, Failure::Inner(_))
    }
}

/// How an evaluation is run.
#[derive(Clone, Copy)]
pub(crate) struct Settings<'a> {
    /// The scoring tolerance of the inner problems ([`inner::EPS_IN`] for an evaluation).
    pub eps: f64,
    /// Keep the [`Linearisation`] of every subject.
    pub keep: bool,
    /// The test hook of POP-INN-03 item 4.
    pub fail_inner: &'a dyn Fn(&str) -> bool,
}

/// The step of the central differences of the polish for a random effect of prior variance ω²:
/// [`inner::POLISH_H`] times its prior standard deviation.
pub(crate) fn inner_step(variance: f64) -> f64 {
    let sd = variance.sqrt();
    let scale = if sd.is_finite() && sd > 0.0 { sd } else { 1.0 };
    inner::POLISH_H * scale
}

impl Setup {
    /// The objective at `p`, each inner problem started at `warm[i]` (zeros when `warm` is
    /// shorter), POP-INN-02 requirement 3.
    pub(crate) fn evaluate_params(
        &self,
        p: &Params,
        warm: &[Vec<f64>],
        settings: Settings<'_>,
    ) -> Result<Evaluation, Failure> {
        let q = self.random_effects.len();
        let omega = p.omega();
        let l_inv = p.l_inv();
        let steps: Vec<f64> = (0..q).map(|k| inner_step(omega.at(k, k))).collect();
        let zero = vec![0.0; q];
        let mut subjects = Vec::with_capacity(self.subjects.len());
        let mut total = 0.0;
        for (i, s) in self.subjects.iter().enumerate() {
            if s.y.is_empty() {
                subjects.push(SubjectValue {
                    core: 0.0,
                    eta: None,
                    decrement: None,
                    linearisation: None,
                });
                continue;
            }
            let theta_i = super::subject_theta(&p.theta, s).map_err(Failure::Invalid)?;
            let model = SubjectModel {
                structure: &self.structure,
                theta: &theta_i,
                random_effects: &self.random_effects,
                dose: s.dose,
                times: &s.times,
            };
            let needs_pred = settings.keep || self.method == Method::Foce;
            let pred = if needs_pred {
                Some(
                    model
                        .predict(&zero)
                        .ok_or_else(|| Failure::Covariance(s.id.clone()))?,
                )
            } else {
                None
            };
            let value = match self.method {
                Method::Fo => {
                    let ev = model
                        .evaluate(&zero)
                        .ok_or_else(|| Failure::FoDerivatives(s.id.clone()))?;
                    let sigma0: Vec<f64> = ev
                        .pred
                        .iter()
                        .map(|&f| self.error_model.sigma(&p.xi, f))
                        .collect();
                    let lin = linearise(&s.y, &ev.pred, &ev.g, &zero, &sigma0, &omega)
                        .ok_or_else(|| Failure::Covariance(s.id.clone()))?;
                    SubjectValue {
                        core: lin.core,
                        eta: None,
                        decrement: None,
                        linearisation: settings.keep.then(|| Linearisation {
                            pred: ev.pred.clone(),
                            ipred: ev.pred.clone(),
                            ..lin
                        }),
                    }
                }
                Method::FoceI | Method::Foce => {
                    let sigma0: Option<Vec<f64>> = (self.method == Method::Foce).then(|| {
                        pred.iter()
                            .flatten()
                            .map(|&f| self.error_model.sigma(&p.xi, f))
                            .collect()
                    });
                    let problem = InnerProblem {
                        model: &model,
                        y: &s.y,
                        error_model: self.error_model,
                        xi: &p.xi,
                        frozen: sigma0.as_deref(),
                        l_inv: &l_inv,
                        steps: &steps,
                        eps: settings.eps,
                    };
                    let start = warm
                        .get(i)
                        .filter(|w| w.len() == q)
                        .map_or(zero.as_slice(), Vec::as_slice);
                    let mode = if (settings.fail_inner)(&s.id) {
                        None
                    } else {
                        problem.solve(start)
                    };
                    let point = mode.ok_or_else(|| Failure::Inner(s.id.clone()))?;
                    // POP-OBJ-02: σ̂ at the individual prediction; POP-OBJ-06: σ⁰.
                    let sigma = sigma0.as_ref().unwrap_or(&point.sigma);
                    let lin =
                        linearise(&s.y, &point.ev.pred, &point.ev.g, &point.eta, sigma, &omega)
                            .ok_or_else(|| Failure::Covariance(s.id.clone()))?;
                    SubjectValue {
                        core: lin.core,
                        decrement: Some(point.decrement),
                        linearisation: settings.keep.then(|| Linearisation {
                            pred: pred.clone().unwrap_or_default(),
                            ipred: point.ev.pred.clone(),
                            ..lin
                        }),
                        eta: Some(point.eta),
                    }
                }
            };
            total += value.core;
            subjects.push(value);
        }
        if !total.is_finite() {
            let id = self
                .subjects
                .first()
                .map(|s| s.id.clone())
                .unwrap_or_default();
            return Err(Failure::Covariance(id));
        }
        Ok(Evaluation {
            subjects,
            core: total,
        })
    }

    /// The objective at an arbitrary φ of `layout`, with a warm start per subject by label (the API
    /// the review of T-153 asked for; a label that is not a subject of the run is ignored).
    #[allow(dead_code)] // the entry point of T-155 and of the hosts; the fit itself calls `evaluate_params`
    pub(crate) fn evaluate_phi(
        &self,
        layout: &Layout,
        phi: &[f64],
        warm: &BTreeMap<String, Vec<f64>>,
        settings: Settings<'_>,
    ) -> Result<Evaluation, Failure> {
        let starts: Vec<Vec<f64>> = self
            .subjects
            .iter()
            .map(|s| warm.get(&s.id).cloned().unwrap_or_default())
            .collect();
        self.evaluate_params(&layout.params(phi), &starts, settings)
    }
}

/// The linearisation of POP-OBJ-02 from ŷ, G, η and σ: E = ŷ − G·η, C = G·Ω·Gᵀ + diag(σ²), its
/// Cholesky factor, and ln|C| + (y − E)ᵀC⁻¹(y − E); `None` when C is not positive definite or the
/// value is not finite. PRED and IPRED are left empty for the caller.
pub(crate) fn linearise(
    y: &[f64],
    pred: &[f64],
    g: &[Vec<f64>],
    eta: &[f64],
    sigma: &[f64],
    omega: &Mat,
) -> Option<Linearisation> {
    let n = y.len();
    let q = eta.len();
    let at = |j: usize, k: usize| g.get(k).and_then(|c| c.get(j)).copied().unwrap_or(f64::NAN);
    let mut e = Vec::with_capacity(n);
    let mut resid = Vec::with_capacity(n);
    for (j, (&yj, &fj)) in y.iter().zip(pred).enumerate() {
        let shift: f64 = eta.iter().enumerate().map(|(k, x)| at(j, k) * x).sum();
        let ej = fj - shift;
        e.push(ej);
        resid.push(yj - ej);
    }
    // G·Ω, n × q.
    let mut g_omega = Mat::zeros(n, q);
    for j in 0..n {
        for l in 0..q {
            let s: f64 = (0..q).map(|k| at(j, k) * omega.at(k, l)).sum();
            g_omega.set(j, l, s);
        }
    }
    let mut c = Mat::zeros(n, n);
    for i in 0..n {
        for j in 0..=i {
            let mut s: f64 = (0..q).map(|l| g_omega.at(i, l) * at(j, l)).sum();
            if i == j {
                let sd = sigma.get(i).copied()?;
                s += sd * sd;
            }
            c.set(i, j, s);
            c.set(j, i, s);
        }
    }
    let factor = dense::cholesky(&c)?;
    let log_det: f64 = (0..n).map(|i| 2.0 * libm::log(factor.at(i, i))).sum();
    let w = dense::forward(&factor, &resid);
    let distance: f64 = w.iter().map(|x| x * x).sum();
    let core = log_det + distance;
    core.is_finite().then(|| Linearisation {
        e,
        c_factor: factor,
        g: g.to_vec(),
        sigma: sigma.to_vec(),
        pred: Vec::new(),
        ipred: Vec::new(),
        core,
    })
}
