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

mod arith;
mod closed_forms;
mod engine;
mod error;
mod flags;
mod float;
mod initial;
mod linalg;
mod ml;
mod ml_result;
mod model;
mod objective;
pub mod pop;
mod result;
mod stats;
mod stepper;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use caladrius_models::ModelId;
pub use error::FitError;
pub use flags::FitFlag;
pub use initial::initial_estimates;
pub use model::{FitModel, Secondary};
pub use objective::{ErrorModel, Information, Objective};
pub use result::{CurvePoint, FitResult, ObservationRow, TraceRow};
pub use stepper::{Control, FitRun, Observer, Outcome, Progress, Step};

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
    /// Closed forms when the model has them for the fitted parameters, otherwise forward
    /// differences with the relative `increment` (the default, FIT-JAC-01). The result says which
    /// was used ([`FitResult::derivatives`]).
    #[default]
    Auto,
    /// Forward differences with the relative `increment`.
    ForwardDifference,
    /// Closed forms, in the (v, k) parameterisation: pk1.iv_bolus, pk1.iv_infusion, pk1.oral_1,
    /// pk1.oral_1_lag, pk1.oral_0 and pk1.oral_0_lag, for any subset of fitted parameters, and
    /// every pk2 and pk3 model; refused (never replaced) when the model has none.
    Analytic,
}

/// A named set of the five iteration settings of [`FitOptions`]: `derivatives`, `increment`,
/// `criterion`, `convergence`, `max_iterations` (`specs/fit.md` FIT-JAC-01, FIT-CNV-01, CNV-02;
/// `specs/differences.md` D-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitPreset {
    /// Caladrius's defaults (Q-014): closed-form derivatives when the model has them, else forward
    /// differences of relative increment 1e-5; relative decrease of WRSS below 1e-10; 50
    /// iterations. On the 185 reference fits they reach the exact minimum within 1e-4 relative.
    #[default]
    Default,
    /// Optional preset: forward differences of relative increment 0.001, relative decrease
    /// below 0.0001, 50 iterations. Estimates then lie up to about 5e-3 relative from the
    /// exact minimum (D-03). Not a claim about another program.
    ReferenceConventions,
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

/// Options of a fit. The defaults are the preset [`FitPreset::Default`]; the optional preset
/// [`FitPreset::ReferenceConventions`] keeps the other iteration settings. In JSON, the key
/// `preset` chooses the starting values of the five iteration settings and any of them given
/// explicitly replaces the preset's value; the options are then stored as plain values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "OptionsInput")]
pub struct FitOptions {
    /// What is minimised: `wls` (the default) or `ml` (FIT-ML-01). The four options below it are
    /// not written while they hold their defaults, so a least-squares option set serialises as it
    /// always did.
    #[serde(skip_serializing_if = "is_wls")]
    pub objective: Objective,
    /// The residual error model of `ml` (FIT-ML-02); required with `ml`, refused with `wls`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_model: Option<ErrorModel>,
    /// Initial error parameters a, b in the user's units, replacing the starting values of
    /// FIT-ML-10 (`ml` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_init: Option<Vec<f64>>,
    /// Where the covariance of `ml` comes from (FIT-ML-17).
    #[serde(skip_serializing_if = "is_expected")]
    pub information: Information,
    /// Partial derivatives (`auto`).
    pub derivatives: Derivatives,
    /// Relative increment of the forward differences (1e-5), > 0 and <= 0.1.
    pub increment: f64,
    /// Convergence criterion (relative decrease).
    pub criterion: Criterion,
    /// Threshold of the criterion (1e-10), >= 0 and <= 0.1.
    pub convergence: f64,
    /// Maximum number of accepted iterations (50), at most 100 000.
    pub max_iterations: usize,
    /// Level of the confidence intervals (0.95), strictly between 0 and 1.
    pub confidence_level: f64,
    /// Points of the smooth predicted curve (1000): 0 (no curve) or 2 to 1 000 000.
    pub n_curve: usize,
    /// Parameters held at a given value, by name (not fitted; e.g. the infusion duration `dur`).
    pub fixed: BTreeMap<String, f64>,
    /// User bounds by fitted parameter (FIT-BND-01). Without them the generated bounds of
    /// FIT-BND-03 apply: `v`, `cl`, `k`, `ka`, `dur` above 1e-6 times their initial estimate,
    /// `tlag` >= 0, no upper bound.
    pub bounds: BTreeMap<String, Bounds>,
    /// Thresholds of the quality flags (FIT-FLG-01).
    pub flags: FlagThresholds,
}

