//! The two-stage start of POP-EST-05: one maximum-likelihood fit per subject (brick 2), then the
//! geometric means of the typical values, the sample covariance of their logarithms, and the mean of
//! the error parameters on the variance scale.
//!
//! It runs only for a fit (not for `evaluate_only`), only for the catalogue models (the test model
//! has no individual fit), and only when the request leaves a typical value of a random effect or Ω
//! out. A value the request gives is used as given (POP-EST-05: "the user's initial values, where
//! given, are used").

use std::collections::BTreeMap;

use caladrius_models::ModelId;

use crate::linalg::Mat;
use crate::{ErrorModel, FitInput, FitOptions, FitStatus, Weighting};

use super::data::Subject;
use super::phi::at;
use super::{PopError, invalid};

/// What the two-stage start gives.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TwoStage {
    /// θ⁰ for every name asked for.
    pub theta: BTreeMap<String, f64>,
    /// Ω⁰, q × q, in the order of the random effects (diagonal for `diagonal`).
    pub omega: Mat,
    /// ξ⁰ on the moved scale (FIT-ML-05), the mean over the surviving subjects.
    pub xi: Vec<f64>,
    /// The subjects left out of the means, in dataset order.
    pub left_out: Vec<String>,
    /// m₂.
    pub survivors: usize,
    /// Set when a block Ω⁰ was not positive definite and its diagonal is used.
    pub note: Option<String>,
}

/// One parameter of an individual result, converting between the rate constant and the clearance
/// of a one-compartment model (cl = k·v) when the individual fit used the other one (FIT-INI-01
/// gives v and k). `None` when the result has neither.
fn individual_value(values: &BTreeMap<String, f64>, name: &str) -> Option<f64> {
    let get = |n: &str| values.get(&format!("estimate.{n}")).copied();
    if let Some(x) = get(name) {
        return Some(x);
    }
    match name {
        "cl" => Some(get("k")? * get("v")?),
        "k" => Some(get("cl")? / get("v")?),
        _ => None,
    }
}

/// POP-EST-05 for the parameters `names` (the typical values of the population fit that are not
/// fixed), given `fixed` (held typical values, passed to every individual fit) and `initial` (the
/// user's values, used as the initial estimates of each individual fit when they name every
/// parameter; otherwise FIT-INI-01 generates them per subject).
#[allow(clippy::too_many_arguments)]
pub(crate) fn two_stage(
    model: ModelId,
    names: &[String],
    random_effects: &[String],
    block: bool,
    error_model: ErrorModel,
    fixed: &BTreeMap<String, f64>,
    initial: &BTreeMap<String, f64>,
    subjects: &[Subject],
) -> Result<TwoStage, PopError> {
    let complete = names.iter().all(|n| initial.contains_key(n));
    let mut rows: Vec<Vec<f64>> = Vec::new();
    let mut xis: Vec<Vec<f64>> = Vec::new();
    let mut left_out = Vec::new();
    for s in subjects {
        if s.y.is_empty() {
            continue;
        }
        let mut options = FitOptions::maximum_likelihood(error_model);
        options.fixed = fixed.clone();
        if let Some(d) = s.duration {
            options.fixed.insert("dur".to_string(), d);
        }
        let input = FitInput {
            model,
            dose: s.dose,
            time: s.times.clone(),
            conc: s.y.clone(),
            weighting: Weighting::Uniform,
            initial: if complete {
                names
                    .iter()
                    .filter_map(|n| initial.get(n).map(|x| (n.clone(), *x)))
                    .collect()
            } else {
                BTreeMap::new()
            },
            options,
        };
        let row = crate::run(&input).ok().and_then(|r| {
            if r.status() != FitStatus::Converged {
                return None;
            }
            let values = r.values();
            let theta: Option<Vec<f64>> = names
                .iter()
                .map(|n| individual_value(values, n).filter(|x| x.is_finite()))
                .collect();
            let user: Option<Vec<f64>> = error_model
                .names()
                .iter()
                .map(|n| values.get(&format!("estimate.{n}")).copied())
                .collect();
            Some((theta?, error_model.moved_from_user(&user?)))
        });
        match row {
            Some((theta, xi)) => {
                rows.push(theta);
                xis.push(xi);
            }
            None => left_out.push(s.id.clone()),
        }
    }
    let m2 = rows.len();
    if m2 < 2 {
        return Err(invalid(
            "initial",
            format!(
                "the two-stage start needs at least 2 subjects whose individual fit converges and {m2} did; give the typical values in `initial` and the matrix `omega`"
            ),
        ));
    }
    let n2 = m2 as f64;
    let mut theta = BTreeMap::new();
    for (j, name) in names.iter().enumerate() {
        let column: Vec<f64> = rows.iter().map(|r| at(r, j)).collect();
        let value = if random_effects.contains(name) {
            if column.iter().any(|x| x.is_nan() || *x <= 0.0) {
                return Err(invalid(
                    "initial",
                    format!(
                        "the two-stage start needs positive individual values of `{name}`; give its typical value in `initial`"
                    ),
                ));
            }
            libm::exp(column.iter().map(|x| libm::log(*x)).sum::<f64>() / n2)
        } else {
            column.iter().sum::<f64>() / n2
        };
        theta.insert(name.clone(), value);
    }
    // Ω⁰: the sample covariance of the logarithms, divisor m₂ − 1.
    let q = random_effects.len();
    let logs: Vec<Vec<f64>> = rows
        .iter()
        .map(|r| {
            random_effects
                .iter()
                .map(|re| {
                    names
                        .iter()
                        .position(|n| n == re)
                        .map_or(f64::NAN, |j| libm::log(at(r, j)))
                })
                .collect()
        })
        .collect();
    let means: Vec<f64> = (0..q)
        .map(|k| logs.iter().map(|r| at(r, k)).sum::<f64>() / n2)
        .collect();
    let mut omega = Mat::zeros(q, q);
    for a in 0..q {
        for b in 0..=a {
            if !block && a != b {
                continue;
            }
            let s: f64 = logs
                .iter()
                .map(|r| (at(r, a) - at(&means, a)) * (at(r, b) - at(&means, b)))
                .sum::<f64>()
                / (n2 - 1.0);
            omega.set(a, b, s);
            omega.set(b, a, s);
        }
    }
    for (k, re) in random_effects.iter().enumerate() {
        let d = omega.at(k, k);
        if !(d.is_finite() && d > 0.0) {
            return Err(invalid(
                "omega",
                format!(
                    "the two-stage start gives a variance of {d} for the random effect of `{re}` (the individual values do not vary); give the matrix `omega`"
                ),
            ));
        }
    }
    let mut note = None;
    if block && super::dense::cholesky(&omega).is_none() {
        for a in 0..q {
            for b in 0..q {
                if a != b {
                    omega.set(a, b, 0.0);
                }
            }
        }
        note = Some(
            "the two-stage covariance of the random effects is not positive definite; its diagonal is used for the start".to_string(),
        );
    }
    let nx = error_model.q();
    // Summed in subject order.
    let xi: Vec<f64> = (0..nx)
        .map(|k| xis.iter().map(|row| at(row, k)).sum::<f64>() / n2)
        .collect();
    Ok(TwoStage {
        theta,
        omega,
        xi,
        left_out,
        survivors: m2,
        note,
    })
}
