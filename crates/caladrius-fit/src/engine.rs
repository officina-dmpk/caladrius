//! The minimiser (`specs/fit.md` section 3 to 5): Gauss-Newton direction from a QR factorisation
//! of the column-scaled weighted Jacobian, Hartley step halving, Levenberg-Marquardt damping when
//! halving fails, predicted-value weights recomputed at every iteration (FIT-WGT-03).

use std::collections::BTreeMap;

use crate::linalg::{Mat, least_squares};
use crate::model::FitModel;
use crate::result::{self, FitResult, TraceRow};
use crate::{Criterion, Derivatives, FitError, FitInput, FitOptions, FitStatus, Weighting};

/// Canonical parameter order (MOD-VOC-01); other names follow in alphabetical order.
const ORDER: [&str; 6] = ["v", "cl", "k", "ka", "dur", "tlag"];
/// Smallest step factor of the halving (FIT-ALG-03).
const MIN_STEP: f64 = 1.0 / 1024.0;
/// Damping: first value, factor, largest value (FIT-ALG-04).
const LAMBDA_START: f64 = 1e-3;
const LAMBDA_FACTOR: f64 = 10.0;
const LAMBDA_MAX: f64 = 1e10;

/// A checked fitting problem.
pub(crate) struct Problem<'a> {
    pub model: &'a dyn FitModel,
    pub model_id: String,
    pub dose: f64,
    pub time: Vec<f64>,
    pub y: Vec<f64>,
    pub weighting: Weighting,
    /// Fitted parameters, in canonical order.
    pub names: Vec<String>,
    theta0: Vec<f64>,
    pub options: FitOptions,
}

fn invalid_option(option: &str, reason: &str) -> FitError {
    FitError::InvalidOption {
        option: option.to_string(),
        reason: reason.to_string(),
    }
}

impl<'a> Problem<'a> {
    /// Checks the input (FIT-ERR-01, WGT-02) and the initial estimates.
    pub fn new(model: &'a dyn FitModel, input: &FitInput) -> Result<Self, FitError> {
        let o = &input.options;
        if !(o.increment.is_finite() && o.increment > 0.0) {
            return Err(invalid_option(
                "increment",
                "must be a finite number > 0 (0.001 is usual)",
            ));
        }
        if !(o.convergence.is_finite() && o.convergence >= 0.0) {
            return Err(invalid_option(
                "convergence",
                "must be a finite number >= 0 (0.0001 is usual)",
            ));
        }
        if !(o.confidence_level > 0.0 && o.confidence_level < 1.0) {
            return Err(invalid_option(
                "confidence_level",
                "must lie strictly between 0 and 1 (0.95 is usual)",
            ));
        }
        if input.time.len() != input.conc.len() {
            return Err(FitError::LengthMismatch {
                times: input.time.len(),
                concs: input.conc.len(),
            });
        }
        let mut names: Vec<String> = ORDER
            .iter()
            .filter(|n| input.initial.contains_key(**n))
            .map(|n| (*n).to_string())
            .collect();
        names.extend(
            input
                .initial
                .keys()
                .filter(|k| !ORDER.contains(&k.as_str()))
                .cloned(),
        );
        if names.is_empty() {
            return Err(FitError::NoParameters);
        }
        let (n, p) = (input.time.len(), names.len());
        if n < p {
            return Err(FitError::TooFewObservations { n, p });
        }
        for (index, (&t, &c)) in input.time.iter().zip(&input.conc).enumerate() {
            if !t.is_finite() {
                return Err(FitError::NonFiniteObservation {
                    index,
                    field: "time".to_string(),
                });
            }
            if !c.is_finite() {
                return Err(FitError::NonFiniteObservation {
                    index,
                    field: "concentration".to_string(),
                });
            }
            if t < 0.0 {
                return Err(FitError::TimeBeforeDose { index, time: t });
            }
            if matches!(input.weighting, Weighting::InvY | Weighting::InvY2) && c <= 0.0 {
                return Err(FitError::UnweightableObservation {
                    index,
                    time: t,
                    value: c,
                    weighting: input.weighting.id().to_string(),
                });
            }
        }
        if !(input.dose.is_finite() && input.dose > 0.0) {
            return Err(FitError::InvalidDose { value: input.dose });
        }
        let theta0: Vec<f64> = names
            .iter()
            .map(|n| input.initial.get(n).copied().unwrap_or(f64::NAN))
            .collect();
        let problem = Self {
            model,
            model_id: input.model.id().to_string(),
            dose: input.dose,
            time: input.time.clone(),
            y: input.conc.clone(),
            weighting: input.weighting,
            names,
            theta0,
            options: *o,
        };
        let pred = problem
            .model
            .predict(
                problem.dose,
                &problem.params(&problem.theta0),
                &problem.time,
            )
            .map_err(|source| FitError::InitialEstimates { source })?;
        let unusable = pred
            .iter()
            .position(|&f| !f.is_finite() || (problem.weighting.uses_predictions() && f <= 0.0));
        if let Some(index) = unusable {
            return Err(FitError::UnusableInitialPrediction {
                index,
                time: problem.time.get(index).copied().unwrap_or(f64::NAN),
            });
        }
        if o.derivatives == Derivatives::Analytic
            && problem
                .model
                .analytic_derivatives(
                    problem.dose,
                    &problem.params(&problem.theta0),
                    &problem.time,
                    &problem.names,
                )
                .is_none()
        {
            return Err(FitError::AnalyticDerivativesUnavailable {
                model: problem.model_id.clone(),
            });
        }
        Ok(problem)
    }

