//! The result of a fit (`specs/fit.md` section 7): status, estimates and their statistics by name,
//! diagnostics, predicted values, smooth curve and trace. A value that cannot be computed is
//! absent (`get` gives `None`), never NaN or infinite.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::FitStatus;
use crate::engine::Problem;
use crate::linalg::{Mat, inverse_gram_from_r, least_squares, symmetric_eigenvalues};
use crate::stats::{f_quantile, student_t_quantile};

/// One row of the minimisation trace (FIT-OUT-11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceRow {
    /// Iteration number, 0 for the initial estimates.
    pub iteration: usize,
    /// Weighted residual sum of squares at the estimates, with the weights of these estimates.
    pub wrss: f64,
    /// Estimates, in the parameter order of the result.
    pub estimates: Vec<f64>,
    /// Step factor ν used (None for iteration 0).
    pub step: Option<f64>,
    /// Damping λ used (0: pure Gauss-Newton).
    pub lambda: f64,
    /// Number of step halvings.
    pub halvings: usize,
    /// Relative decrease of the weighted sum of squares in this iteration.
    pub relative_decrease: Option<f64>,
}

/// One observation with its prediction (FIT-OUT-09).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ObservationRow {
    /// Time.
    pub time: f64,
    /// Observed concentration.
    pub observed: f64,
    /// Predicted concentration.
    pub predicted: f64,
    /// Observed − predicted.
    pub residual: f64,
    /// √w · residual.
    pub weighted_residual: f64,
    /// Weight at the solution.
    pub weight: f64,
}

/// One point of the smooth predicted curve (FIT-OUT-10).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    /// Time.
    pub time: f64,
    /// Predicted concentration.
    pub conc: f64,
}

/// Result of [`crate::run`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitResult {
    status: FitStatus,
    parameters: Vec<String>,
    values: BTreeMap<String, f64>,
    observations: Vec<ObservationRow>,
    curve: Vec<CurvePoint>,
    trace: Vec<TraceRow>,
}

impl FitResult {
    /// How the fit ended.
    pub fn status(&self) -> FitStatus {
        self.status
    }

    /// A result by name (see `tests/oracle_fit.rs` and FIT-VOC-01): `n`, `p`, `df`;
    /// `estimate.<par>`, `se.<par>`, `cv_percent.<par>`, `ci_lo.<par>`, `ci_hi.<par>`,
    /// `planar_lo.<par>`, `planar_hi.<par>`; `covariance.<a>.<b>`, `correlation.<a>.<b>`,
    /// `eigenvalue.<i>`, `condition_number`, `kappa_jacobian`; `wrss`, `s`, `ss_weighted`,
    /// `ss_corrected`, `corr_obs_pred`, `aic`, `sbc`; secondary `cl`, `half_life`, `auc_inf`
    /// with `estimate.`, `se.`, `cv_percent.`, `ci_lo.`, `ci_hi.`. `None`: not calculated.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied()
    }

    /// Every computed value, by name.
    pub fn values(&self) -> &BTreeMap<String, f64> {
        &self.values
    }

    /// Fitted parameters, in the order of the trace estimates.
    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    /// One row per iteration including iteration 0 (FIT-OUT-11).
    pub fn trace(&self) -> &[TraceRow] {
        &self.trace
    }

    /// Observed, predicted, residuals and weights (FIT-OUT-09).
    pub fn observations(&self) -> &[ObservationRow] {
        &self.observations
    }

    /// The smooth predicted curve (FIT-OUT-10).
    pub fn curve(&self) -> &[CurvePoint] {
        &self.curve
    }
}

/// Collects named values, keeping finite ones only.
struct Values(BTreeMap<String, f64>);

impl Values {
    fn put(&mut self, name: impl Into<String>, value: f64) {
        if value.is_finite() {
            self.0.insert(name.into(), value);
        }
    }
}

