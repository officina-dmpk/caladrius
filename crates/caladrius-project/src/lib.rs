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
//! (NCA, model fit, simulation). An analysis holds its options ([`AnalysisSpec`]) and its last
//! result ([`AnalysisResult`]); whether that result is still valid is derived from revision
//! counters ([`Project::status`], `AGENTS.md` section 7, friction 7), never stored as a flag that
//! could drift.
//!
//! No file access and no clock: a CSV import takes bytes ([`ImportedTable::from_csv`]), saving gives
//! bytes ([`Project::to_bytes`]). Nothing here computes a number; running the analyses is the
//! job of `caladrius-engine`.

mod analysis;
mod csv;
mod error;
mod project;
mod units;
mod worksheet;

pub use analysis::{
    Analysis, AnalysisId, AnalysisResult, AnalysisSpec, AnalysisStatus, FitRun, FitSpec, NcaSpec,
    NcaSubjectResult, Outcome, SimulationRun, SimulationSpec, StaleReason,
};
pub use csv::{CsvOptions, ImportedTable};
pub use error::ProjectError;
pub use project::{FORMAT_VERSION, Project};
pub use units::{UnitWarning, derived_units, unit_warnings};
pub use worksheet::{
    Column, ColumnData, ColumnRole, Profile, SINGLE_SUBJECT, Worksheet, WorksheetId,
};

#[cfg(test)]
mod tests;
