//! Analysis objects: the options of an NCA, a model fit, a population fit, a simulation or a plot,
//! and their last result.

use std::collections::BTreeMap;

use caladrius_fit::pop::{PopAnswer, PopOptions};
use caladrius_fit::{Bounds, FitOptions, FitResult, Weighting};
use caladrius_models::{ModelId, ModelInput, ModelOutput};
use caladrius_nca::{NcaOptions, NcaResult, Route};
use serde::{Deserialize, Serialize};

use crate::dataset::CovariateValue;
use crate::plot::{PlotRun, PlotSource, PlotSpec};
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

/// The value of [`FitSpec::subject`] that asks for one independent fit per subject (DAT-USE-02).
/// A subject that is itself labelled `all` is the subject, not the keyword.
pub const ALL_SUBJECTS: &str = "all";

/// Fit of a model to the profile of one subject, or to each subject on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FitSpec {
    /// The worksheet read.
    pub worksheet: WorksheetId,
    /// The subject; `None` when the worksheet has a single subject; [`ALL_SUBJECTS`] for one
    /// independent fit per subject, with the same model and options.
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

/// Population fit of a model to every subject of a worksheet (`specs/population.md`): FOCE with
/// interaction by default. Besides the worksheet, the fields are those of the population request
/// (POP-API-01); the subjects are read from the worksheet, one dose per subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopFitSpec {
    /// The worksheet read.
    pub worksheet: WorksheetId,
    /// The structural model.
    pub model: ModelId,
    /// Starting typical values by parameter; a typical value with a random effect that is left out
    /// is computed by the two-stage start (POP-EST-05).
    #[serde(default)]
    pub initial: BTreeMap<String, f64>,
    /// The parameters that carry a random effect, in the order of the rows of Ω.
    #[serde(default)]
    pub random_effects: Option<Vec<String>>,
    /// The starting Ω, q × q; `None` computes it by the two-stage start.
    #[serde(default)]
    pub omega: Option<Vec<Vec<f64>>>,
    /// `diagonal` (the default) or `block`.
    #[serde(default)]
    pub omega_shape: Option<String>,
    /// `additive`, `proportional`, `combined` or `combined_sd`.
    #[serde(default)]
    pub error_model: Option<String>,
    /// The starting error parameters in the user's units: `[a]`, `[b]` or `[a, b]`.
    #[serde(default)]
    pub error_init: Option<Vec<f64>>,
    /// Typical values, error parameters (`a`, `b`) and variances of Ω (`omega.<parameter>`) held
    /// at a value.
    #[serde(default)]
    pub fixed: BTreeMap<String, f64>,
    /// Bounds of the free typical values.
    #[serde(default)]
    pub bounds: BTreeMap<String, Bounds>,
    /// Method, convention of the objective, printed covariance, subjects left out.
    #[serde(default)]
    pub options: PopOptions,
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
    /// Population fit.
    PopFit(PopFitSpec),
    /// Model simulation.
    Simulation(SimulationSpec),
    /// XY plot of a worksheet or of a fit.
    Plot(PlotSpec),
}

impl AnalysisSpec {
    /// The worksheet the analysis reads, if any.
    pub fn worksheet(&self) -> Option<WorksheetId> {
        match self {
            AnalysisSpec::Nca(s) => Some(s.worksheet),
            AnalysisSpec::Fit(s) => Some(s.worksheet),
            AnalysisSpec::PopFit(s) => Some(s.worksheet),
            AnalysisSpec::Simulation(_) => None,
            AnalysisSpec::Plot(s) => match s.source {
                PlotSource::Worksheet { worksheet } => Some(worksheet),
                PlotSource::Fit { .. } => None,
            },
        }
    }

    /// The analysis whose result this one reads, if any (a plot of a fit).
    pub fn source_analysis(&self) -> Option<AnalysisId> {
        match self {
            AnalysisSpec::Plot(PlotSpec {
                source: PlotSource::Fit { analysis, .. },
                ..
            }) => Some(*analysis),
            _ => None,
        }
    }

