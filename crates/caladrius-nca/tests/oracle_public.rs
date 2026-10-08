#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Public NCA oracle: `caladrius-nca` against PKNCA on Theoph (oral) and Indometh (IV bolus).
//!
//! These tests are written BEFORE the engine (task T-003) and are expected to fail to compile until
//! `caladrius-nca` provides the public API below (task T-004). They are not ignored.
//!
//! # API the engine must provide (all in the crate root, `caladrius_nca`)
//!
//! `specs/nca.md` did not exist when this file was written, so the oracle agent defined the
//! test-side API. The engine agent may add fields and variants, but must keep these names and
//! meanings, or change this file in agreement with the orchestrator. Full text in
//! `board/tasks/T-003.md`.
//!
//! ```ignore
//! pub enum Route { Extravascular, IvBolus }              // IV infusion etc. may be added
//! pub enum AucMethod { Linear, LinUpLogDown }
//! pub struct LambdaZOptions {
//!     pub min_points: usize,            // fewest points in the terminal phase (PKNCA min.hl.points = 3)
//!     pub allow_tmax: bool,             // may the observed Cmax be a terminal-phase point (false)
//!     pub adj_r_squared_factor: f64,    // tolerance of the best-adjusted-R-squared rule (1e-4)
//! }
//! pub struct NcaOptions { pub auc_method: AucMethod, pub lambda_z: LambdaZOptions /* , .. */ }
//! impl Default for NcaOptions { /* lin up/log down, 3, false, 1e-4 */ }
//! pub struct NcaInput {
//!     pub time: Vec<f64>,   // h, sorted, as in the file (may start after the dose time)
//!     pub conc: Vec<f64>,   // same unit for the whole profile
//!     pub dose: f64,        // absolute dose at time 0, in the unit of `conc` times volume
//!     pub route: Route,
//!     pub options: NcaOptions,
//! }
//! pub struct NcaResult { /* private */ }
//! impl NcaResult { pub fn get(&self, name: &str) -> Option<f64> }   // None: not estimable / not computed
//! pub struct NcaError { /* Display, std::error::Error */ }
//! pub fn run(input: &NcaInput) -> Result<NcaResult, NcaError>;
//! ```
//!
//! Parameter names are PKNCA's, spelled exactly as in `oracle/expected/*.csv` (`auclast`,
//! `lambda.z`, `aucinf.obs`, `mrt.iv.obs`...). The oracle's conventions that matter (all recorded in
//! `oracle/expected/<case>.options.json`):
//!
//! - AUC, AUMC from time 0; for an IV bolus profile with no sample at time 0 the engine back-
//!   extrapolates C0 on the log scale from the first two concentrations, reports it as `c0`, and
//!   uses it as the point at time 0 for AUC, AUMC and derived parameters. `cmax`, `tmax`, `tfirst`
//!   stay those of the observed samples, and the added point never takes part in the terminal-phase
//!   selection.
//! - `tfirst` is the first time with a positive concentration; `tlast`/`clast.obs` the last positive.
//! - Terminal phase: among the last k >= `min_points` points after Tmax, the best adjusted R squared
//!   (within `adj_r_squared_factor`, the largest k wins). No minimum R squared is applied (PKNCA returns
//!   lambda.z for R squared as low as 0.87 on Indometh).
//! - `aucpext.*` is a percentage. `cl.*` is dose / AUCinf (so CL/F when oral), `vz.*` is cl / lambda.z.
//!   Oral: `mrt.obs`/`mrt.pred` = AUMCinf / AUCinf. IV: `mrt.iv.*` (same ratio) and `vss.iv.*` = cl * mrt.

use caladrius_nca::{AucMethod, LambdaZOptions, NcaInput, NcaOptions, Route, run};
use caladrius_testkit::{OracleCase, Table, Tolerance, compare_tables, load_case};

/// Parameter groups, one test per case and group. Names are PKNCA's.
const OBSERVED: &[&str] = &["c0", "cmax", "tmax", "tfirst", "tlast", "clast.obs"];
const TERMINAL_PHASE: &[&str] = &[
    "lambda.z",
    "r.squared",
    "adj.r.squared",
    "lambda.z.time.first",
    "lambda.z.time.last",
    "lambda.z.n.points",
    "clast.pred",
    "half.life",
    "span.ratio",
];
const AUC_TO_LAST: &[&str] = &["auclast", "aucall", "aumclast"];
const EXTRAPOLATION: &[&str] = &[
    "aucinf.obs",
    "aucinf.pred",
    "aumcinf.obs",
    "aumcinf.pred",
    "aucpext.obs",
    "aucpext.pred",
];
const DERIVED: &[&str] = &[
    "cl.obs",
    "cl.pred",
    "vz.obs",
    "vz.pred",
    "mrt.obs",
    "mrt.pred",
    "mrt.iv.obs",
    "mrt.iv.pred",
    "vss.iv.obs",
    "vss.iv.pred",
];

