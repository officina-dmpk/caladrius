#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Layer L0: weighted least-squares fitting of pharmacokinetic models, with diagnostics.
//!
//! Behaviour: `specs/fit.md` (rule ids `FIT-…` are cited in the code and the tests). One input
//! value, one [`run`], results by name, like `caladrius-nca` and `caladrius-models`.

mod engine;
mod error;
mod float;
mod linalg;
mod model;
mod result;
mod stats;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use caladrius_models::ModelId;
pub use error::FitError;
pub use model::{FitModel, Secondary};
pub use result::{CurvePoint, FitResult, ObservationRow, TraceRow};

/// Weighting scheme (FIT-WGT-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weighting {
    /// w = 1 (the default).
    #[default]
    Uniform,
    /// w = 1/y.
    InvY,
    /// w = 1/y².
    InvY2,
    /// w = 1/ŷ, recomputed at every iteration (FIT-WGT-03).
    InvYhat,
    /// w = 1/ŷ², recomputed at every iteration (FIT-WGT-03).
    InvYhat2,
}

impl Weighting {
    const ALL: [Weighting; 5] = [
        Weighting::Uniform,
        Weighting::InvY,
        Weighting::InvY2,
        Weighting::InvYhat,
        Weighting::InvYhat2,
    ];

    /// The id of FIT-VOC-01: `uniform`, `inv_y`, `inv_y2`, `inv_yhat`, `inv_yhat2`.
    pub fn id(&self) -> &'static str {
        match self {
            Weighting::Uniform => "uniform",
            Weighting::InvY => "inv_y",
            Weighting::InvY2 => "inv_y2",
            Weighting::InvYhat => "inv_yhat",
            Weighting::InvYhat2 => "inv_yhat2",
        }
    }

    /// The scheme with this id.
    pub fn from_id(id: &str) -> Option<Weighting> {
        Self::ALL.into_iter().find(|w| w.id() == id)
    }

    /// True when the weights come from the predictions.
    pub fn uses_predictions(&self) -> bool {
        matches!(self, Weighting::InvYhat | Weighting::InvYhat2)
    }
}

/// How the partial derivatives are formed (FIT-JAC-01, JAC-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Derivatives {
    /// Forward differences with the relative `increment` (the default).
    #[default]
    ForwardDifference,
    /// Closed forms (pk1.iv_bolus with v, k; pk1.oral_1 with v, k, ka).
    Analytic,
}

/// Convergence criterion (FIT-CNV-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criterion {
    /// (WRSS before − WRSS after) <= convergence · WRSS after, for an accepted iteration.
    #[default]
    RelativeDecrease,
    /// The relative offset of the residual (component in the tangent plane over the orthogonal
    /// part, each per degree of freedom) <= convergence.
    RelativeOffset,
}

/// Options of a fit. The defaults are those of `AGENTS.md` section 6 (status `assumed`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FitOptions {
    /// Partial derivatives.
    pub derivatives: Derivatives,
    /// Relative increment of the forward differences (0.001), > 0 and <= 0.1.
    pub increment: f64,
    /// Convergence criterion.
    pub criterion: Criterion,
    /// Threshold of the criterion (0.0001), >= 0 and <= 0.1.
    pub convergence: f64,
    /// Maximum number of accepted iterations (50), at most 100 000.
    pub max_iterations: usize,
    /// Level of the confidence intervals (0.95), strictly between 0 and 1.
    pub confidence_level: f64,
    /// Points of the smooth predicted curve (1000): 0 (no curve) or 2 to 1 000 000.
    pub n_curve: usize,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            derivatives: Derivatives::ForwardDifference,
            increment: 0.001,
            criterion: Criterion::RelativeDecrease,
            convergence: 0.0001,
            max_iterations: 50,
            confidence_level: 0.95,
            n_curve: 1000,
        }
    }
}

/// End of a fit (FIT-CNV-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitStatus {
    /// The criterion is met.
    Converged,
    /// The iteration limit was reached first.
    MaxIterations,
    /// No step decreases the weighted sum of squares, even with the largest damping.
    NoDecrease,
    /// The Jacobian is singular at the best iterate: a parameter combination is not identifiable.
    Singular,
    /// The model gave non-finite values during the iterations.
    NonFinite,
    /// Converged with a parameter on a bound (bounds are not implemented yet; never reported).
    AtBound,
}

impl FitStatus {
    /// What the status means and what to change.
    pub fn message(&self) -> &'static str {
        match self {
            FitStatus::Converged => "converged",
            FitStatus::MaxIterations => {
                "not converged: the iteration limit was reached; raise it, or start from other initial estimates"
            }
            FitStatus::NoDecrease => {
                "not converged: no step decreases the weighted sum of squares; try other initial estimates or another weighting"
            }
            FitStatus::Singular => {
                "the parameters are not all identifiable from these data (singular Jacobian); fix a parameter or use a simpler model"
            }
            FitStatus::NonFinite => {
                "not converged: the model gave non-finite values; check the initial estimates and the units"
            }
            FitStatus::AtBound => "converged with a parameter on a bound",
        }
    }
}

/// One fit: model, dose, observations, weighting, initial estimates and options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FitInput {
    /// The model (`caladrius-models`).
    pub model: ModelId,
    /// Effective dose, finite and > 0.
    pub dose: f64,
    /// Observation times since the dose (finite, >= 0).
    pub time: Vec<f64>,
    /// Observed concentrations, finite.
    pub conc: Vec<f64>,
    /// Weighting scheme.
    pub weighting: Weighting,
    /// Initial estimates by parameter name; the fitted parameters are exactly the ones named.
    pub initial: BTreeMap<String, f64>,
    /// Options.
    #[serde(default)]
    pub options: FitOptions,
}

/// Fits `input.model` to the observations (Gauss-Newton with the Levenberg and Hartley
/// modification, FIT-ALG-01). Invalid input is an error; a fit that starts always returns a
/// result with a status, its best iterate and its trace.
pub fn run(input: &FitInput) -> Result<FitResult, FitError> {
    run_model(&input.model, input)
}

/// As [`run`], with any [`FitModel`] in place of `input.model`.
pub fn run_model(model: &dyn FitModel, input: &FitInput) -> Result<FitResult, FitError> {
    let problem = engine::Problem::new(model, input)?;
    Ok(problem.solve())
}

#[cfg(test)]
mod tests;
