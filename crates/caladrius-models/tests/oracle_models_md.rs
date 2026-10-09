#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Multiple-dosing and steady-state oracle (task T-047, open item OM-16): `caladrius-models` against
//! exact values (`oracle/expected/models/md/`, produced by `oracle/scripts/models_md.R`: plain sums
//! of the single-dose closed forms of `specs/models.md` in 256-bit arithmetic, cross-checked against
//! an ODE solution with dosing events and, at steady state, the periodic solution of the linear
//! system) for the twelve ids `pk1.*` and `pk2.*`: schedules of doses (MOD-MD-01), regular regimens of
//! n doses (MOD-MD-07), the steady state of a regular regimen (MOD-MD-03 to 06) with its derived
//! quantities (MOD-MD-08 to 10), the two convention points of OM-17 (e) and (f), and the inputs the
//! engine must refuse (MOD-MD-12).
//!
//! Written before the engine knows any dosing regimen: every test below fails today, the engine
//! refusing the regimen parameters (`tau`, `n_doses`, `dose_time[i]`, ...) as unknown parameters,
//! reported here as "unsupported dosing"; the tests must stay red until the engine card for multiple
//! dosing lands. The file compiles against the present API of the crate.
//!
//! # What the engine must provide (all in the crate root, `caladrius_models`)
//!
//! The regimen is carried by the present `ModelInput`, through named parameters. This encoding is
//! provisional (MOD-MD-11 only proposes the names `tau`, `n_doses` and the record fields `time`,
//! `dose`, `dur`): the engine card may replace it by a proper type and then changes the one helper
//! `input_of` below, in agreement with the orchestrator; the tolerances are never to be touched.
//!
//! - `params` as for a single dose (`v`, `cl`; or `cl, vc, q, vp`; or `a, b, alpha, beta`; plus `ka`,
//!   `dur`, `tlag` as the id needs) and, for the regimen:
//!   - steady state: `tau`; `times` are the times `s` since the last dose, `0 <= s <= tau`; `dose` is
//!     the dose of every interval;
//!   - n equal doses: `tau` and `n_doses` (>= 1), `dose` the dose of each, the first at time 0;
//!     `times` since the first dose, in any order, any sign (a time before the first dose gives 0);
//!   - a schedule: `dose_time[i]`, `dose_amount[i]` and, for an input with a duration (infusion,
//!     zero-order absorption), `dose_dur[i]`, for `i = 0 ..`, the records in any order, overlaps
//!     allowed; the model parameter `dur` is then absent and `dose` is ignored (the field of the
//!     file is the nominal dose of the case).
//! - `ModelOutput::conc()` and `auc()` at each time: for a schedule or n doses, the sum of the
//!   single-dose concentrations and areas, each area from its own dose time (MOD-MD-01); at steady
//!   state, the concentration of the periodic profile and the area from `s = 0` to `s`.
//! - `ModelOutput::get(name)`: schedule or n doses: `auc_inf` (sum of the doses over CL); steady
//!   state: `cmax_ss`, `tmax_ss` (time since the dose, in [0, tau); not available when the maximum
//!   is attained at every s, a continuous infusion with tau = T), `cmin_ss`, `cav_ss`, `auc_tau_ss`,
//!   `accum_cmax`, `accum_auc` (MOD-MD-08 to 10), and `accum_c[i]` for the i-th time (MOD-MD-08,
//!   `R_C(s) = C_ss(s) / C_1(s)`, not available where the single-dose concentration is 0), the same
//!   convention as `aumc[i]` of the two-compartment models.
//!
//! Tolerance: `Tolerance::MODEL_VALUES` for every value and scalar (relative 1e-12, an expected zero
//! returned as exactly zero, an expected not-available row returned as not available).
//!
//! The contract comparisons are the ones carried by the case files. The tests of the last section are
//! property checks beside that contract, not comparisons with an oracle value; they use
//! `MODEL_VALUES` too, and the convention test asserts that the engine does NOT follow the printed
//! branch of S-36 where that branch differs from the oracle (a documented difference of the engine,
//! `specs/models.md` OM-17 (e)).

use caladrius_models::{ModelId, ModelInput, run};
use caladrius_testkit::{
    MdCase, Table, Tolerance, compare_tables, dose_parameters, list_md_cases, load_md_case,
    load_md_errors,
};