    pub fn params(&self, theta: &[f64]) -> BTreeMap<String, f64> {
        self.names
            .iter()
            .cloned()
            .zip(theta.iter().copied())
            .collect()
    }

    /// Predictions, or `None` when the model refuses the parameters or gives a non-finite value.
    pub fn predict_at(&self, theta: &[f64], times: &[f64]) -> Option<Vec<f64>> {
        if theta.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let pred = self
            .model
            .predict(self.dose, &self.params(theta), times)
            .ok()?;
        pred.iter().all(|f| f.is_finite()).then_some(pred)
    }

    pub fn predict(&self, theta: &[f64]) -> Option<Vec<f64>> {
        self.predict_at(theta, &self.time)
    }

    /// Weights for the predictions `pred` (FIT-WGT-01); `None` when a predicted-value weight is
    /// undefined (ŷ <= 0).
    pub fn weights(&self, pred: &[f64]) -> Option<Vec<f64>> {
        let from = |x: f64, power: i32| (x > 0.0).then(|| 1.0 / x.powi(power));
        match self.weighting {
            Weighting::Uniform => Some(vec![1.0; self.y.len()]),
            Weighting::InvY => self.y.iter().map(|&y| from(y, 1)).collect(),
            Weighting::InvY2 => self.y.iter().map(|&y| from(y, 2)).collect(),
            Weighting::InvYhat => pred.iter().map(|&f| from(f, 1)).collect(),
            Weighting::InvYhat2 => pred.iter().map(|&f| from(f, 2)).collect(),
        }
    }

    pub fn wrss(&self, pred: &[f64], w: &[f64]) -> f64 {
        self.y
            .iter()
            .zip(pred)
            .zip(w)
            .map(|((y, f), w)| w * (y - f).powi(2))
            .sum()
    }

