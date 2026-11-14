//! PRED, IPRED and IWRES per observation (POP-DIA-01), shrinkage and information criteria
//! (POP-DIA-03, POP-DIA-04), computed from an [`Evaluation`] that kept its linearisations. Every
//! answer carries them, an evaluation's as a fit's.
//!
//! The standard deviations are two-pass (the mean first, then the sum of squared deviations), with
//! the sums in subject order and observation order (POP-DET-01).

use crate::ErrorModel;
use crate::linalg::Mat;

use super::data::Subject;
use super::evaluation::{Evaluation, SubjectValue};
use super::{Method, ObservationAnswer};

/// η-shrinkage and ε-shrinkage.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shrinkage {
    /// 1 − SD(η̂_k)/ω̂_k per random effect (divisor m₁ − 1); `None` where NC.
    pub eta: Vec<Option<f64>>,
    /// 1 − SD(IWRES) over all observations used (divisor N − 1); `None` where NC.
    pub eps: Option<f64>,
}

/// IWRES = (y − IPRED)/g(IPRED; ξ) (POP-DIA-01), for `foce` as for `foce_i`.
fn iwres(error_model: ErrorModel, xi: &[f64], y: f64, ipred: f64) -> f64 {
    (y - ipred) / error_model.sigma(xi, ipred)
}

/// POP-DIA-01 for one subject at the point of `value`, which must have kept its linearisation (an
/// evaluation that did not gives no row): one row per observation used, with the error parameters
/// `xi` (moved). FO has no η̂, so its IPRED and IWRES are null (POP-OBJ-03).
pub(crate) fn observations(
    method: Method,
    error_model: ErrorModel,
    xi: &[f64],
    subject: &Subject,
    value: &SubjectValue,
) -> Vec<ObservationAnswer> {
    let Some(lin) = &value.linearisation else {
        return Vec::new();
    };
    let individual = method != Method::Fo;
    subject
        .times
        .iter()
        .zip(&subject.y)
        .zip(lin.pred.iter().zip(&lin.ipred))
        .map(|((&time, &y), (&pred, &ipred))| {
            let w = iwres(error_model, xi, y, ipred);
            ObservationAnswer {
                time,
                y,
                pred,
                ipred: individual.then_some(ipred),
                iwres: (individual && w.is_finite()).then_some(w),
            }
        })
        .collect()
}

/// Mean and two-pass standard deviation with divisor n − 1; `None` below 2 values.
fn sd(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let ss: f64 = values.iter().map(|x| (x - mean) * (x - mean)).sum();
    let s = (ss / (n - 1.0)).sqrt();
    s.is_finite().then_some(s)
}

/// POP-DIA-03 at the point of `evaluation` (which must have kept its linearisations), with the
/// error parameters `xi` (moved) and Ω. NC after FO, when m₁ < 2, or when ω̂_k = 0.
pub(crate) fn shrinkage(
    method: Method,
    error_model: ErrorModel,
    xi: &[f64],
    omega: &Mat,
    subjects: &[Subject],
    evaluation: &Evaluation,
) -> Shrinkage {
    let q = omega.rows();
    if method == Method::Fo {
        return Shrinkage {
            eta: vec![None; q],
            eps: None,
        };
    }
    let etas: Vec<&Vec<f64>> = evaluation
        .subjects
        .iter()
        .filter_map(|s| s.eta.as_ref())
        .collect();
    let eta = (0..q)
        .map(|k| {
            let column: Vec<f64> = etas.iter().filter_map(|e| e.get(k).copied()).collect();
            let omega_k = omega.at(k, k).sqrt();
            if !(omega_k.is_finite() && omega_k > 0.0) {
                return None;
            }
            sd(&column).map(|s| 1.0 - s / omega_k)
        })
        .collect();
    let mut residuals = Vec::new();
    for (s, value) in subjects.iter().zip(&evaluation.subjects) {
        let Some(lin) = &value.linearisation else {
            continue;
        };
        for (&y, &f) in s.y.iter().zip(&lin.ipred) {
            residuals.push(iwres(error_model, xi, y, f));
        }
    }
    let eps = sd(&residuals).map(|s| 1.0 - s);
    Shrinkage { eta, eps }
}

/// AIC = OFV + 2k and BIC = OFV + k·ln N (POP-DIA-04); `None` when the OFV is not finite.
pub(crate) fn criteria(ofv: f64, k: usize, n: usize) -> (Option<f64>, Option<f64>) {
    if !ofv.is_finite() || n == 0 {
        return (None, None);
    }
    let k = k as f64;
    let aic = ofv + 2.0 * k;
    let bic = ofv + k * libm::log(n as f64);
    (Some(aic), Some(bic))
}