/// The one place where a regimen becomes a `ModelInput` (see the header).
fn input_of(case: &MdCase) -> ModelInput {
    let name = &case.case.name;
    let mut params = case.case.parameters.clone();
    for (key, value) in dose_parameters(&case.regimen) {
        params.insert(key, value);
    }
    ModelInput {
        model: ModelId::from_id(&case.case.model)
            .unwrap_or_else(|| panic!("{name}: unknown model id {:?}", case.case.model)),
        dose: case.case.dose,
        params,
        times: case.case.times.clone(),
    }
}

fn expected_of(case: &MdCase, quantity: &str) -> Table {
    let mut expected = Table::new();
    for (group, name, value) in case.case.expected.iter() {
        let wanted = if quantity == "scalar" {
            group == "scalar"
        } else {
            group.starts_with("t=") && name == quantity
        };
        if wanted {
            expected.insert(group, name, value);
        }
    }
    expected
}

fn check(case_name: &str, quantity: &str) {
    let loaded = load_md_case(case_name).expect("the multiple-dosing oracle case loads");
    let case = &loaded.case;
    let expected = expected_of(&loaded, quantity);
    assert!(
        !expected.is_empty(),
        "{case_name}: nothing expected for {quantity}"
    );
    let output = run(&input_of(&loaded))
        .unwrap_or_else(|e| panic!("{case_name}: unsupported dosing: the engine refuses the regimen: {e}"));
    let mut actual = Table::new();
    match quantity {
        "conc" | "auc" | "accum_c" => {
            for (i, key) in case.time_keys.iter().enumerate() {
                let value = match quantity {
                    "conc" => output.conc().get(i).copied(),
                    "auc" => output.auc().get(i).copied(),
                    _ => output.get(&format!("accum_c[{i}]")),
                };
                actual.insert(key.as_str(), quantity, value);
            }
        }
        _ => {
            for (_, name, _) in expected.iter() {
                actual.insert("scalar", name, output.get(name));
            }
        }
    }
    let report = compare_tables(&expected, &actual, Tolerance::MODEL_VALUES);
    assert!(report.is_ok(), "{case_name} {quantity}: {report}");
}

macro_rules! schedule_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::check;
                #[test]
                fn concentration() { check(concat!("model_md_", stringify!($case)), "conc"); }
                #[test]
                fn auc() { check(concat!("model_md_", stringify!($case)), "auc"); }
                #[test]
                fn scalars() { check(concat!("model_md_", stringify!($case)), "scalar"); }
            }
        )*
    };
}

macro_rules! steady_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::check;
                #[test]
                fn concentration() { check(concat!("model_md_", stringify!($case)), "conc"); }
                #[test]
                fn auc() { check(concat!("model_md_", stringify!($case)), "auc"); }
                #[test]
                fn accumulation_by_time() { check(concat!("model_md_", stringify!($case)), "accum_c"); }
                #[test]
                fn scalars() { check(concat!("model_md_", stringify!($case)), "scalar"); }
            }
        )*
    };
}

// Generated by oracle/scripts/models_md.R: the case lists, the invocations of the two macros above,
// and the constants SCHEDULE_CASES and STEADY_CASES used by the guard below.
include!("oracle_models_md.cases");

/// Guards the guard: every case file of the multiple-dosing oracle is run by a test above, and every
/// case named above exists.
#[test]
fn every_md_case_has_tests() {
    let on_disk = list_md_cases().unwrap();
    let mut listed: Vec<String> = SCHEDULE_CASES
        .iter()
        .chain(STEADY_CASES.iter())
        .map(|c| format!("model_md_{c}"))
        .collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "the cases on disk and the cases tested differ"
    );
}

// ---------------------------------------------------------------- errors (MOD-MD-12)