/// Lower and upper bound of one parameter; `None` is unbounded on that side.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bounds {
    /// Lower bound (inclusive).
    pub lower: Option<f64>,
    /// Upper bound (inclusive).
    pub upper: Option<f64>,
}

/// Thresholds of the quality flags (FIT-FLG-01, status `assumed`); `None` turns a flag off.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FlagThresholds {
    /// Flag a CV% above this value (50).
    pub max_cv_percent: Option<f64>,
    /// Flag an absolute correlation between two estimates above this value (0.95).
    pub max_abs_correlation: Option<f64>,
    /// Flag a condition number of the correlation matrix above this value (1e6).
    pub max_condition_number: Option<f64>,
    /// Flag fewer degrees of freedom than this (2).
    pub min_degrees_of_freedom: Option<usize>,
}

impl Default for FlagThresholds {
    fn default() -> Self {
        Self {
            max_cv_percent: Some(50.0),
            max_abs_correlation: Some(0.95),
            max_condition_number: Some(1e6),
            min_degrees_of_freedom: Some(2),
        }
    }
}

impl Default for FitOptions {
    fn default() -> Self {
        Self::preset(FitPreset::Default)
    }
}

impl FitOptions {
    /// The options of a preset; everything outside the five iteration settings at its default.
    /// The values of both presets are written here and nowhere else (golden rule 5).
    pub fn preset(preset: FitPreset) -> Self {
        let (derivatives, increment, convergence) = match preset {
            FitPreset::Default => (Derivatives::Auto, 1e-5, 1e-10),
            FitPreset::ReferenceConventions => (Derivatives::ForwardDifference, 0.001, 0.0001),
        };
        Self {
            objective: Objective::Wls,
            error_model: None,
            error_init: None,
            information: Information::Expected,
            derivatives,
            increment,
            criterion: Criterion::RelativeDecrease,
            convergence,
            max_iterations: 50,
            confidence_level: 0.95,
            n_curve: 1000,
            fixed: BTreeMap::new(),
            bounds: BTreeMap::new(),
            flags: FlagThresholds::default(),
        }
    }

    /// The preset [`FitPreset::ReferenceConventions`] (the defaults before Q-014).
    pub fn reference_conventions() -> Self {
        Self::preset(FitPreset::ReferenceConventions)
    }

    /// The defaults of `objective = ml` with `error_model`: the iteration limit is 500 (FIT-ML-08).
    pub fn maximum_likelihood(error_model: ErrorModel) -> Self {
        Self {
            objective: Objective::Ml,
            error_model: Some(error_model),
            max_iterations: ML_MAX_ITERATIONS,
            ..Self::default()
        }
    }
}

/// Default iteration limit of `objective = ml` (FIT-ML-08).
const ML_MAX_ITERATIONS: usize = 500;

fn is_wls(objective: &Objective) -> bool {
    *objective == Objective::Wls
}

fn is_expected(information: &Information) -> bool {
    *information == Information::Expected
}

/// [`FitOptions`] as read from JSON: a preset, then the values given explicitly.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct OptionsInput {
    preset: FitPreset,
    // Read as text, so that a wrong word is refused with a message that names the option.
    objective: Option<String>,
    error_model: Option<String>,
    error_init: Option<Vec<f64>>,
    information: Option<String>,
    derivatives: Option<Derivatives>,
    increment: Option<f64>,
    criterion: Option<Criterion>,
    convergence: Option<f64>,
    max_iterations: Option<usize>,
    confidence_level: Option<f64>,
    n_curve: Option<usize>,
    fixed: BTreeMap<String, f64>,
    bounds: BTreeMap<String, Bounds>,
    flags: FlagThresholds,
}

impl TryFrom<OptionsInput> for FitOptions {
    type Error = String;