fn options_of(case: &OracleCase) -> NcaOptions {
    let pk = &case.options.pknca_options;
    let auc_method = match pk.auc_method.as_str() {
        "linear" => AucMethod::Linear,
        "lin up/log down" => AucMethod::LinUpLogDown,
        other => panic!(
            "{}: unknown AUC method {other:?} in the oracle options",
            case.name
        ),
    };
    NcaOptions {
        auc_method,
        lambda_z: LambdaZOptions {
            min_points: pk.min_hl_points as usize,
            allow_tmax: pk.allow_tmax_in_half_life,
            adj_r_squared_factor: pk.adj_r_squared_factor,
        },
        ..NcaOptions::default()
    }
}

fn route_of(case: &OracleCase) -> Route {
    match case.options.route.as_str() {
        "extravascular" => Route::Extravascular,
        "iv_bolus" => Route::IvBolus,
        other => panic!(
            "{}: unknown route {other:?} in the oracle options",
            case.name
        ),
    }
}

/// Runs the engine on every subject of the case and tabulates the requested parameters. A subject
/// the engine rejects is reported as an error text, together with the numeric mismatches of the
/// others (the test fails with everything at once).
fn run_engine(case: &OracleCase, group: &[&str]) -> (Table, Vec<String>) {
    let options = options_of(case);
    let route = route_of(case);
    let mut actual = Table::new();
    let mut errors = Vec::new();
    for profile in &case.dataset.profiles {
        let input = NcaInput {
            time: profile.time.clone(),
            conc: profile.conc.clone(),
            dose: profile.dose,
            route,
            options: options.clone(),
        };
        let subject = profile.subject.to_string();
        match run(&input) {
            Ok(result) => {
                for name in group {
                    actual.insert(subject.as_str(), *name, result.get(name));
                }
            }
            Err(e) => errors.push(format!("subject {subject}: engine error: {e}")),
        }
    }
    (actual, errors)
}

/// The expected values of the case restricted to `group` (the case may not define every name).
fn expected_for(case: &OracleCase, group: &[&str]) -> Table {
    let mut expected = Table::new();
    for (subject, name, value) in case.expected.iter() {
        if group.contains(&name) {
            expected.insert(subject, name, value);
        }
    }
    expected
}

fn check(case_name: &str, group: &[&str]) {
    let case = load_case(case_name).expect("the public oracle case loads");
    let expected = expected_for(&case, group);
    assert!(
        !expected.is_empty(),
        "{case_name}: no expected value in this parameter group"
    );
    let (actual, errors) = run_engine(&case, group);
    let report = compare_tables(&expected, &actual, Tolerance::NCA_VS_PKNCA);
    let mut problems = errors;
    if !report.is_ok() {
        problems.push(report.to_string());
    }
    assert!(problems.is_empty(), "{case_name}: {}", problems.join("\n"));
}

macro_rules! oracle_tests {
    ($($test:ident: $case:literal, $group:expr;)*) => {
        $(
            #[test]
            fn $test() {
                check($case, $group);
            }
        )*
    };
}

oracle_tests! {
    theoph_observed: "theoph", OBSERVED;
    theoph_terminal_phase: "theoph", TERMINAL_PHASE;
    theoph_auc_to_last: "theoph", AUC_TO_LAST;
    theoph_extrapolation: "theoph", EXTRAPOLATION;
    theoph_derived: "theoph", DERIVED;

    theoph_linear_observed: "theoph_linear", OBSERVED;
    theoph_linear_terminal_phase: "theoph_linear", TERMINAL_PHASE;
    theoph_linear_auc_to_last: "theoph_linear", AUC_TO_LAST;
    theoph_linear_extrapolation: "theoph_linear", EXTRAPOLATION;
    theoph_linear_derived: "theoph_linear", DERIVED;

    indometh_observed: "indometh", OBSERVED;
    indometh_terminal_phase: "indometh", TERMINAL_PHASE;
    indometh_auc_to_last: "indometh", AUC_TO_LAST;
    indometh_extrapolation: "indometh", EXTRAPOLATION;
    indometh_derived: "indometh", DERIVED;

    indometh_linear_observed: "indometh_linear", OBSERVED;
    indometh_linear_terminal_phase: "indometh_linear", TERMINAL_PHASE;
    indometh_linear_auc_to_last: "indometh_linear", AUC_TO_LAST;
    indometh_linear_extrapolation: "indometh_linear", EXTRAPOLATION;
    indometh_linear_derived: "indometh_linear", DERIVED;
}

/// Guards the guard: every expected value of every public case belongs to exactly one group, so no
/// parameter can silently escape the tests above.
#[test]
fn every_expected_parameter_is_covered_by_a_group() {
    let groups: [&[&str]; 5] = [
        OBSERVED,
        TERMINAL_PHASE,
        AUC_TO_LAST,
        EXTRAPOLATION,
        DERIVED,
    ];
    for name in ["theoph", "theoph_linear", "indometh", "indometh_linear"] {
        let case = load_case(name).expect("the public oracle case loads");
        for parameter in &case.options.parameters {
            let count = groups
                .iter()
                .filter(|g| g.contains(&parameter.as_str()))
                .count();
            assert_eq!(
                count, 1,
                "{name}: parameter {parameter} is in {count} groups"
            );
        }
    }
}
