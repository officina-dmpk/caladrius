#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
//! Test layer: oracles, tolerances and table comparison for the conformance tests; never shipped.
//!
//! - [`tolerance`]: the tolerances of `AGENTS.md` section 5, defined once.
//! - [`compare`]: tables of values and a comparison that reports every value outside tolerance.
//! - [`oracle`]: loaders for the versioned public oracle in `oracle/`.
//! - [`naive`]: independent, deliberately simple computations (second implementation for cross-checks).
//!
//! Nothing here panics: loaders return [`oracle::OracleError`], comparisons return a
//! [`compare::Report`] whose [`compare::Report::into_result`] a test can unwrap.

pub mod compare;
pub mod naive;
pub mod oracle;
pub mod tolerance;

pub use compare::{
    DocumentedDifference, Mismatch, MismatchKind, Report, Skipped, Table, compare_tables,
    compare_tables_documented,
};
pub use oracle::{OracleCase, OracleError, Profile, list_cases, load_case};
pub use tolerance::Tolerance;