    fn try_from(input: OptionsInput) -> Result<Self, String> {
        let base = FitOptions::preset(input.preset);
        let objective = match input.objective.as_deref() {
            None => Objective::Wls,
            Some(id) => Objective::from_id(id).ok_or_else(|| {
                format!(
                    "option `objective`: unknown objective `{id}`; use `wls` (least squares, the default) or `ml` (maximum likelihood)"
                )
            })?,
        };
        let error_model = match input.error_model.as_deref() {
            None => None,
            Some(id) => Some(ErrorModel::from_id(id).ok_or_else(|| {
                format!(
                    "option `error_model`: unknown error model `{id}`; use `additive`, `proportional`, `combined` or `combined_sd`"
                )
            })?),
        };
        let information = match input.information.as_deref() {
            None => Information::Expected,
            Some(id) => Information::from_id(id).ok_or_else(|| {
                format!(
                    "option `information`: unknown information `{id}`; only `expected` is available"
                )
            })?,
        };
        // FIT-ML-08: the limit of a maximum-likelihood fit is 500 unless one is given.
        let default_limit = if objective == Objective::Ml {
            ML_MAX_ITERATIONS
        } else {
            base.max_iterations
        };
        Ok(Self {
            objective,
            error_model,
            error_init: input.error_init,
            information,
            derivatives: input.derivatives.unwrap_or(base.derivatives),
            increment: input.increment.unwrap_or(base.increment),
            criterion: input.criterion.unwrap_or(base.criterion),
            convergence: input.convergence.unwrap_or(base.convergence),
            max_iterations: input.max_iterations.unwrap_or(default_limit),
            confidence_level: input.confidence_level.unwrap_or(base.confidence_level),
            n_curve: input.n_curve.unwrap_or(base.n_curve),
            fixed: input.fixed,
            bounds: input.bounds,
            flags: input.flags,
        })
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
    /// Converged with a parameter on a bound (FIT-BND-02): a model parameter at its bound, or,
    /// with `objective = ml`, an error component at 0 (FIT-ML-15).
    AtBound,
    /// `objective = ml` only: during the iteration the error of a one-parameter error model fell to
    /// rounding level, so the likelihood has no maximum (FIT-ML-15). The result is the last
    /// iterate before that; [`run`] and [`run_model`] turn this status into
    /// [`FitError::ExactFit`].
    ExactFit,
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
            FitStatus::ExactFit => {
                "the model reproduces the observations exactly (the residual sum of squares is 0 within rounding): the maximum-likelihood estimate of the error does not exist; add observations, remove a parameter, or add noise to simulated data"
            }
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
    /// Empty: generated from the data by [`initial_estimates`] (FIT-INI-01).
    pub initial: BTreeMap<String, f64>,
    /// Options.
    #[serde(default)]
    pub options: FitOptions,
}

/// Starts the fit of `input.model`: the checks of [`run`], and the initial estimates of FIT-INI-01
/// when `input.initial` is empty, then a [`FitRun`] at iteration 0 that owns its model. Drive it
/// with [`FitRun::step`] or [`FitRun::run`]; [`run`] is this and nobody watching.
pub fn start(input: &FitInput) -> Result<FitRun<ModelId>, FitError> {
    if input.initial.is_empty() {
        // FIT-INI-01: no initial estimates given, generate them from the data.
        let initial = initial_estimates(
            input.model,
            input.dose,
            &input.time,
            &input.conc,
            &input.options.fixed,
        )?;
        let generated = FitInput {
            initial,
            ..input.clone()
        };
        return FitRun::new(generated.model, &generated);
    }
    FitRun::new(input.model, input)
}

/// Fits `input.model` to the observations (Gauss-Newton with the Levenberg and Hartley
/// modification, FIT-ALG-01). Invalid input is an error; a fit that starts always returns a
/// result with a status, its best iterate and its trace.
pub fn run(input: &FitInput) -> Result<FitResult, FitError> {
    refuse_exact_fit(start(input)?.solve())
}

/// As [`run`], with any [`FitModel`] in place of `input.model`.
pub fn run_model(model: &dyn FitModel, input: &FitInput) -> Result<FitResult, FitError> {
    refuse_exact_fit(FitRun::new(model, input)?.solve())
}

/// FIT-ML-15: an exact fit found during the iteration is a refusal for the one-shot entry points; a
/// host that drives the steps sees the status [`FitStatus::ExactFit`] on the result instead
/// (a [`Step`] has no error to carry, and adding a variant would break its exhaustive matches).
fn refuse_exact_fit(result: FitResult) -> Result<FitResult, FitError> {
    if result.status() == FitStatus::ExactFit {
        Err(FitError::ExactFit)
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ml;
#[cfg(test)]
mod tests_outputs;
#[cfg(test)]
mod tests_pinned;
#[cfg(test)]
mod tests_presets;
#[cfg(test)]
mod tests_progress;
#[cfg(test)]
mod tests_stopping;
