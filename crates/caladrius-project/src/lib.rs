#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Layer L1: project model (worksheets, workflow objects, results) as pure serializable data.
//!
//! A [`Project`] holds worksheets (columns with a role and a unit, all data) and analysis objects
//! (NCA, model fit, population fit, simulation, XY plot). An analysis holds its options ([`AnalysisSpec`]) and its last
//! result ([`AnalysisResult`]); whether that result is still valid is derived from revision
//! counters ([`Project::status`], `AGENTS.md` section 7, friction 7), never stored as a flag that
//! could drift.
//!
//! No file access and no clock: a CSV import takes bytes ([`ImportedTable::from_csv`]), saving gives
//! bytes ([`Project::to_bytes`]). Nothing here computes a number; running the analyses is the
//! job of `caladrius-engine`.

mod analysis;
mod csv;
mod dataset;
mod error;
mod plot;
mod project;
mod subject;
mod units;
mod worksheet;

pub use analysis::{
    ALL_SUBJECTS, Analysis, AnalysisId, AnalysisResult, AnalysisSpec, AnalysisStatus, FitRun,
    FitSpec, NcaSpec, NcaSubjectResult, Outcome, PopFitRun, PopFitSpec, PopRefusedSubject,
    SimulationRun, SimulationSpec, StaleReason,
};
pub use csv::{CsvOptions, ImportedTable, Reading, ReadingCheck, readings};
pub use dataset::{
    CovariateValue, Dataset, DoseKind, DoseRecord, Layout, ObservationRecord, ObservationStatus,
    SubjectRecords,
};
pub use error::ProjectError;
pub use plot::{
    FitView, PlotAxis, PlotGuide, PlotLayout, PlotPanel, PlotPoint, PlotRun, PlotScale, PlotSeries,
    PlotSource, PlotSpec, SeriesRole, SeriesStyle,
};
pub use project::{FORMAT_VERSION, Project};
pub use subject::{ObservationCounts, Purpose, SubjectData, SubjectRefusal};
pub use units::{AcceptedUnits, UnitWarning, accepted_units, derived_units, unit_warnings};
pub use worksheet::{
    Column, ColumnData, ColumnRole, Profile, SINGLE_SUBJECT, Worksheet, WorksheetId,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dataset;
#[cfg(test)]
mod tests_plot;