/// Every case of the group must be refused, with a readable message that names what the oracle says
/// it must name.
fn refused(group: &str) {
    let cases: Vec<_> = load_md_errors()
        .unwrap()
        .into_iter()
        .filter(|c| c.group == group)
        .collect();
    assert!(!cases.is_empty(), "no error case in group {group}");
    let mut failures = Vec::new();
    for c in &cases {
        let model = ModelId::from_id(&c.model)
            .unwrap_or_else(|| panic!("{}: unknown model id {:?}", c.id, c.model));
        let result = run(&ModelInput {
            model,
            dose: c.dose,
            params: c.parameters.clone(),
            times: c.times.clone(),
        });
        match result {
            Ok(_) => failures.push(format!("{}: accepted ({})", c.id, c.reason)),
            Err(e) => {
                let message = e.to_string().to_lowercase();
                if message.contains("unknown parameter") {
                    failures.push(format!(
                        "{}: unsupported dosing: refused as an unknown parameter, not for its reason ({})",
                        c.id, c.reason
                    ));
                    continue;
                }
                for alternatives in &c.message_contains {
                    if !alternatives.iter().any(|w| message.contains(w.as_str())) {
                        failures.push(format!(
                            "{}: message {message:?} contains none of {alternatives:?} ({})",
                            c.id, c.reason
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn errors_tau() {
    refused("tau");
}
#[test]
fn errors_tau_below_the_duration() {
    refused("tau_below_dur");
}
#[test]
fn errors_times_outside_the_interval() {
    refused("times");
}
#[test]
fn errors_n_doses() {
    refused("n_doses");
}
#[test]
fn errors_schedule_records() {
    refused("schedule");
}
#[test]
fn errors_lag() {
    refused("lag");
}
#[test]
fn errors_domain() {
    refused("domain");
}

// ---------------------------------------------------------------- properties and conventions

fn run_case(name: &str) -> (MdCase, caladrius_models::ModelOutput) {
    let loaded = load_md_case(name).unwrap();
    let output = run(&input_of(&loaded))
        .unwrap_or_else(|e| panic!("{name}: unsupported dosing: the engine refuses the regimen: {e}"));
    (loaded, output)
}

/// MOD-MD-07: a regular regimen of one dose is the single dose.
#[test]
fn one_regular_dose_is_the_single_dose() {
    for model in ["pk1.iv_bolus", "pk1.oral_1_lag", "pk2.oral_0", "pk2.iv_infusion"] {
        let name = format!("model_md_{}_regular_n1", model.replace('.', "_"));
        let (loaded, output) = run_case(&name);
        let mut input = input_of(&loaded);
        input.params.remove("tau");
        input.params.remove("n_doses");
        let plain = run(&input).unwrap();
        let mut expected = Table::new();
        let mut actual = Table::new();
        for (i, key) in loaded.case.time_keys.iter().enumerate() {
            expected.insert(key.as_str(), "conc", plain.conc().get(i).copied());
            actual.insert(key.as_str(), "conc", output.conc().get(i).copied());
        }
        let report = compare_tables(&expected, &actual, Tolerance::MODEL_VALUES);
        assert!(report.is_ok(), "{name}: {report}");
    }
}

/// MOD-MD-03: at steady state a bolus profile drops by D/V at the dose, C_ss(0) - C_ss(tau) = D/V.
#[test]
fn the_steady_state_bolus_jumps_by_d_over_v() {
    let (loaded, output) = run_case("model_md_pk1_iv_bolus_ss_tau_6");
    let times = &loaded.case.times;
    let first = times.iter().position(|&t| t == 0.0).unwrap();
    let last = times.iter().position(|&t| t == 6.0).unwrap();
    let jump = output.conc()[first] - output.conc()[last];
    let mut expected = Table::new();
    let mut actual = Table::new();
    expected.insert("jump", "value", Some(100.0 / 10.0));
    actual.insert("jump", "value", Some(jump));
    let report = compare_tables(&expected, &actual, Tolerance::MODEL_VALUES);
    assert!(report.is_ok(), "{report}");
}

/// OM-17 (e): where S-36's printed branch for the interval before the end of the lag differs from the
/// oracle (an earlier dose not yet in its decay phase), the engine follows MOD-MD-05 and must not
/// return the printed value.
#[test]
fn the_engine_follows_the_shift_rule_not_the_printed_branch() {
    let mut examined = 0;
    for name in list_md_cases().unwrap() {
        let loaded = load_md_case(&name).unwrap();
        let Some(printed) = loaded.printed_form.clone().filter(|p| p.differs_from_oracle) else {
            continue;
        };
        let output = run(&input_of(&loaded)).unwrap_or_else(|e| {
            panic!("{name}: unsupported dosing: the engine refuses the regimen: {e}")
        });
        for (t, value) in printed.times_that_differ.iter().zip(&printed.printed_values) {
            let i = loaded
                .case
                .times
                .iter()
                .position(|x| x.to_bits() == t.to_bits())
                .unwrap();
            let engine = output.conc()[i];
            assert!(
                (engine - value).abs() > 1e-9 * value.abs(),
                "{name}: at s = {t} the engine returns {engine}, the printed branch of S-36"
            );
            examined += 1;
        }
    }
    assert!(examined > 0, "no case where the printed branch differs");
}