/// Statistics at the solution (FIT-OUT-01 to 07, 09, 10).
pub(crate) fn build(
    problem: &Problem<'_>,
    theta: Vec<f64>,
    pred: Vec<f64>,
    trace: Vec<TraceRow>,
    mut status: FitStatus,
) -> FitResult {
    let names = &problem.names;
    let (n, p) = (problem.y.len(), names.len());
    let df = n.saturating_sub(p);
    let mut v = Values(BTreeMap::new());
    v.put("n", n as f64);
    v.put("p", p as f64);
    v.put("df", df as f64);
    for (name, x) in names.iter().zip(&theta) {
        v.put(format!("estimate.{name}"), *x);
    }
    let weights = problem.weights(&pred).unwrap_or_else(|| vec![f64::NAN; n]);
    let wrss = problem.wrss(&pred, &weights);
    v.put("wrss", wrss);

    // FIT-OUT-07 diagnostics.
    let sum_w: f64 = weights.iter().sum();
    let mean_y = problem
        .y
        .iter()
        .zip(&weights)
        .map(|(y, w)| w * y)
        .sum::<f64>()
        / sum_w;
    let mean_f = pred.iter().zip(&weights).map(|(f, w)| w * f).sum::<f64>() / sum_w;
    v.put(
        "ss_weighted",
        problem.y.iter().zip(&weights).map(|(y, w)| w * y * y).sum(),
    );
    v.put(
        "ss_corrected",
        problem
            .y
            .iter()
            .zip(&weights)
            .map(|(y, w)| w * (y - mean_y).powi(2))
            .sum(),
    );
    let (mut syy, mut sff, mut syf) = (0.0, 0.0, 0.0);
    for ((y, f), w) in problem.y.iter().zip(&pred).zip(&weights) {
        syy += w * (y - mean_y).powi(2);
        sff += w * (f - mean_f).powi(2);
        syf += w * (y - mean_y) * (f - mean_f);
    }
    if syy > 0.0 && sff > 0.0 {
        v.put("corr_obs_pred", syf / (syy * sff).sqrt());
    }
    let s2 = (df > 0).then(|| wrss / df as f64);
    if let Some(s2) = s2 {
        v.put("s", s2.sqrt());
        if wrss > 0.0 {
            let nf = n as f64;
            v.put("aic", nf * wrss.ln() + 2.0 * p as f64);
            v.put("sbc", nf * wrss.ln() + p as f64 * nf.ln());
        }
    }

    // Jacobian at the solution, with the method of the fit (FIT-JAC-03).
    let system = problem
        .jacobian(&theta, &pred)
        .and_then(|columns| problem.scaled_system(&columns, &pred, &weights));
    let factor = system.and_then(|(a, norms, _)| {
        let zero = vec![0.0; a.rows()];
        least_squares(&a, &zero).map(|(_, r)| (r, norms))
    });
    match factor {
        None => {
            if status == FitStatus::Converged {
                status = FitStatus::Singular;
            }
        }
        Some((r, norms)) => {
            // FIT-OUT-05: κ of the Jacobian with unit-length columns.
            let scaled_gram = r_transpose_r(&r);
            let e = symmetric_eigenvalues(&scaled_gram);
            if let (Some(max), Some(min)) = (e.first(), e.last()) {
                if *min > 0.0 {
                    v.put("kappa_jacobian", (max / min).sqrt());
                }
            }
            if let (Some(s2), Some(inv)) = (s2, inverse_gram_from_r(&r)) {
                statistics(problem, &theta, s2, df, &inv, &norms, &mut v);
            }
        }
    }

    let observations = problem
        .time
        .iter()
        .zip(&problem.y)
        .zip(pred.iter().zip(&weights))
        .map(
            |((&time, &observed), (&predicted, &weight))| ObservationRow {
                time,
                observed,
                predicted,
                residual: observed - predicted,
                weighted_residual: weight.sqrt() * (observed - predicted),
                weight,
            },
        )
        .collect();
    FitResult {
        status,
        parameters: names.clone(),
        values: v.0,
        observations,
        curve: curve(problem, &theta),
        trace,
    }
}

fn r_transpose_r(r: &Mat) -> Mat {
    let n = r.cols();
    let mut g = Mat::zeros(n, n);
    for a in 0..n {
        for b in 0..n {
            g.set(a, b, (0..n).map(|k| r.at(k, a) * r.at(k, b)).sum());
        }
    }
    g
}