    /// ∂f(t_i)/∂θ_j (unweighted), one column per parameter (FIT-JAC-01, JAC-02).
    pub fn jacobian(&self, theta: &[f64], pred: &[f64]) -> Option<Vec<Vec<f64>>> {
        if self.options.derivatives == Derivatives::Analytic {
            let columns = self.model.analytic_derivatives(
                self.dose,
                &self.params(theta),
                &self.time,
                &self.names,
            )?;
            return columns
                .iter()
                .all(|c| c.iter().all(|x| x.is_finite()))
                .then_some(columns);
        }
        let h = self.options.increment;
        let mut columns = Vec::with_capacity(theta.len());
        for (j, &x) in theta.iter().enumerate() {
            let step = if x == 0.0 { h } else { h * x.abs() };
            let mut moved = theta.to_vec();
            if let Some(slot) = moved.get_mut(j) {
                *slot = x + step;
            }
            let shifted = self.predict(&moved)?;
            columns.push(
                shifted
                    .iter()
                    .zip(pred)
                    .map(|(a, b)| (a - b) / step)
                    .collect(),
            );
        }
        Some(columns)
    }

    /// The weighted Jacobian with unit-length columns, the column norms, and the weighted
    /// residuals. `None` when a column is zero or not finite.
    pub fn scaled_system(
        &self,
        columns: &[Vec<f64>],
        pred: &[f64],
        w: &[f64],
    ) -> Option<(Mat, Vec<f64>, Vec<f64>)> {
        let sqrt_w: Vec<f64> = w.iter().map(|x| x.sqrt()).collect();
        let weighted: Vec<Vec<f64>> = columns
            .iter()
            .map(|c| c.iter().zip(&sqrt_w).map(|(d, s)| d * s).collect())
            .collect();
        let a = Mat::from_columns(&weighted);
        let norms: Vec<f64> = (0..a.cols()).map(|j| a.column_norm(j)).collect();
        if norms.iter().any(|c| !(c.is_finite() && *c > 0.0)) {
            return None;
        }
        let mut scaled = a.clone();
        for (j, c) in norms.iter().enumerate() {
            for i in 0..a.rows() {
                scaled.set(i, j, a.at(i, j) / c);
            }
        }
        let r: Vec<f64> = self
            .y
            .iter()
            .zip(pred)
            .zip(&sqrt_w)
            .map(|((y, f), s)| s * (y - f))
            .collect();
        Some((scaled, norms, r))
    }

