//! Errors of a model evaluation (`specs/models.md` MOD-GEN-04, golden rule 6). Each one names the
//! parameter or value and says what to fix.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Why a model could not be evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum ModelError {
    /// A parameter name that no model knows.
    UnknownParameter {
        /// The name given.
        name: String,
        /// The model id.
        model: String,
    },
    /// A known parameter that this model does not have.
    ParameterNotInModel {
        /// The name given.
        name: String,
        /// The model id.
        model: String,
    },
    /// A parameter the model needs is missing.
    MissingParameter {
        /// The missing name (`cl or k` for the elimination parameter).
        name: String,
        /// The model id.
        model: String,
    },
    /// Both `cl` and `k` are given.
    ClearanceAndRateConstant,
    /// A parameter outside its domain.
    ParameterOutOfDomain {
        /// The parameter.
        name: String,
        /// The value given.
        value: f64,
        /// The domain, e.g. `a finite number > 0`.
        domain: String,
    },
    /// The dose is negative or not finite.
    InvalidDose {
        /// The dose given.
        value: f64,
    },
    /// A time is NaN or infinite.
    NonFiniteTime {
        /// Position in `times`.
        index: usize,
    },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParameter { name, model } => write!(
                f,
                "unknown parameter `{name}` for model {model}; use v, cl or k, and the input parameters of the model (ka, dur, tlag)"
            ),
            Self::ParameterNotInModel { name, model } => write!(
                f,
                "model {model} has no parameter `{name}`; remove it or choose a model that has it"
            ),
            Self::MissingParameter { name, model } => {
                write!(
                    f,
                    "model {model} needs the parameter `{name}`; give its value"
                )
            }
            Self::ClearanceAndRateConstant => write!(
                f,
                "both `cl` and `k` are given; give exactly one of them (k = cl / v)"
            ),
            Self::ParameterOutOfDomain {
                name,
                value,
                domain,
            } => write!(
                f,
                "parameter `{name}` = {value} is not {domain}; correct its value"
            ),
            Self::InvalidDose { value } => write!(
                f,
                "the dose {value} is not a finite number >= 0; give the effective dose (F x dose)"
            ),
            Self::NonFiniteTime { index } => write!(
                f,
                "time {} is missing or not a finite number; give finite times",
                index + 1
            ),
        }
    }
}

impl std::error::Error for ModelError {}
