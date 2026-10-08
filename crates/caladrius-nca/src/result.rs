//! The result of an NCA run: named parameters, each a value or "not calculated" with a reason
//! (NCA-OUT-02), and the profile actually integrated.

use serde::{Deserialize, Serialize};

use crate::clean::{ProfilePoint, RemovedPoint};
use crate::lambda_z::LambdaZCandidate;

/// Why a parameter has no value (NCA-OUT-02). Never NaN, infinity or a stand-in zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NcReason {
    /// No point is left after cleaning.
    NoDataAfterCleaning,
    /// Only one point is left: there is no segment to integrate.
    SinglePoint,
    /// No concentration at the dose time and the start policy inserts none (NCA-DAT-08).
    NoStartConcentration,
    /// No concentration is quantifiable (> 0).
    NoPositiveConcentration,
    /// The parameter does not exist for this route (for example C0 outside IV bolus).
    NotApplicableToRoute,
    /// Fewer eligible points than the terminal phase needs (NCA-LZ-03, LZ-08).
    TooFewPoints,
    /// No candidate terminal phase decreases: every admissible fit has λz <= 0 (NCA-LZ-04 to 07).
    NoValidFit,
    /// A ratio whose area is zero or negative (NCA-EXT-02, EXT-08).
    NonPositiveArea,
    /// The computation overflowed or was undefined.
    NonFinite,
}

impl std::fmt::Display for NcReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoDataAfterCleaning => "no point is left after cleaning; check for missing values and the BLQ policy",
            Self::SinglePoint => "only one sample is left; an area needs at least two",
            Self::NoStartConcentration => "there is no concentration at the dose time; add a sample at time 0 or choose a start policy",
            Self::NoPositiveConcentration => "no concentration is above zero",
            Self::NotApplicableToRoute => "not defined for this route of administration",
            Self::NonFinite => "the computation overflowed; check the magnitude of times and concentrations",
            Self::TooFewPoints => "too few positive points after Tmax for the terminal phase; lower the minimum number of points, allow the Tmax point, or choose the points manually",
            Self::NoValidFit => "no terminal phase decreases (the fitted slope is not negative); choose other points or report the terminal phase as not estimable",
            Self::NonPositiveArea => "the area is zero or negative, so the ratio is undefined",
        })
    }
}

/// A parameter value, or the reason why it has none.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamValue {
    /// A finite number.
    Value(f64),
    /// Not calculated.
    NotCalculated(NcReason),
}

impl ParamValue {
    /// `Value(x)` when `x` is finite, otherwise not calculated with [`NcReason::NonFinite`].
    pub fn of(x: f64) -> Self {
        if x.is_finite() {
            Self::Value(x)
        } else {
            Self::NotCalculated(NcReason::NonFinite)
        }
    }

    /// Not calculated for `reason`.
    pub fn nc(reason: NcReason) -> Self {
        Self::NotCalculated(reason)
    }

    /// The reason, if not calculated.
    pub fn reason(&self) -> Option<NcReason> {
        match self {
            Self::Value(_) => None,
            Self::NotCalculated(reason) => Some(*reason),
        }
    }

    /// `f(a, b)` when both are values, otherwise the first reason.
    pub(crate) fn zip(self, other: Self, f: impl FnOnce(f64, f64) -> Self) -> Self {
        match (self, other) {
            (Self::Value(a), Self::Value(b)) => f(a, b),
            (Self::NotCalculated(r), _) | (_, Self::NotCalculated(r)) => Self::NotCalculated(r),
        }
    }

    /// The number, if any.
    pub fn value(&self) -> Option<f64> {
        match self {
            Self::Value(x) => Some(*x),
            Self::NotCalculated(_) => None,
        }
    }
}

/// One named parameter. Names are the PKNCA names (`cmax`, `clast.obs`, `auclast`...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// PKNCA name.
    pub name: String,
    /// Value or reason.
    pub value: ParamValue,
}

/// Result of [`crate::run`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NcaResult {
    parameters: Vec<Parameter>,
    profile: Vec<ProfilePoint>,
    removed: Vec<RemovedPoint>,
    lambda_z_candidates: Vec<LambdaZCandidate>,
}

impl NcaResult {
    pub(crate) fn new(
        parameters: Vec<Parameter>,
        profile: Vec<ProfilePoint>,
        removed: Vec<RemovedPoint>,
        lambda_z_candidates: Vec<LambdaZCandidate>,
    ) -> Self {
        Self {
            parameters,
            profile,
            removed,
            lambda_z_candidates,
        }
    }

    /// The value of the parameter called `name` (PKNCA spelling); `None` when it is not calculated
    /// or not computed by this version.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.parameter(name).and_then(|v| v.value())
    }

    /// The value or the reason for the parameter called `name`; `None` when it is not computed by
    /// this version.
    pub fn parameter(&self, name: &str) -> Option<ParamValue> {
        self.parameters
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value)
    }

    /// Every parameter computed, in a stable order.
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// The profile actually integrated (after cleaning, with any point inserted at the dose time).
    pub fn profile(&self) -> &[ProfilePoint] {
        &self.profile
    }

    /// The input points left out, with the reason.
    pub fn removed(&self) -> &[RemovedPoint] {
        &self.removed
    }

    /// Every candidate terminal phase of the automatic selection (or the manual fit), with the
    /// selected one marked (NCA-OUT-01).
    pub fn lambda_z_candidates(&self) -> &[LambdaZCandidate] {
        &self.lambda_z_candidates
    }
}