    /// The minimisation (FIT-ALG-02 to 04, CNV-01 to 03), then the statistics (section 7).
    pub fn solve(&self) -> FitResult {
        let eps = self.options.convergence;
        let mut theta = self.theta0.clone();
        // Checked in `new`: the initial prediction exists and its weights are defined.
        let mut pred = self.predict(&theta).unwrap_or_default();
        let ss_weighted_start = self.weights(&pred).map_or(0.0, |w| {
            self.y.iter().zip(&w).map(|(y, w)| w * y * y).sum::<f64>()
        });
        let wrss_own = |pred: &[f64]| self.weights(pred).map_or(f64::NAN, |w| self.wrss(pred, &w));
        let mut trace = vec![TraceRow {
            iteration: 0,
            wrss: wrss_own(&pred),
            estimates: theta.clone(),
            step: None,
            lambda: 0.0,
            halvings: 0,
            relative_decrease: None,
        }];
        let mut lambda = 0.0;
        let mut status = FitStatus::MaxIterations;
        for iteration in 1..=self.options.max_iterations {
            // FIT-WGT-03: weights of this iteration, constants in the derivatives.
            let Some(w) = self.weights(&pred) else {
                status = FitStatus::NonFinite;
                break;
            };
            let s_cur = self.wrss(&pred, &w);
            // An exact fit (residuals at rounding level) cannot decrease any further.
            if s_cur <= 1e-26 * ss_weighted_start {
                status = FitStatus::Converged;
                break;
            }
            let Some(columns) = self.jacobian(&theta, &pred) else {
                status = FitStatus::NonFinite;
                break;
            };
            let Some((a, norms, r)) = self.scaled_system(&columns, &pred, &w) else {
                status = FitStatus::Singular;
                break;
            };
            let p = norms.len();
            // Pure Gauss-Newton direction; ‖J·δ‖² is the decrease it predicts.
            let gn = least_squares(&a, &r);
            let predicted = gn.as_ref().map(|(d, _)| {
                let jd: f64 = (0..a.rows())
                    .map(|i| {
                        (0..p)
                            .map(|j| a.at(i, j) * d.get(j).copied().unwrap_or(0.0))
                            .sum::<f64>()
                            .powi(2)
                    })
                    .sum();
                jd
            });
            if self.options.criterion == Criterion::RelativeOffset {
                let n = self.y.len();
                if let Some(q) = predicted {
                    if n > p {
                        let offset =
                            ((q / p as f64) / ((s_cur - q).max(0.0) / (n - p) as f64)).sqrt();
                        if offset <= eps {
                            status = FitStatus::Converged;
                            break;
                        }
                    }
                }
            }
            let mut accepted = None;
            let mut lam = lambda;
            let mut at_minimum = false;
            loop {
                let direction = if lam == 0.0 {
                    gn.as_ref().map(|(d, _)| d.clone())
                } else {
                    damped(&a, &r, lam)
                };
                if let Some(scaled_delta) = direction {
                    let delta: Vec<f64> = scaled_delta
                        .iter()
                        .zip(&norms)
                        .map(|(d, c)| d / c)
                        .collect();
                    let mut nu = 1.0;
                    let mut halvings = 0;
                    while nu >= MIN_STEP {
                        let trial: Vec<f64> =
                            theta.iter().zip(&delta).map(|(t, d)| t + nu * d).collect();
                        if let Some(pt) = self.predict(&trial) {
                            let weights_ok =
                                !self.weighting.uses_predictions() || pt.iter().all(|f| *f > 0.0);
                            let s_trial = self.wrss(&pt, &w);
                            if weights_ok && s_trial < s_cur {
                                accepted = Some((trial, pt, s_trial, nu, halvings));
                                break;
                            }
                        }
                        nu /= 2.0;
                        halvings += 1;
                    }
                    if accepted.is_some() {
                        break;
                    }
                    if lam == 0.0 && predicted.is_some_and(|q| q <= eps * s_cur) {
                        // Nothing measurable is left to gain: the current point is the minimum.
                        at_minimum = true;
                        break;
                    }
                }
                lam = if lam == 0.0 {
                    LAMBDA_START
                } else {
                    lam * LAMBDA_FACTOR
                };
                if lam > LAMBDA_MAX {
                    break;
                }
            }
            if at_minimum {
                status = FitStatus::Converged;
                break;
            }
            let Some((new_theta, new_pred, s_new, nu, halvings)) = accepted else {
                status = FitStatus::NoDecrease;
                break;
            };
            lambda = if lam <= LAMBDA_START {
                0.0
            } else {
                lam / LAMBDA_FACTOR
            };
            theta = new_theta;
            pred = new_pred;
            let decrease = s_cur - s_new;
            trace.push(TraceRow {
                iteration,
                wrss: wrss_own(&pred),
                estimates: theta.clone(),
                step: Some(nu),
                lambda: lam,
                halvings,
                relative_decrease: Some(decrease / s_new),
            });
            if self.options.criterion == Criterion::RelativeDecrease && decrease <= eps * s_new {
                status = FitStatus::Converged;
                break;
            }
        }
        result::build(self, theta, pred, trace, status)
    }
}

/// Damped direction: least squares of [A; √λ·I]·δ ≈ [r; 0] on the scaled columns, which is
/// Marquardt's (JᵀJ + λ·diag(JᵀJ))·δ = Jᵀr in the original scale (FIT-ALG-02, ALG-04).
fn damped(a: &Mat, r: &[f64], lambda: f64) -> Option<Vec<f64>> {
    let (m, n) = (a.rows(), a.cols());
    let mut aug = Mat::zeros(m + n, n);
    for i in 0..m {
        for j in 0..n {
            aug.set(i, j, a.at(i, j));
        }
    }
    for j in 0..n {
        aug.set(m + j, j, lambda.sqrt());
    }
    let mut rhs = r.to_vec();
    rhs.extend(std::iter::repeat_n(0.0, n));
    least_squares(&aug, &rhs).map(|(d, _)| d)
}
