#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Synthetic lambda_z oracle (task T-005): `caladrius-nca` against PKNCA on four hand-made oral
//! profiles that separate the competing readings of the terminal-phase selection rule
//! (`specs/nca.md`, section 6 and open items O-01 and O-02). Like `oracle_public.rs`, this file is
//! written before the engine exists and uses the same API (see the header of that file); it does
//! not compile until `caladrius-nca` provides it, and its tests are not ignored.
//!
//! What PKNCA 0.12.1 does, established by running it on these profiles (`oracle/data/README.md`):
//!
//! - O-01: among the candidate fits, the admissible ones have an adjusted R squared above the
//!   best adjusted R squared minus the factor `adj_r_squared_factor`; the one with the most points
//!   wins (the "tolerance" reading). The "bonus" reading (adjusted R squared + factor * n) gives
//!   other windows on subjects 1 and 3.
//! - O-02: the best adjusted R squared is taken over ALL candidate fits, those with a rising
//!   terminal phase included; fits with lambda_z <= 0 are dropped afterwards (the "positive filter
//!   after the selection"). Subject 2 therefore has no terminal phase, and subject 4 gets another
//!   window than it would with the filter applied first.
//!
//! The default `NcaOptions` of the engine must follow PKNCA, so these tests need no option beyond
//! the ones of `oracle_public.rs`. A test of the other readings belongs to the engine's own unit tests.
//!
//! Subjects (1: profile D1 of W6, 2: profile D2 of W6, 3 and 4: found by search, see the README):
//! expected number of points in the terminal phase is 3, not estimable, 4, 4. Cases:
//! `synthetic_lz` (factor 1e-4, PKNCA's default) and `synthetic_lz_f1e3` (factor 1e-3).

use caladrius_nca::{AucMethod, LambdaZOptions, NcaInput, NcaOptions, Route, run};
use caladrius_testkit::{OracleCase, Table, Tolerance, compare_tables, load_case};

/// Parameter groups, one test per case and group. Names are PKNCA's.
const OBSERVED: &[&str] = &["cmax", "tmax", "tfirst", "tlast", "clast.obs"];
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
    synthetic_lz_observed: "synthetic_lz", OBSERVED;
    synthetic_lz_terminal_phase: "synthetic_lz", TERMINAL_PHASE;
    synthetic_lz_auc_to_last: "synthetic_lz", AUC_TO_LAST;
    synthetic_lz_extrapolation: "synthetic_lz", EXTRAPOLATION;
    synthetic_lz_derived: "synthetic_lz", DERIVED;

    synthetic_lz_f1e3_observed: "synthetic_lz_f1e3", OBSERVED;
    synthetic_lz_f1e3_terminal_phase: "synthetic_lz_f1e3", TERMINAL_PHASE;
    synthetic_lz_f1e3_auc_to_last: "synthetic_lz_f1e3", AUC_TO_LAST;
    synthetic_lz_f1e3_extrapolation: "synthetic_lz_f1e3", EXTRAPOLATION;
    synthetic_lz_f1e3_derived: "synthetic_lz_f1e3", DERIVED;
}

/// The number of points in the terminal phase of one subject, as the engine reports it.
fn engine_points(case_name: &str, subject: u32) -> Option<f64> {
    let case = load_case(case_name).expect("the synthetic oracle case loads");
    let profile = case
        .dataset
        .profiles
        .iter()
        .find(|p| p.subject == subject)
        .expect("the subject is in the dataset");
    let input = NcaInput {
        time: profile.time.clone(),
        conc: profile.conc.clone(),
        dose: profile.dose,
        route: route_of(&case),
        options: options_of(&case),
    };
    run(&input)
        .expect("the engine accepts a valid synthetic profile")
        .get("lambda.z.n.points")
}

/// O-01, profile D1 (subject 1): 3 points (tolerance reading); the bonus reading would give 6.
#[test]
fn o01_d1_takes_3_points_not_6() {
    assert_eq!(engine_points("synthetic_lz", 1), Some(3.0));
}

/// O-01, subject 3: 4 points; the bonus reading gives 5, the best adjusted R squared alone 3.
#[test]
fn o01_subject_3_takes_4_points() {
    assert_eq!(engine_points("synthetic_lz", 3), Some(4.0));
}

/// O-02, profile D2 (subject 2): the best fit is rising, so there is no terminal phase (the
/// positive filter applied first would give 6 points).
#[test]
fn o02_d2_has_no_terminal_phase() {
    assert_eq!(engine_points("synthetic_lz", 2), None);
}

/// O-02, subject 4: 4 points with the filter after the selection (5 with the filter first).
#[test]
fn o02_subject_4_takes_4_points() {
    assert_eq!(engine_points("synthetic_lz", 4), Some(4.0));
}

/// The factor reaches the selection: with 1e-3, D1 takes all 6 eligible points.
#[test]
fn wider_factor_takes_more_points_on_d1() {
    assert_eq!(engine_points("synthetic_lz_f1e3", 1), Some(6.0));
}

/// Guards the guard: every expected value of the synthetic cases belongs to exactly one group.
#[test]
fn every_expected_parameter_is_covered_by_a_group() {
    let groups: [&[&str]; 5] = [
        OBSERVED,
        TERMINAL_PHASE,
        AUC_TO_LAST,
        EXTRAPOLATION,
        DERIVED,
    ];
    for name in ["synthetic_lz", "synthetic_lz_f1e3"] {
        let case = load_case(name).expect("the synthetic oracle case loads");
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
