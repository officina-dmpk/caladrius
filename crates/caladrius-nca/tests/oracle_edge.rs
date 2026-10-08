#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Edge-case oracle (task T-012): `caladrius-nca` against PKNCA on small hand-made profiles for the
//! rules of `specs/nca.md` that the public datasets do not exercise (open item O-17): BLQ policies
//! on interior, leading and trailing zeros, missing and negative concentrations, IV infusion, Tlag,
//! dose-normalised values, MRT to Tlast, plain Vss, percent AUMC extrapolated, C0 methods and the
//! IV ("back-extrapolated") areas and percentages. Data and rules: `oracle/data/README.md`.
//!
//! One test per rule and case; each lists every value outside tolerance. A failure is information
//! for the engine (or for the spec, where PKNCA and the spec differ), not a reason to loosen a
//! tolerance. Which of these tests fail is recorded in `board/tasks/T-012.md` and in
//! `docs/conformance.md`. The profiles are run as given (no point is added), with the start policy
//! and the negative-value policy named in the `engine` section of each case's options file.
//!
//! The expected `aumcpext.*` are computed from PKNCA's AUMC values by the oracle script
//! (`100 * (1 - aumclast / aumcinf)`), since PKNCA has no such parameter.

use caladrius_nca::{
    AucMethod, BlqAction, BlqPolicy, LambdaZOptions, MissingPolicy, NcaInput, NcaOptions,
    NegativePolicy, Route, StartPolicy, TmaxTie, run,
};
use caladrius_testkit::oracle::{BlqRule, NaRule};
use caladrius_testkit::{OracleCase, Table, Tolerance, compare_tables, load_case};

// Parameter groups. Names are PKNCA's.
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
const AREAS: &[&str] = &["auclast", "aucall", "aumclast", "aumcall"];
const EXTRAPOLATION: &[&str] = &[
    "aucinf.obs",
    "aucinf.pred",
    "aumcinf.obs",
    "aumcinf.pred",
    "aucpext.obs",
    "aucpext.pred",
];
const DERIVED_ORAL: &[&str] = &[
    "cl.obs", "cl.pred", "vz.obs", "vz.pred", "mrt.obs", "mrt.pred",
];
const DERIVED_IV: &[&str] = &[
    "cl.obs",
    "cl.pred",
    "vz.obs",
    "vz.pred",
    "mrt.iv.obs",
    "mrt.iv.pred",
    "vss.iv.obs",
    "vss.iv.pred",
];
const TLAG: &[&str] = &["tlag"];
const MRT_LAST_ORAL: &[&str] = &["mrt.last"];
const MRT_LAST_IV: &[&str] = &["mrt.iv.last", "vss.iv.last"];
const VSS_PLAIN: &[&str] = &["vss.obs", "vss.pred"];
const AUMC_PERCENT: &[&str] = &["aumcpext.obs", "aumcpext.pred"];
const DOSE_NORMALISED: &[&str] = &[
    "cmax.dn",
    "clast.obs.dn",
    "auclast.dn",
    "aucall.dn",
    "aucinf.obs.dn",
    "aucinf.pred.dn",
    "aumclast.dn",
    "aumcall.dn",
    "aumcinf.obs.dn",
    "aumcinf.pred.dn",
];
const C0: &[&str] = &["c0"];
const IV_AREAS: &[&str] = &["aucivlast", "aucivall", "aucivinf.obs", "aucivinf.pred"];
const IV_BACK_EXTRAPOLATED_PERCENT: &[&str] = &[
    "aucivpbextlast",
    "aucivpbextall",
    "aucivpbextinf.obs",
    "aucivpbextinf.pred",
];

fn blq_action(rule: BlqRule) -> BlqAction {
    match rule {
        BlqRule::Keep => BlqAction::Keep,
        BlqRule::Drop => BlqAction::Drop,
        BlqRule::Set(x) => BlqAction::Set(x),
    }
}

