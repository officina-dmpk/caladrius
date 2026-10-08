//! Options of an NCA run (`specs/nca.md` section 2.2). Every convention that changes a number is an
//! option here, never a constant in the computation (golden rule 5).

use serde::{Deserialize, Serialize};

/// Route of administration of the single dose given at time 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Route {
    /// Oral or any other extravascular route.
    Extravascular,
    /// Intravenous bolus.
    IvBolus,
    /// Intravenous infusion of the given duration (same time unit as the profile, > 0).
    IvInfusion {
        /// Duration of the infusion.
        #[serde(
            serialize_with = "crate::float::ser",
            deserialize_with = "crate::float::de"
        )]
        duration: f64,
    },
}

/// Rule used to integrate each segment between two consecutive points (NCA-AUC-05 to 07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AucMethod {
    /// Linear trapezoids everywhere (NCA-AUC-05).
    Linear,
    /// Linear while rising or flat, logarithmic while falling with both ends positive (NCA-AUC-06).
    #[default]
    LinUpLogDown,
    /// Linear up to Tmax, logarithmic afterwards whenever both ends are positive and differ
    /// (NCA-AUC-07).
    LinLog,
}

/// Options of the terminal-phase (λz) selection. Read by the λz step (task T-004b); validated here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LambdaZOptions {
    /// Fewest points in a candidate terminal phase (NCA-LZ-03). At least 2.
    pub min_points: usize,
    /// May the observed Cmax point be part of the terminal phase (NCA-LZ-02).
    pub allow_tmax: bool,
    /// Tolerance of the best-adjusted-R² rule (NCA-LZ-05). Finite and >= 0.
    pub adj_r_squared_factor: f64,
}

impl Default for LambdaZOptions {
    fn default() -> Self {
        Self {
            min_points: 3,
            allow_tmax: false,
            adj_r_squared_factor: 1e-4,
        }
    }
}

/// What to do with a missing (NaN) concentration (NCA-DAT-03).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingPolicy {
    /// Remove the point and its time.
    #[default]
    Drop,
    /// Replace the value by this number (finite, >= 0).
    Replace(f64),
}

/// What to do with a negative concentration (NCA-DAT-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegativePolicy {
    /// Stop with [`crate::NcaError::NegativeConcentration`].
    #[default]
    Error,
    /// Keep the value: it enters AUC and AUMC by the linear rule only, and is never Tlast or Clast.
    Allow,
    /// Replace it by 0; it then follows the BLQ policy.
    SetZero,
}

/// What to do with one class of BLQ (zero) points (NCA-DAT-06).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlqAction {
    /// Leave the zero in the profile.
    Keep,
    /// Remove the point and its time; its neighbours become adjacent.
    Drop,
    /// Replace the value by this number (finite, >= 0), for example half the LLOQ.
    Set(f64),
}

/// How BLQ (zero) points are classified, and the action for each class (NCA-DAT-06).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BlqPolicy {
    /// Classes by position relative to the quantifiable points (C > 0).
    Position {
        /// BLQ points before the first quantifiable point (all of them if none is quantifiable).
        first: BlqAction,
        /// BLQ points between the first and the last quantifiable point.
        middle: BlqAction,
        /// BLQ points after the last quantifiable point.
        last: BlqAction,
    },
    /// Classes by position relative to the time of the first maximum.
    Tmax {
        /// BLQ points before Tmax (all of them if no point is quantifiable).
        before: BlqAction,
        /// BLQ points after Tmax.
        after: BlqAction,
    },
}

impl BlqPolicy {
    /// The same action for every BLQ point.
    pub const fn all(action: BlqAction) -> Self {
        Self::Position {
            first: action,
            middle: action,
            last: action,
        }
    }
}

impl Default for BlqPolicy {
    /// PKNCA default: first keep, middle drop, last keep.
    fn default() -> Self {
        Self::Position {
            first: BlqAction::Keep,
            middle: BlqAction::Drop,
            last: BlqAction::Keep,
        }
    }
}

/// How the profile gets a concentration at the dose time when it has no sample there (NCA-DAT-08).
/// A sample at the dose time is always used as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPolicy {
    /// Insert nothing: AUC and AUMC are not calculated (reason `no_start_concentration`).
    None,
    /// Insert a zero at the dose time, whatever the route.
    Zero,
    /// Insert the route's C0 at the dose time: the estimate of NCA-IV-01 for an IV bolus, 0 for
    /// the other routes. For single-dose data this is the `auto` behaviour of NCA-DAT-08b.
    #[default]
    C0,
}

/// Which time is Tmax when the maximum occurs more than once (NCA-OBS-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TmaxTie {
    /// The first occurrence.
    #[default]
    First,
    /// The last occurrence.
    Last,
}

/// All options of an NCA run. `NcaOptions::default()` is the PKNCA profile of `specs/nca.md`
/// section 2.2, except `start` (see [`StartPolicy::C0`]), which the public oracle needs for IV bolus
/// data without a sample at time 0.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NcaOptions {
    /// Segment rule for AUC and AUMC.
    pub auc_method: AucMethod,
    /// Terminal-phase selection.
    pub lambda_z: LambdaZOptions,
    /// Missing (NaN) concentrations.
    pub missing: MissingPolicy,
    /// Negative concentrations.
    pub negative: NegativePolicy,
    /// BLQ (zero) concentrations.
    pub blq: BlqPolicy,
    /// Concentration at the dose time when there is no sample there.
    pub start: StartPolicy,
    /// Tmax when the maximum is not unique.
    pub tmax_tie: TmaxTie,
    /// How the terminal phase is chosen among the candidate point sets, or given by the user.
    pub lambda_z_selection: LambdaZSelection,
}

/// How the best candidate terminal phase is chosen (NCA-LZ-05, LZ-06; open item O-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LambdaZTieRule {
    /// Among the fits whose adjusted R² is within `adj_r_squared_factor` of the best, the one with
    /// the most points (NCA-LZ-05, the primary reading).
    #[default]
    Tolerance,
    /// The valid fit (λz > 0) with the largest adjusted R² + `adj_r_squared_factor` × number of
    /// points, ties to the most points (NCA-LZ-06).
    Bonus,
}

/// Terminal-phase points chosen by the user (NCA-LZ-08). Times are sample times of the input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LambdaZManual {
    /// Exactly these samples; each must have a quantified concentration (> 0, not BLQ).
    Times(Vec<f64>),
    /// Every quantified sample with `start <= t <= end`.
    Range {
        /// First time of the range, included.
        start: f64,
        /// Last time of the range, included.
        end: f64,
    },
}

/// Further options of the terminal phase. They live here, not in [`LambdaZOptions`], whose three
/// fields are fixed by the oracle tests.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LambdaZSelection {
    /// Tie rule of the automatic selection.
    pub tie_rule: LambdaZTieRule,
    /// Open item O-02 (NCA-LZ-07). `false` (PKNCA): the best adjusted R² is taken over every
    /// candidate, and fits with λz <= 0 are discarded afterwards, so λz can be not calculated even
    /// when a decreasing fit exists. `true`: only fits with λz > 0 compete. It changes the
    /// tolerance rule only: the bonus rule always scores the valid fits only (NCA-LZ-06).
    pub positive_filter_first: bool,
    /// Sample times left out of the automatic selection (NCA-LZ-09).
    pub exclude: Vec<f64>,
    /// User-chosen points; when set, no automatic selection happens (NCA-LZ-08).
    pub manual: Option<LambdaZManual>,
}
