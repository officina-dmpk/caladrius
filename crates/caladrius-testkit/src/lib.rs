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
//! - [`private`]: the private oracle (exports of the reference software, equality at the displayed precision).
//! - [`step3`]: loaders for the closed-form model values and the reference fits of step 3.
//! - [`naive`]: independent, deliberately simple computations (second implementation for cross-checks).
//! - [`naive2c`]: the same for the two-compartment models (task T-032), in double precision.
//! - [`pk2`]: loaders for the two-compartment oracle (`oracle/expected/models/pk2/`): values, derivatives, error cases.
//!
//! Nothing here panics: loaders return [`oracle::OracleError`], comparisons return a
//! [`compare::Report`] whose [`compare::Report::into_result`] a test can unwrap.

pub mod compare;
pub mod naive;
pub mod naive2c;
pub mod oracle;
pub mod pk2;
pub mod private;
pub mod step3;
pub mod tolerance;

pub use compare::{
    DocumentedDifference, Mismatch, MismatchKind, Report, Skipped, Table, compare_tables,
    compare_tables_documented,
};
pub use oracle::{OracleCase, OracleError, Profile, list_cases, load_case};
pub use pk2::{
    PK2_ERRORS, Pk2Case, Pk2ErrorCase, Pk2Kind, list_pk2_cases, load_pk2_case, load_pk2_errors,
    pk2_dir,
};
pub use step3::{
    FitCase, FitCaseOptions, GaussNewtonStep, ModelCase, Observations, list_fit_cases,
    list_model_cases, load_fit_case, load_gauss_newton_f1, load_model_case,
};
pub use tolerance::Tolerance;
