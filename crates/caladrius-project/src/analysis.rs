//! Analysis objects: the options of an NCA, a model fit or a simulation, and their last result.

use std::collections::BTreeMap;

use caladrius_fit::{FitOptions, FitResult, Weighting};
use caladrius_models::{ModelId, ModelInput, ModelOutput};
use caladrius_nca::{NcaOptions, NcaResult, Route};
use serde::{Deserialize, Serialize};

use crate::worksheet::WorksheetId;

/// Stable identifier of an analysis inside a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnalysisId(pub u64);

impl std::fmt::Display for AnalysisId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Non-compartmental analysis of the profiles of a worksheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NcaSpec {
    /// The worksheet read.
    pub worksheet: WorksheetId,
    /// One subject; `None` analyses every subject of the worksheet.
    #[serde(default)]
    pub subject: Option<String>,
    /// Route of administration; `None` reads the route column of the worksheet.
    #[serde(default)]
    pub route: Option<Route>,
    /// Dose; `None` reads the dose column of the worksheet (a missing dose only makes the
    /// dose-dependent parameters not calculated).
    #[serde(default)]
    pub dose: Option<f64>,
    /// Conventions of the analysis.
    #[serde(default)]
    pub options: NcaOptions,
}

/// Fit of a one-compartment model to the profile of one subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FitSpec {
    /// The worksheet read.
    pub worksheet: WorksheetId,
    /// The subject; `None` when the worksheet has a single subject.
    #[serde(default)]
    pub subject: Option<String>,
    /// The model.
    pub model: ModelId,
    /// Effective dose; `None` reads the dose column of the worksheet.
    #[serde(default)]
    pub dose: Option<f64>,
    /// Weighting scheme.
    #[serde(default)]
    pub weighting: Weighting,
    /// Initial estimates by parameter name; empty generates them from the data.
    #[serde(default)]
    pub initial: BTreeMap<String, f64>,
    /// Options of the fit.
    #[serde(default)]
    pub options: FitOptions,
}

/// Evaluation of a model on a time grid (no worksheet).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationSpec {
    /// Model, dose, parameters and times.
    pub input: ModelInput,
}

/// What an analysis does, with its options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalysisSpec {
    /// Non-compartmental analysis.
    Nca(NcaSpec),
    /// Model fit.
    Fit(FitSpec),
    /// Model simulation.
    Simulation(SimulationSpec),
}

impl AnalysisSpec {
    /// The worksheet the analysis reads, if any.
    pub fn worksheet(&self) -> Option<WorksheetId> {
        match self {
            AnalysisSpec::Nca(s) => Some(s.worksheet),
            AnalysisSpec::Fit(s) => Some(s.worksheet),
            AnalysisSpec::Simulation(_) => None,
        }
    }

    /// `nca`, `fit` or `simulation`.
    pub fn kind(&self) -> &'static str {
        match self {
            AnalysisSpec::Nca(_) => "nca",
            AnalysisSpec::Fit(_) => "fit",
            AnalysisSpec::Simulation(_) => "simulation",
        }
    }
}

/// A value, or the message of the error that prevented it. Errors are results too: a subject that
/// could not be analysed is shown with its reason, not dropped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> {
    /// It worked.
    Ok(T),
    /// It did not; the message says what to fix.
    Error(String),
}

impl<T> Outcome<T> {
    /// The value, if any.
    pub fn ok(&self) -> Option<&T> {
        match self {
            Outcome::Ok(v) => Some(v),
            Outcome::Error(_) => None,
        }
    }
}

/// The NCA of one subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NcaSubjectResult {
    /// Subject label.
    pub subject: String,
    /// The dose used, if any.
    pub dose: Option<f64>,
    /// The route used.
    pub route: Option<Route>,
    /// The result, or why there is none.
    pub outcome: Outcome<NcaResult>,
}

/// The fit of one subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitRun {
    /// Subject label.
    pub subject: String,
    /// The dose used.
    pub dose: Option<f64>,
    /// Observations used (rows with a missing time or concentration are left out).
    pub n_observations: usize,
    /// Rows left out because of a missing concentration.
    pub n_missing: usize,
    /// The result, or why there is none.
    pub outcome: Outcome<FitResult>,
}

/// A simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationRun {
    /// The times evaluated.
    pub times: Vec<f64>,
    /// The curves, or why there are none.
    pub outcome: Outcome<ModelOutput>,
}

/// The result of an analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalysisResult {
    /// One result per subject.
    Nca {
        /// Subjects, in worksheet order.
        subjects: Vec<NcaSubjectResult>,
    },
    /// A fit.
    Fit(FitRun),
    /// A simulation.
    Simulation(SimulationRun),
}

/// Why a stored result no longer matches the analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum StaleReason {
    /// The worksheet changed after the run (data, a role or a unit).
    InputChanged {
        /// Revision of the worksheet when the analysis ran.
        ran_on_revision: u64,
        /// Revision now.
        current_revision: u64,
    },
    /// The options of the analysis changed after the run.
    OptionsChanged,
    /// The worksheet no longer exists.
    WorksheetMissing,
}

impl StaleReason {
    /// A sentence for the person looking at the result.
    pub fn message(&self) -> &'static str {
        match self {
            StaleReason::InputChanged { .. } => {
                "the data changed since this result was computed; run the analysis again"
            }
            StaleReason::OptionsChanged => {
                "the options changed since this result was computed; run the analysis again"
            }
            StaleReason::WorksheetMissing => "the worksheet this result came from no longer exists",
        }
    }
}

/// Whether the result of an analysis can be trusted as shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AnalysisStatus {
    /// Never run.
    NoResult,
    /// The result matches the current data and options.
    Fresh,
    /// The result is out of date.
    Stale {
        /// Why.
        reasons: Vec<StaleReason>,
    },
}

/// The record of a run: the result and what it was computed from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredResult {
    pub(crate) result: AnalysisResult,
    /// Revision of the worksheet read, `None` for an analysis without a worksheet.
    pub(crate) worksheet_revision: Option<u64>,
    pub(crate) spec_version: u64,
}

/// An analysis object: options and last result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub(crate) id: AnalysisId,
    /// Name given by the user; the label is derived from the content when it is `None`.
    pub(crate) name: Option<String>,
    pub(crate) spec: AnalysisSpec,
    /// Incremented when the spec changes.
    pub(crate) spec_version: u64,
    pub(crate) stored: Option<StoredResult>,
}

impl Analysis {
    /// Identifier in the project.
    pub fn id(&self) -> AnalysisId {
        self.id
    }

    /// The name given by the user, if any.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// What the analysis does.
    pub fn spec(&self) -> &AnalysisSpec {
        &self.spec
    }

    /// The last result, even if stale.
    pub fn result(&self) -> Option<&AnalysisResult> {
        self.stored.as_ref().map(|s| &s.result)
    }
}