    /// `nca`, `fit`, `pop_fit`, `simulation` or `plot`.
    pub fn kind(&self) -> &'static str {
        match self {
            AnalysisSpec::Nca(_) => "nca",
            AnalysisSpec::Fit(_) => "fit",
            AnalysisSpec::PopFit(_) => "pop_fit",
            AnalysisSpec::Simulation(_) => "simulation",
            AnalysisSpec::Plot(_) => "plot",
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

fn is_zero(n: &usize) -> bool {
    *n == 0
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
    /// The declared covariates of the subject, copied from the dataset and used nowhere else
    /// (DAT-USE-03). Absent when none is declared.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub covariates: BTreeMap<String, Option<CovariateValue>>,
    /// Observation rows left out because they come before the dose (event layout); the notes say
    /// for which subject. Absent when none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub n_before_dose: usize,
    /// What was decided for the subject that the person should know: the rows before the dose that
    /// were left out, an infusion duration given twice. Absent when there is nothing to say.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
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
    /// Rows left out because the concentration is below the limit of quantification
    /// (NCA-DAT-05c, FIT-DAT-01). Absent from files written before it: 0.
    #[serde(default)]
    pub n_blq: usize,
    /// The result, or why there is none.
    pub outcome: Outcome<FitResult>,
    /// The declared covariates of the subject, copied from the dataset and used nowhere else
    /// (DAT-USE-03). Absent when none is declared.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub covariates: BTreeMap<String, Option<CovariateValue>>,
    /// Observation rows left out because they come before the dose (event layout); the notes say
    /// for which subject. Absent when none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub n_before_dose: usize,
    /// What was decided for the subject that the person should know (see [`NcaSubjectResult`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A subject of the worksheet that the checks of the data refused, left out of a population fit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopRefusedSubject {
    /// Subject label.
    pub subject: String,
    /// The sentence of the check: what is wrong and what to do.
    pub message: String,
}

/// A population fit: the answer of the population fit (POP-API-01, with the diagnostics of every
/// observation, POP-DIA-01) and what was decided about the data of the worksheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopFitRun {
    /// The answer, its fields at the top level.
    #[serde(flatten)]
    pub answer: PopAnswer,
    /// The subjects of the worksheet that the checks of the data refused, in worksheet order; the
    /// fit ran without them. Absent when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refused_subjects: Vec<PopRefusedSubject>,
    /// What was decided for the data that the person should know (rows before the dose left out,
    /// a duration of the dose row not used). Absent when there is nothing to say.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
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
    /// One independent fit per subject (`subject = all`), in the order of the subjects. An error
    /// in one subject is that subject's `outcome`.
    Fits {
        /// Subjects, in worksheet order.
        subjects: Vec<FitRun>,
    },
    /// A population fit (boxed: it is much larger than the other results).
    PopFit(Box<PopFitRun>),
    /// A simulation.
    Simulation(SimulationRun),
    /// The panels of a plot.
    Plot(PlotRun),
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
    /// The analysis a plot reads was run again (or its result removed) after the plot was drawn.
    SourceChanged,
    /// The result of the analysis a plot reads is itself out of date.
    SourceStale,
    /// The analysis a plot reads no longer exists.
    SourceMissing,
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
            StaleReason::SourceChanged => {
                "the analysis this plot reads was run again since it was drawn; draw it again"
            }
            StaleReason::SourceStale => {
                "the analysis this plot reads is out of date; run it again, then this plot"
            }
            StaleReason::SourceMissing => "the analysis this plot read no longer exists",
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
    /// For an analysis that reads another one (a plot of a fit): the run counter of that analysis
    /// when this result was made. Absent from files written before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_run: Option<u64>,
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
    /// Incremented each time a result is stored, so that a plot of this analysis knows when it
    /// was run again. Absent from files written before it: 0.
    #[serde(default)]
    pub(crate) runs: u64,
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
