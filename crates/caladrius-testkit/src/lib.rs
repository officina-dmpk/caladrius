#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unreachable
)]
// The independent reference code (`naive`, `naive2c`, the tolerances) uses the platform's maths library
// on purpose: a second implementation that the engine's pure Rust `libm` is checked against.
#![allow(clippy::disallowed_methods)]
//! Test layer: oracles, tolerances and table comparison for the conformance tests; never shipped.
//!
//! - [`tolerance`]: the tolerances of `AGENTS.md` section 5, defined once.
//! - [`compare`]: tables of values and a comparison that reports every value outside tolerance.
//! - [`oracle`]: loaders for the versioned public oracle in `oracle/`.
//! - [`step3`]: loaders for the closed-form model values and the reference fits of step 3.
//! - [`ml`]: loaders for the maximum-likelihood oracle (`oracle/expected/fit/ml/`, task T-148), the absolute tolerances of its tests and a naive double-precision maximum-likelihood fit.
//! - [`pop`]: loaders for the population-fit oracle (`oracle/expected/pop/`, task T-152): the exact identification suite, the objective at fixed parameters with its diagnostics, the refusals, the sealed simulate-then-fit data and their SHA-256 check, and the absolute tolerances of those tests.
//! - [`naive_pop`]: an independent double-precision population objective (second-order jets, Newton modes, five forms, diagnostics) that reproduces the 256-bit values.
//! - [`naive`]: independent, deliberately simple computations (second implementation for cross-checks).
//! - [`naive2c`]: the same for the two-compartment models (task T-032), in double precision.
//! - [`pk2`]: loaders for the two-compartment oracle (`oracle/expected/models/pk2/`): values, derivatives, error cases.
//! - [`naive3c`]: the same for the three-compartment models (task T-142), in double precision.
//! - [`pk3`]: loaders for the three-compartment oracle (`oracle/expected/models/pk3/`): values, conditioning, conversions, derivatives, multiple dosing, error cases.
//! - [`md`]: loaders for the multiple-dosing and steady-state oracle (`oracle/expected/models/md/`, task T-047): schedules, regular regimens, steady state, error cases.
//!
//! Nothing here panics: loaders return [`oracle::OracleError`], comparisons return a
//! [`compare::Report`] whose [`compare::Report::into_result`] a test can unwrap.

pub mod compare;
pub mod md;
pub mod ml;
pub mod naive;
pub mod naive2c;
pub mod naive3c;
pub mod naive_pop;
pub mod oracle;
pub mod pk2;
pub mod pk3;
pub mod pop;
pub mod step3;
pub mod tolerance;

pub use compare::{
    DocumentedDifference, Mismatch, MismatchKind, Report, Skipped, Table, compare_tables,
    compare_tables_documented,
};
pub use md::{
    DoseRecord, MD_ERRORS, MdCase, PrintedForm, Regimen, RegimenKind, dose_parameters,
    list_md_cases, load_md_case, load_md_case_in, load_md_errors, md_dir,
};
pub use ml::{
    AbsoluteTolerance, FIT_ML_ERRORS, ML_ERROR_MODELS, ML_LS_FORMULA_RELATIVE,
    ML_STRUCTURAL_ZERO_CUTOFF, MlCase, MlControl, MlErrorCase, MlExpectedError, MlFitOptions,
    MlGroup, NaiveMl, fit_ml_dir, list_fit_ml_cases, load_fit_ml_case, load_fit_ml_errors,
    load_fit_ml_gnls, naive_ml,
};
pub use naive_pop::{
    NaiveDiagnostics, NaiveFrozen, NaivePop, NaivePopulation, NaiveSubject, naive_pop,
};
pub use oracle::{OracleCase, OracleError, Profile, list_cases, load_case};
pub use pk2::{
    PK2_ERRORS, Pk2Case, Pk2ErrorCase, Pk2Kind, list_pk2_cases, load_pk2_case, load_pk2_errors,
    pk2_dir,
};
pub use pk3::{
    PK3_ERRORS, Pk3Case, Pk3Kind, PoleClaim, list_pk3_cases, list_pk3_md_cases, load_pk3_case,
    load_pk3_errors, load_pk3_md_case, pk3_dir,
};
pub use pop::{
    POP_FORMS, POP_SEALED_DESIGNS, PopControl, PopErrorCase, PopExactCase, PopExpectedError,
    PopFitBasis, PopFitCase, PopFitDesign, PopFitExpected, PopFitStart, PopModel, PopOfvCase,
    PopSealed, PopSubject, list_pop_exact_cases, list_pop_fit_cases, list_pop_ofv_cases,
    load_pop_errors, load_pop_exact_case, load_pop_fit_case, load_pop_ofv_case, load_pop_sealed,
    load_pop_sealed_replicate, pop_data_dir, pop_dir, sha256_hex, verify_pop_sealed,
};
pub use step3::{
    FitCase, FitCaseOptions, GaussNewtonStep, ModelCase, Observations, list_fit_cases,
    list_model_cases, load_fit_case, load_gauss_newton_f1, load_model_case,
};
pub use tolerance::Tolerance;