/// The engine options matching the PKNCA options and the engine hints recorded with the case.
fn options_of(case: &OracleCase) -> NcaOptions {
    let pk = &case.options.pknca_options;
    let auc_method = match pk.auc_method.as_str() {
        "linear" => AucMethod::Linear,
        "lin up/log down" => AucMethod::LinUpLogDown,
        other => panic!("{}: unknown AUC method {other:?}", case.name),
    };
    let missing = match pk.conc_na {
        NaRule::Drop => MissingPolicy::Drop,
        NaRule::Replace(x) => MissingPolicy::Replace(x),
    };
    let rule = &pk.conc_blq;
    let blq = match (
        rule.first,
        rule.middle,
        rule.last,
        rule.before_tmax,
        rule.after_tmax,
    ) {
        (Some(first), Some(middle), Some(last), None, None) => BlqPolicy::Position {
            first: blq_action(first),
            middle: blq_action(middle),
            last: blq_action(last),
        },
        (None, None, None, Some(before), Some(after)) => BlqPolicy::Tmax {
            before: blq_action(before),
            after: blq_action(after),
        },
        _ => panic!("{}: incomplete BLQ rule in the oracle options", case.name),
    };
    let hints = &case.options.engine;
    let start = match hints.start.as_deref() {
        None => StartPolicy::default(),
        Some("none") => StartPolicy::None,
        Some("zero") => StartPolicy::Zero,
        Some("c0") => StartPolicy::C0,
        Some(other) => panic!("{}: unknown start policy {other:?}", case.name),
    };
    let negative = match hints.negative.as_deref() {
        None => NegativePolicy::default(),
        Some("error") => NegativePolicy::Error,
        Some("allow") => NegativePolicy::Allow,
        Some("set_zero") => NegativePolicy::SetZero,
        Some(other) => panic!("{}: unknown negative policy {other:?}", case.name),
    };
    NcaOptions {
        auc_method,
        missing,
        blq,
        start,
        negative,
        tmax_tie: if pk.first_tmax {
            TmaxTie::First
        } else {
            TmaxTie::Last
        },
        lambda_z: LambdaZOptions {
            min_points: pk.min_hl_points as usize,
            allow_tmax: pk.allow_tmax_in_half_life,
            adj_r_squared_factor: pk.adj_r_squared_factor,
        },
        ..NcaOptions::default()
    }
}

fn route_of(case: &OracleCase) -> Route {
    match (case.options.route.as_str(), case.options.infusion_duration) {
        ("extravascular", _) => Route::Extravascular,
        ("iv_bolus", _) => Route::IvBolus,
        ("iv_infusion", Some(duration)) => Route::IvInfusion { duration },
        (other, _) => panic!(
            "{}: unusable route {other:?} in the oracle options",
            case.name
        ),
    }
}

/// Runs the engine on every subject and tabulates the requested parameters. A subject the engine
/// rejects is reported as an error text, together with the numeric mismatches of the others.
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
    let case = load_case(case_name).expect("the edge oracle case loads");
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

