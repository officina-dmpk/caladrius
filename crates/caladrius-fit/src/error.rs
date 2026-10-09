//! Errors that stop a fit before it starts (`specs/fit.md` FIT-ERR-01, WGT-02, golden rule 6).
//! A fit that starts always ends with a status (FIT-CNV-03), never with an error.

use serde::{Deserialize, Serialize};
use std::fmt;

use caladrius_models::ModelError;

/// Why a fit could not start. The message says what to fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum FitError {
    /// `time` and `conc` differ in length.
    LengthMismatch {
        /// Number of times.
        times: usize,
        /// Number of concentrations.
        concs: usize,
    },
    /// No parameter to fit.
    NoParameters,
    /// Fewer observations than fitted parameters.
    TooFewObservations {
        /// Observations.
        n: usize,
        /// Fitted parameters.
        p: usize,
    },
    /// A time or a concentration is NaN or infinite.
    NonFiniteObservation {
        /// Position (0-based; messages show it 1-based).
        index: usize,
        /// `time` or `concentration`.
        field: String,
    },
    /// An observation before the dose.
    TimeBeforeDose {
        /// Position.
        index: usize,
        /// Its time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
    },
    /// The dose is not a finite number > 0.
    InvalidDose {
        /// The dose given.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
    },
    /// The model refuses the initial estimates (unknown, missing or out-of-domain parameter).
    InitialEstimates {
        /// The model's error, which names the parameter.
        source: ModelError,
    },
    /// A weight 1/y or 1/y² needs y > 0 (FIT-WGT-02).
    UnweightableObservation {
        /// Position.
        index: usize,
        /// Its time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
        /// The concentration.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// The weighting id.
        weighting: String,
    },
    /// At the initial estimates a prediction is not usable (not finite, or not > 0 for weights on
    /// the predicted values).
    UnusableInitialPrediction {
        /// Position.
        index: usize,
        /// Its time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
    },
    /// An initial estimate outside its bounds (FIT-BND-01).
    InitialOutsideBounds {
        /// The parameter.
        parameter: String,
        /// Its initial estimate.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
        /// Lower bound.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        lower: f64,
        /// Upper bound.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        upper: f64,
    },
    /// Initial estimates cannot be generated from the data (FIT-ERR-03).
    InitialEstimatesUnavailable {
        /// Why.
        reason: String,
    },
    /// Closed-form derivatives are not available for this model and parameterisation.
    AnalyticDerivativesUnavailable {
        /// The model id.
        model: String,
    },
    /// Closed-form derivatives were asked for a model with a dosing regimen among its parameters
    /// (`tau`, `n_doses`, a schedule), for which none is derived yet (card T-049).
    RegimenDerivativesUnavailable {
        /// The model id.
        model: String,
    },
    /// An option outside its domain.
    InvalidOption {
        /// Option name.
        option: String,
        /// What is allowed.
        reason: String,
    },
}

impl fmt::Display for FitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { times, concs } => write!(
                f,
                "{times} times but {concs} concentrations; give one concentration per time"
            ),
            Self::NoParameters => write!(
                f,
                "no parameter to fit; give an initial estimate for each parameter of the model"
            ),
            Self::TooFewObservations { n, p } => write!(
                f,
                "{n} observations for {p} parameters; a fit needs at least as many observations as parameters (fix some parameters or add data)"
            ),
            Self::NonFiniteObservation { index, field } => write!(
                f,
                "observation {}: the {field} is missing or not a finite number; correct it or leave the observation out",
                index + 1
            ),
            Self::TimeBeforeDose { index, time } => write!(
                f,
                "observation {} is at time {time}, before the dose; leave it out of the fit",
                index + 1
            ),
            Self::InvalidDose { value } => write!(
                f,
                "the dose {value} is not a positive number; give a dose greater than 0"
            ),
            Self::InitialEstimates { source } => write!(f, "initial estimates: {source}"),
            Self::UnweightableObservation {
                index,
                time,
                value,
                weighting,
            } => write!(
                f,
                "observation {} (time {time}) has concentration {value}, which has no weight under {weighting} (zero, negative, or too small to weight); leave it out of the fit or choose another weighting",
                index + 1
            ),
            Self::UnusableInitialPrediction { index, time } => write!(
                f,
                "at the initial estimates the prediction for observation {} (time {time}) is not usable (not finite, or not positive for weights on predicted values); change the initial estimates",
                index + 1
            ),
            Self::InitialOutsideBounds {
                parameter,
                value,
                lower,
                upper,
            } => write!(
                f,
                "the initial estimate of `{parameter}` ({value}) lies outside its bounds [{lower}, {upper}]; move it inside or widen the bounds"
            ),
            Self::InitialEstimatesUnavailable { reason } => write!(
                f,
                "initial estimates cannot be generated: {reason}; enter initial estimates for the parameters"
            ),
            Self::AnalyticDerivativesUnavailable { model } if model.starts_with("pk2.") => write!(
                f,
                "closed-form derivatives of {model} are not available for these parameters (a fitted name is not a parameter of the model, or the model refuses the values); give one complete set (cl, vc, q, vp; k10, k12, k21, vc; or a, b, alpha, beta) with the input parameters of the model, or use forward differences"
            ),
            Self::AnalyticDerivativesUnavailable { model } => write!(
                f,
                "closed-form derivatives are available in the (v, k) parameterisation only (v, k and the input parameters of the model), not for {model} with these parameters; use forward differences or give `k` instead of `cl`"
            ),
            Self::RegimenDerivativesUnavailable { model } => write!(
                f,
                "closed-form derivatives of {model} are unavailable for a dosing regimen (`tau`, `n_doses` or a schedule among the parameters); use forward differences or `auto`"
            ),
            Self::InvalidOption { option, reason } => write!(f, "option `{option}`: {reason}"),
        }
    }
}

impl std::error::Error for FitError {}
