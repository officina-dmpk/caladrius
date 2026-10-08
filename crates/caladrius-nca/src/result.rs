//! The result of an NCA run: named parameters, each a value or "not calculated" with a reason
//! (NCA-OUT-02), and the profile actually integrated.

use serde::{Deserialize, Serialize};

use crate::clean::{ProfilePoint, RemovedPoint};

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
    /// The computation overflowed or was undefined.
    NonFinite,
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
}

impl NcaResult {
    pub(crate) fn new(
        parameters: Vec<Parameter>,
        profile: Vec<ProfilePoint>,
        removed: Vec<RemovedPoint>,
    ) -> Self {
        Self {
            parameters,
            profile,
            removed,
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
}