// NCA-DAT-05 to DAT-07: BLQ (zero) policies on leading, interior and trailing zeros.
oracle_tests! {
    blq_default_observed: "edge_blq_default", OBSERVED;
    blq_default_terminal_phase: "edge_blq_default", TERMINAL_PHASE;
    blq_default_areas: "edge_blq_default", AREAS;
    blq_default_extrapolation: "edge_blq_default", EXTRAPOLATION;
    blq_default_derived: "edge_blq_default", DERIVED_ORAL;

    blq_keep_observed: "edge_blq_keep", OBSERVED;
    blq_keep_terminal_phase: "edge_blq_keep", TERMINAL_PHASE;
    blq_keep_areas: "edge_blq_keep", AREAS;
    blq_keep_extrapolation: "edge_blq_keep", EXTRAPOLATION;
    blq_keep_derived: "edge_blq_keep", DERIVED_ORAL;

    blq_last_drop_observed: "edge_blq_last_drop", OBSERVED;
    blq_last_drop_terminal_phase: "edge_blq_last_drop", TERMINAL_PHASE;
    blq_last_drop_areas: "edge_blq_last_drop", AREAS;
    blq_last_drop_extrapolation: "edge_blq_last_drop", EXTRAPOLATION;
    blq_last_drop_derived: "edge_blq_last_drop", DERIVED_ORAL;

    blq_first_drop_observed: "edge_blq_first_drop", OBSERVED;
    blq_first_drop_terminal_phase: "edge_blq_first_drop", TERMINAL_PHASE;
    blq_first_drop_areas: "edge_blq_first_drop", AREAS;
    blq_first_drop_extrapolation: "edge_blq_first_drop", EXTRAPOLATION;
    blq_first_drop_derived: "edge_blq_first_drop", DERIVED_ORAL;

    blq_set_observed: "edge_blq_set", OBSERVED;
    blq_set_terminal_phase: "edge_blq_set", TERMINAL_PHASE;
    blq_set_areas: "edge_blq_set", AREAS;
    blq_set_extrapolation: "edge_blq_set", EXTRAPOLATION;
    blq_set_derived: "edge_blq_set", DERIVED_ORAL;

    blq_tmax_observed: "edge_blq_tmax", OBSERVED;
    blq_tmax_terminal_phase: "edge_blq_tmax", TERMINAL_PHASE;
    blq_tmax_areas: "edge_blq_tmax", AREAS;
    blq_tmax_extrapolation: "edge_blq_tmax", EXTRAPOLATION;
    blq_tmax_derived: "edge_blq_tmax", DERIVED_ORAL;
}

// NCA-DAT-03: missing concentrations, dropped or replaced by a number.
oracle_tests! {
    missing_drop_observed: "edge_missing_drop", OBSERVED;
    missing_drop_terminal_phase: "edge_missing_drop", TERMINAL_PHASE;
    missing_drop_areas: "edge_missing_drop", AREAS;
    missing_drop_extrapolation: "edge_missing_drop", EXTRAPOLATION;

    missing_replace_observed: "edge_missing_replace", OBSERVED;
    missing_replace_terminal_phase: "edge_missing_replace", TERMINAL_PHASE;
    missing_replace_areas: "edge_missing_replace", AREAS;
    missing_replace_extrapolation: "edge_missing_replace", EXTRAPOLATION;
}

// NCA-DAT-04: negative concentrations kept (policy `allow`), linear rule.
oracle_tests! {
    negative_observed: "edge_negative_linear", OBSERVED;
    negative_terminal_phase: "edge_negative_linear", TERMINAL_PHASE;
    negative_areas: "edge_negative_linear", AREAS;
    negative_extrapolation: "edge_negative_linear", EXTRAPOLATION;
    negative_derived: "edge_negative_linear", DERIVED_ORAL;
}

// Oral profiles with and without a lag, several doses (also the base of the Tlag, MRT, Vss and
// dose-normalised tests below).
oracle_tests! {
    oral_observed: "edge_oral", OBSERVED;
    oral_terminal_phase: "edge_oral", TERMINAL_PHASE;
    oral_areas: "edge_oral", AREAS;
    oral_extrapolation: "edge_oral", EXTRAPOLATION;
    oral_derived: "edge_oral", DERIVED_ORAL;
}

// NCA-OBS-04: Tlag.
oracle_tests! {
    tlag_oral: "edge_oral", TLAG;
    tlag_with_blq_zeros: "edge_blq_default", TLAG;
}

// NCA-OBS-05: dose-normalised values.
oracle_tests! {
    dose_normalised_oral: "edge_oral", DOSE_NORMALISED;
    dose_normalised_iv_bolus: "edge_iv", DOSE_NORMALISED;
    dose_normalised_infusion: "edge_infusion", DOSE_NORMALISED;
}

// NCA-EXT-04: MRT to Tlast (and the IV Vss to Tlast).
oracle_tests! {
    mrt_last_oral: "edge_oral", MRT_LAST_ORAL;
    mrt_last_iv_bolus: "edge_iv", MRT_LAST_IV;
    mrt_last_infusion: "edge_infusion", MRT_LAST_IV;
}

// NCA-EXT-07: plain (uncorrected) Vss.
oracle_tests! {
    vss_plain_oral: "edge_oral", VSS_PLAIN;
    vss_plain_iv_bolus: "edge_iv", VSS_PLAIN;
    vss_plain_infusion: "edge_infusion", VSS_PLAIN;
}

