//! The refusals of POP-INN-03 that need the model or the data: `unbounded_likelihood` (item 1),
//! `non_positive_sigma` (item 2), `too_few_subjects` and `too_few_observations` (item 3).

use caladrius_models::ModelId;

use crate::ErrorModel;
use crate::model::FitModel;

use super::data::Subject;
use super::{PopError, PopErrorCode};

/// The domain of `tlag`: a fixed value c is [c, c]; otherwise its bounds, [0, ∞) by default
/// (FIT-BND-03: `tlag` >= 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LagDomain {
    pub lower: f64,
    pub upper: f64,
}

/// The rows of one subject that make the likelihood unbounded (FIT-ML-13c, POP-INN-03 item 1):
/// `(lag, rows from 0)`, or `None`.
fn unbounded_rows(
    model: ModelId,
    em: ErrorModel,
    subject: &Subject,
    lag: Option<LagDomain>,
) -> Option<(bool, Vec<usize>)> {
    let combined = matches!(em, ErrorModel::Combined | ErrorModel::CombinedSd);
    if combined && model.silent_at_dose_time() {
        let at_dose: Vec<usize> = subject
            .y
            .iter()
            .zip(&subject.times)
            .enumerate()
            .filter(|(_, (y, t))| **y == 0.0 && **t == 0.0)
            .map(|(i, _)| i)
            .collect();
        if !at_dose.is_empty() {
            return Some((false, at_dose));
        }
    }
    if em == ErrorModel::Additive {
        return None;
    }
    model.lag_parameter()?;
    let domain = lag?;
    let first_nonzero = subject
        .y
        .iter()
        .zip(&subject.times)
        .filter(|(y, _)| **y != 0.0)
        .map(|(_, t)| *t)
        .fold(f64::INFINITY, f64::min);
    if domain.lower >= first_nonzero {
        return None;
    }
    let rows: Vec<usize> = subject
        .y
        .iter()
        .zip(&subject.times)
        .enumerate()
        .filter(|(_, (y, t))| **y == 0.0 && **t > 0.0 && **t < first_nonzero && domain.upper >= **t)
        .map(|(i, _)| i)
        .collect();
    (!rows.is_empty()).then_some((true, rows))
}

/// POP-INN-03 item 1, checked for every subject before anything else that needs a prediction.
/// The message has one sentence per affected subject, in dataset order.
pub(crate) fn refuse_unbounded(
    model: ModelId,
    em: ErrorModel,
    subjects: &[Subject],
    lag: Option<LagDomain>,
) -> Result<(), PopError> {
    let sentences: Vec<String> = subjects
        .iter()
        .filter_map(|s| {
            let (lagged, rows) = unbounded_rows(model, em, s, lag)?;
            Some(unbounded_sentence(em, s, lagged, &rows))
        })
        .collect();
    if sentences.is_empty() {
        return Ok(());
    }
    Err(PopError {
        code: PopErrorCode::UnboundedLikelihood,
        message: sentences.join(" "),
    })
}

/// The sentences of FIT-ML-13c with "in subject {S}, " inserted after "here: " (POP-INN-03 item 1).
fn unbounded_sentence(em: ErrorModel, s: &Subject, lag: bool, rows: &[usize]) -> String {
    let list = rows
        .iter()
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let at = rows
        .iter()
        .map(|&i| format!("time {}", s.times.get(i).copied().unwrap_or(f64::NAN)))
        .collect::<Vec<_>>()
        .join(", ");
    let id = em.id();
    if lag {
        format!(
            "The {id} error model has no maximum likelihood here: in subject {}, observation(s) {list} ({at}) are exactly 0 before the first nonzero observation, and with a lag the model predicts exactly 0 up to `tlag`, which is, or which the fit can move to, at or beyond those times, so the standard deviation can shrink to 0 there without cost. Exclude those observations, bound or fix `tlag` below the first of them, or use the additive error model.",
            s.id
        )
    } else {
        format!(
            "The {id} error model has no maximum likelihood here: in subject {}, observation(s) {list} ({at}) are exactly 0 where the model predicts exactly 0 for every parameter value, so the standard deviation can shrink to 0 there without cost. Exclude those observations, or use the additive error model.",
            s.id
        )
    }
}

/// POP-INN-03 item 2: σ at the start point (η = 0) must be finite and > 0 at every observation used.
/// `pred0` holds the population prediction of each subject.
pub(crate) fn refuse_non_positive_sigma(
    em: ErrorModel,
    xi: &[f64],
    subjects: &[Subject],
    pred0: &[Vec<f64>],
) -> Result<(), PopError> {
    for (s, pred) in subjects.iter().zip(pred0) {
        for (j, (&f, &t)) in pred.iter().zip(&s.times).enumerate() {
            let sigma = em.sigma(xi, f);
            if sigma.is_finite() && sigma > 0.0 {
                continue;
            }
            let n = j + 1;
            let message = if em == ErrorModel::Proportional {
                format!(
                    "The proportional error model gives a standard deviation of 0 at observation {n} at time {t} of subject {} (the model predicts {f} there). A proportional error cannot describe an observation where the prediction is 0. Leave that observation out, or use the additive or the combined error model.",
                    s.id
                )
            } else {
                format!(
                    "The {} error model gives a standard deviation of {sigma} at observation {n} at time {t} of subject {} (the model predicts {f} there). Give a positive `a` in `error_init`, or leave that observation out.",
                    em.id(),
                    s.id
                )
            };
            return Err(PopError {
                code: PopErrorCode::NonPositiveSigma,
                message,
            });
        }
    }
    Ok(())
}

/// POP-INN-03 item 3. `p` is the number of free typical values, `q` the number of free error
/// parameters; the elements of Ω are not counted.
pub(crate) fn refuse_too_few(
    em: ErrorModel,
    subjects: &[Subject],
    p: usize,
    q: usize,
) -> Result<(), PopError> {
    let m = subjects.iter().filter(|s| !s.y.is_empty()).count();
    if m < 2 {
        return Err(PopError {
            code: PopErrorCode::TooFewSubjects,
            message: format!(
                "the population fit needs at least 2 subjects to estimate the variance of the random effects; this dataset has {m} subject(s) with usable observations"
            ),
        });
    }
    let n: usize = subjects.iter().map(|s| s.y.len()).sum();
    if n <= p + q {
        return Err(PopError {
            code: PopErrorCode::TooFewObservations,
            message: format!(
                "The population fit with the {} error model estimates {} parameters ({p} typical values and {q} of the error) from {n} observations. It needs more observations than parameters. Fix a typical value, choose an error model with fewer parameters, or add observations.",
                em.id(),
                p + q
            ),
        });
    }
    Ok(())
}
