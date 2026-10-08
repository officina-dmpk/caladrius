//! Hard errors: invalid input that stops the run before any computation (`specs/nca.md` section 2.4
//! and NCA-DAT-02, DAT-04, DAT-11). Everything else gives "not calculated" results with a reason.
//!
//! Point positions (`index`) are 0-based in the data; messages show them 1-based ("point 3").

use serde::{Deserialize, Serialize};
use std::fmt;

/// Why an NCA run was refused. The message says what to fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum NcaError {
    /// `time` and `conc` have different lengths.
    LengthMismatch {
        /// Number of times.
        times: usize,
        /// Number of concentrations.
        concs: usize,
    },
    /// The profile has no point at all.
    EmptyProfile,
    /// A time is NaN or infinite.
    NonFiniteTime {
        /// Position of the point.
        index: usize,
    },
    /// Two consecutive times are equal.
    DuplicateTime {
        /// Position of the second of the two points.
        index: usize,
        /// The repeated time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
    },
    /// A time is smaller than the one before it.
    UnsortedTime {
        /// Position of the point that comes too early.
        index: usize,
        /// Its time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
        /// The time of the point before it.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        previous: f64,
    },
    /// A concentration is infinite (a missing value is NaN, not infinite).
    InfiniteConcentration {
        /// Position of the point.
        index: usize,
        /// Its time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
    },
    /// A concentration is negative under [`crate::NegativePolicy::Error`].
    NegativeConcentration {
        /// Position of the point.
        index: usize,
        /// Its time.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        time: f64,
        /// The negative value.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
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
    /// An IV infusion without a finite duration > 0.
    InvalidInfusionDuration {
        /// The duration given.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        value: f64,
    },
    /// An option has a value outside its domain.
    InvalidOption {
        /// Name of the option, as in [`crate::NcaOptions`].
        option: String,
        /// What is wrong and what is allowed.
        reason: String,
    },
}

impl fmt::Display for NcaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { times, concs } => write!(
                f,
                "the profile has {times} times but {concs} concentrations; give exactly one concentration per time"
            ),
            Self::EmptyProfile => write!(
                f,
                "the profile is empty; give at least one time and concentration"
            ),
            Self::NonFiniteTime { index } => write!(
                f,
                "point {}: the time is missing or not a finite number; fill it in or remove the point",
                index + 1
            ),
            Self::DuplicateTime { index, time } => write!(
                f,
                "point {}: time {time} appears twice in a row; remove or merge the duplicated sample",
                index + 1
            ),
            Self::UnsortedTime {
                index,
                time,
                previous,
            } => write!(
                f,
                "point {}: time {time} comes after time {previous}; sort the profile by increasing time",
                index + 1
            ),
            Self::InfiniteConcentration { index, time } => write!(
                f,
                "point {} (time {time}): the concentration is infinite; correct it, or leave it empty to mark it missing",
                index + 1
            ),
            Self::NegativeConcentration { index, time, value } => write!(
                f,
                "point {} (time {time}): the concentration {value} is negative; correct it, mark it below the limit of quantification (0), or choose a negative-concentration policy (allow or set to zero)",
                index + 1
            ),
            Self::InvalidDose { value } => write!(
                f,
                "the dose {value} is not a positive number; give a dose greater than 0"
            ),
            Self::InvalidInfusionDuration { value } => write!(
                f,
                "the infusion duration {value} is not a positive number; give the duration of the infusion, greater than 0"
            ),
            Self::InvalidOption { option, reason } => {
                write!(f, "option `{option}`: {reason}")
            }
        }
    }
}

impl std::error::Error for NcaError {}