// NCA-EXT-03b: percent AUMC extrapolated (expected values derived by the script).
oracle_tests! {
    aumc_percent_extrapolated_oral: "edge_oral", AUMC_PERCENT;
    aumc_percent_extrapolated_blq: "edge_blq_keep", AUMC_PERCENT;
    aumc_percent_extrapolated_iv_bolus: "edge_iv", AUMC_PERCENT;
    aumc_percent_extrapolated_infusion: "edge_infusion", AUMC_PERCENT;
}

// NCA-IV-01: the three C0 methods; NCA-IV-02 and IV-03: IV areas and percent back-extrapolated.
oracle_tests! {
    iv_bolus_c0_methods: "edge_iv", C0;
    iv_bolus_observed: "edge_iv", OBSERVED;
    iv_bolus_terminal_phase: "edge_iv", TERMINAL_PHASE;
    iv_bolus_areas: "edge_iv", AREAS;
    iv_bolus_extrapolation: "edge_iv", EXTRAPOLATION;
    iv_bolus_derived: "edge_iv", DERIVED_IV;
    iv_bolus_iv_areas: "edge_iv", IV_AREAS;
    iv_bolus_percent_back_extrapolated: "edge_iv", IV_BACK_EXTRAPOLATED_PERCENT;

    iv_bolus_linear_c0_methods: "edge_iv_linear", C0;
    iv_bolus_linear_areas: "edge_iv_linear", AREAS;
    iv_bolus_linear_extrapolation: "edge_iv_linear", EXTRAPOLATION;
    iv_bolus_linear_derived: "edge_iv_linear", DERIVED_IV;
    iv_bolus_linear_iv_areas: "edge_iv_linear", IV_AREAS;
    iv_bolus_linear_percent_back_extrapolated: "edge_iv_linear", IV_BACK_EXTRAPOLATED_PERCENT;
}

// NCA-EXT-04 and EXT-07 on an infusion: MRT and Vss corrected for the duration; NCA-LZ-02 rule 3:
// no terminal-phase point during the infusion.
oracle_tests! {
    infusion_observed: "edge_infusion", OBSERVED;
    infusion_terminal_phase: "edge_infusion", TERMINAL_PHASE;
    infusion_areas: "edge_infusion", AREAS;
    infusion_extrapolation: "edge_infusion", EXTRAPOLATION;
    infusion_derived: "edge_infusion", DERIVED_IV;

    infusion_linear_observed: "edge_infusion_linear", OBSERVED;
    infusion_linear_terminal_phase: "edge_infusion_linear", TERMINAL_PHASE;
    infusion_linear_areas: "edge_infusion_linear", AREAS;
    infusion_linear_extrapolation: "edge_infusion_linear", EXTRAPOLATION;
    infusion_linear_derived: "edge_infusion_linear", DERIVED_IV;
}

/// Guards the guard: every expected parameter of every edge case belongs to a group used above, so
/// no parameter silently escapes the tests.
#[test]
fn every_expected_parameter_is_covered_by_a_group() {
    let groups: &[&[&str]] = &[
        OBSERVED,
        TERMINAL_PHASE,
        AREAS,
        EXTRAPOLATION,
        DERIVED_ORAL,
        DERIVED_IV,
        TLAG,
        MRT_LAST_ORAL,
        MRT_LAST_IV,
        VSS_PLAIN,
        AUMC_PERCENT,
        DOSE_NORMALISED,
        C0,
        IV_AREAS,
        IV_BACK_EXTRAPOLATED_PERCENT,
    ];
    for name in caladrius_testkit::list_cases().expect("list the cases") {
        if !name.starts_with("edge_") {
            continue;
        }
        let case = load_case(&name).expect("the edge oracle case loads");
        for parameter in &case.options.parameters {
            assert!(
                groups.iter().any(|g| g.contains(&parameter.as_str())),
                "{name}: parameter {parameter} is in no group"
            );
        }
    }
}