/// FIT-OUT-02 to 06 from (JᵀJ)⁻¹ of the scaled Jacobian.
fn statistics(
    problem: &Problem<'_>,
    theta: &[f64],
    s2: f64,
    df: usize,
    inv_scaled: &Mat,
    norms: &[f64],
    v: &mut Values,
) {
    let names = &problem.names;
    let p = names.len();
    let norm = |j: usize| norms.get(j).copied().unwrap_or(f64::NAN);
    let mut cov = Mat::zeros(p, p);
    for a in 0..p {
        for b in 0..p {
            cov.set(a, b, s2 * inv_scaled.at(a, b) / (norm(a) * norm(b)));
        }
    }
    let level = problem.options.confidence_level;
    let t = student_t_quantile((1.0 + level) / 2.0, df as f64);
    let planar = (p as f64 * f_quantile(level, p as f64, df as f64)).sqrt();
    let se: Vec<f64> = (0..p).map(|j| cov.at(j, j).sqrt()).collect();
    for (j, name) in names.iter().enumerate() {
        let (x, s) = (
            theta.get(j).copied().unwrap_or(f64::NAN),
            se.get(j).copied().unwrap_or(f64::NAN),
        );
        v.put(format!("se.{name}"), s);
        if x != 0.0 {
            v.put(format!("cv_percent.{name}"), 100.0 * s / x.abs());
        }
        v.put(format!("ci_lo.{name}"), x - t * s);
        v.put(format!("ci_hi.{name}"), x + t * s);
        v.put(format!("planar_lo.{name}"), x - planar * s);
        v.put(format!("planar_hi.{name}"), x + planar * s);
    }
    let mut corr = Mat::zeros(p, p);
    for (a, na) in names.iter().enumerate() {
        for (b, nb) in names.iter().enumerate() {
            if a <= b {
                v.put(format!("covariance.{na}.{nb}"), cov.at(a, b));
            }
            let c = cov.at(a, b)
                / (se.get(a).copied().unwrap_or(f64::NAN) * se.get(b).copied().unwrap_or(f64::NAN));
            corr.set(a, b, c);
            if a < b {
                v.put(format!("correlation.{na}.{nb}"), c);
            }
        }
    }
    let e = symmetric_eigenvalues(&corr);
    for (i, x) in e.iter().enumerate() {
        v.put(format!("eigenvalue.{}", i + 1), *x);
    }
    if let (Some(max), Some(min)) = (e.first(), e.last()) {
        if *min > 0.0 {
            v.put("condition_number", max / min);
        }
    }
    secondary(problem, theta, &cov, t, v);
}

/// FIT-OUT-06: CL = V·k, t½ = ln 2/k, AUC∞ = D/(V·k), standard errors by the delta method.
/// Needs `v` and `k` among the fitted parameters.
fn secondary(problem: &Problem<'_>, theta: &[f64], cov: &Mat, t: f64, v: &mut Values) {
    let names = &problem.names;
    let position = |name: &str| names.iter().position(|n| n == name);
    let (Some(iv), Some(ik)) = (position("v"), position("k")) else {
        return;
    };
    let (vol, k) = (
        theta.get(iv).copied().unwrap_or(f64::NAN),
        theta.get(ik).copied().unwrap_or(f64::NAN),
    );
    let dose = problem.dose;
    let ln2 = std::f64::consts::LN_2;
    let quantities = [
        ("cl", vol * k, k, vol),
        ("half_life", ln2 / k, 0.0, -ln2 / (k * k)),
        (
            "auc_inf",
            dose / (vol * k),
            -dose / (vol * vol * k),
            -dose / (vol * k * k),
        ),
    ];
    let p = names.len();
    for (name, value, d_v, d_k) in quantities {
        let mut grad = vec![0.0; p];
        if let Some(slot) = grad.get_mut(iv) {
            *slot = d_v;
        }
        if let Some(slot) = grad.get_mut(ik) {
            *slot = d_k;
        }
        let mut var = 0.0;
        for (a, ga) in grad.iter().enumerate() {
            for (b, gb) in grad.iter().enumerate() {
                var += ga * cov.at(a, b) * gb;
            }
        }
        let se = var.sqrt();
        v.put(format!("estimate.{name}"), value);
        v.put(format!("se.{name}"), se);
        if value != 0.0 {
            v.put(format!("cv_percent.{name}"), 100.0 * se / value.abs());
        }
        v.put(format!("ci_lo.{name}"), value - t * se);
        v.put(format!("ci_hi.{name}"), value + t * se);
    }
}

/// FIT-OUT-10: `n_curve` equally spaced times from the first to the last observation time.
fn curve(problem: &Problem<'_>, theta: &[f64]) -> Vec<CurvePoint> {
    let n = problem.options.n_curve;
    let lo = problem.time.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = problem
        .time
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if n == 0 || !(lo.is_finite() && hi.is_finite()) {
        return Vec::new();
    }
    let times: Vec<f64> = (0..n)
        .map(|i| {
            if n == 1 {
                lo
            } else {
                lo + (hi - lo) * i as f64 / (n - 1) as f64
            }
        })
        .collect();
    problem
        .predict_at(theta, &times)
        .map(|conc| {
            times
                .iter()
                .zip(conc)
                .map(|(&time, conc)| CurvePoint { time, conc })
                .collect()
        })
        .unwrap_or_default()
}
