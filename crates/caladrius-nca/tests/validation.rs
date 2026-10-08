#![allow(clippy::panic)] // test code: helpers panic on purpose
//! Regression tests for every invalid input of golden rule 6 (`specs/nca.md` section 2.4, NCA-DAT-02,
//! DAT-04, DAT-11, section 9). Each one must give a readable error that says what to fix, never a
//! panic and never a number.

use caladrius_nca::{
    BlqAction, BlqPolicy, LambdaZManual, LambdaZOptions, MissingPolicy, NcReason, NcaError,
    NcaInput, NcaOptions, NegativePolicy, ParamValue, Route, run,
};

fn valid() -> NcaInput {
    NcaInput {
        time: vec![0.0, 1.0, 2.0, 4.0, 8.0],
        conc: vec![0.0, 6.0, 5.0, 3.0, 1.0],
        dose: 100.0,
        route: Route::Extravascular,
        options: NcaOptions::default(),
    }
}

/// Runs `input`, asserts the error is `expected`, and that its message contains `hint`.
#[track_caller]
fn assert_error(input: &NcaInput, expected: NcaError, hint: &str) {
    match run(input) {
        Ok(_) => panic!("expected {expected:?}, the run succeeded"),
        Err(e) => {
            assert_eq!(e, expected);
            let message = e.to_string();
            assert!(message.contains(hint), "message {message:?} lacks {hint:?}");
        }
    }
}

#[test]
fn the_valid_baseline_runs() {
    assert!(run(&valid()).is_ok());
}

#[test]
fn lengths_differ() {
    let mut i = valid();
    i.conc.pop();
    assert_error(
        &i,
        NcaError::LengthMismatch { times: 5, concs: 4 },
        "one concentration per time",
    );
}

#[test]
fn empty_profile() {
    let mut i = valid();
    i.time.clear();
    i.conc.clear();
    assert_error(&i, NcaError::EmptyProfile, "at least one");
}

#[test]
fn nan_time() {
    let mut i = valid();
    i.time[2] = f64::NAN;
    assert_error(&i, NcaError::NonFiniteTime { index: 2 }, "point 3");
}

#[test]
fn infinite_time() {
    let mut i = valid();
    i.time[4] = f64::INFINITY;
    assert_error(&i, NcaError::NonFiniteTime { index: 4 }, "point 5");
    i.time[4] = f64::NEG_INFINITY;
    assert_error(
        &i,
        NcaError::NonFiniteTime { index: 4 },
        "not a finite number",
    );
}

#[test]
fn duplicated_time() {
    let mut i = valid();
    i.time[2] = 1.0;
    assert_error(
        &i,
        NcaError::DuplicateTime {
            index: 2,
            time: 1.0,
        },
        "remove or merge",
    );
}

#[test]
fn unsorted_time() {
    let mut i = valid();
    i.time = vec![0.0, 2.0, 1.0, 4.0, 8.0];
    assert_error(
        &i,
        NcaError::UnsortedTime {
            index: 2,
            time: 1.0,
            previous: 2.0,
        },
        "sort the profile",
    );
}

#[test]
fn infinite_concentration() {
    let mut i = valid();
    i.conc[1] = f64::INFINITY;
    assert_error(
        &i,
        NcaError::InfiniteConcentration {
            index: 1,
            time: 1.0,
        },
        "infinite",
    );
    i.conc[1] = f64::NEG_INFINITY;
    assert_error(
        &i,
        NcaError::InfiniteConcentration {
            index: 1,
            time: 1.0,
        },
        "leave it empty",
    );
}

#[test]
fn nan_concentration_is_missing_not_an_error() {
    // NCA-DAT-03.
    let mut i = valid();
    i.conc[2] = f64::NAN;
    assert!(run(&i).is_ok());
}

#[test]
fn negative_concentration_under_the_default_policy() {
    let mut i = valid();
    i.conc[3] = -0.2;
    assert_error(
        &i,
        NcaError::NegativeConcentration {
            index: 3,
            time: 4.0,
            value: -0.2,
        },
        "below the limit of quantification",
    );
    i.options.negative = NegativePolicy::Allow;
    assert!(run(&i).is_ok());
    i.options.negative = NegativePolicy::SetZero;
    assert!(run(&i).is_ok());
}

#[test]
fn zero_concentrations_are_valid() {
    // NCA-DAT-05: zero is BLQ, handled by the policy.
    let mut i = valid();
    i.conc = vec![0.0; 5];
    assert!(run(&i).is_ok());
}

#[test]
fn missing_or_invalid_dose_only_blanks_dose_dependent_parameters() {
    // NCA-DAT-10, DAT-11: the run goes on; the reason says what to fix.
    for (dose, reason) in [
        (f64::NAN, NcReason::DoseMissing),
        (0.0, NcReason::InvalidDose),
        (-5.0, NcReason::InvalidDose),
        (f64::INFINITY, NcReason::InvalidDose),
    ] {
        let mut i = valid();
        i.dose = dose;
        let r = run(&i).unwrap_or_else(|e| panic!("dose {dose}: {e}"));
        for name in [
            "cl.obs",
            "cl.pred",
            "vz.obs",
            "vz.pred",
            "cmax.dn",
            "aucinf.obs.dn",
        ] {
            assert_eq!(
                r.parameter(name),
                Some(ParamValue::NotCalculated(reason)),
                "dose {dose}: {name}"
            );
        }
        for name in ["cmax", "auclast", "lambda.z", "aucinf.obs", "mrt.obs"] {
            assert!(r.get(name).is_some(), "dose {dose}: {name}");
        }
        assert!(reason.to_string().contains("dose"));
    }
}

#[test]
fn invalid_infusion_duration() {
    for duration in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut i = valid();
        i.route = Route::IvInfusion { duration };
        match run(&i) {
            Err(NcaError::InvalidInfusionDuration { value }) => {
                assert!(value.to_bits() == duration.to_bits());
            }
            other => panic!("duration {duration}: expected InvalidInfusionDuration, got {other:?}"),
        }
    }
    let mut i = valid();
    i.route = Route::IvInfusion { duration: 0.5 };
    assert!(run(&i).is_ok());
}

#[track_caller]
fn assert_invalid_option(options: NcaOptions, option: &str) {
    let mut i = valid();
    i.options = options;
    match run(&i) {
        Err(NcaError::InvalidOption {
            option: name,
            reason,
        }) => {
            assert_eq!(name, option);
            assert!(!reason.is_empty());
        }
        other => panic!("{option}: expected InvalidOption, got {other:?}"),
    }
}

#[test]
fn invalid_substituted_concentrations() {
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        assert_invalid_option(
            NcaOptions {
                missing: MissingPolicy::Replace(value),
                ..NcaOptions::default()
            },
            "missing",
        );
        assert_invalid_option(
            NcaOptions {
                blq: BlqPolicy::Position {
                    first: BlqAction::Keep,
                    middle: BlqAction::Set(value),
                    last: BlqAction::Keep,
                },
                ..NcaOptions::default()
            },
            "blq.middle",
        );
        assert_invalid_option(
            NcaOptions {
                blq: BlqPolicy::Tmax {
                    before: BlqAction::Keep,
                    after: BlqAction::Set(value),
                },
                ..NcaOptions::default()
            },
            "blq.after",
        );
    }
}

#[test]
fn invalid_lambda_z_options() {
    for min_points in [0, 1, 2] {
        assert_invalid_option(
            NcaOptions {
                lambda_z: LambdaZOptions {
                    min_points,
                    ..LambdaZOptions::default()
                },
                ..NcaOptions::default()
            },
            "lambda_z.min_points",
        );
    }
    for factor in [f64::NAN, f64::INFINITY, -1e-4] {
        assert_invalid_option(
            NcaOptions {
                lambda_z: LambdaZOptions {
                    adj_r_squared_factor: factor,
                    ..LambdaZOptions::default()
                },
                ..NcaOptions::default()
            },
            "lambda_z.adj_r_squared_factor",
        );
    }
}

#[test]
fn every_error_message_names_a_fix() {
    // Golden rule 6 and AGENTS.md section 7: every error says what to fix.
    let errors = [
        NcaError::LengthMismatch { times: 2, concs: 1 },
        NcaError::EmptyProfile,
        NcaError::NonFiniteTime { index: 0 },
        NcaError::DuplicateTime {
            index: 1,
            time: 1.0,
        },
        NcaError::UnsortedTime {
            index: 1,
            time: 1.0,
            previous: 2.0,
        },
        NcaError::InfiniteConcentration {
            index: 0,
            time: 0.0,
        },
        NcaError::NegativeConcentration {
            index: 0,
            time: 0.0,
            value: -1.0,
        },
        NcaError::InvalidInfusionDuration { value: 0.0 },
    ];
    for e in errors {
        let message = e.to_string();
        assert!(
            message.contains("; "),
            "{message:?} does not say what to fix"
        );
        let source: &dyn std::error::Error = &e;
        assert!(source.source().is_none());
    }
}

#[test]
fn lambda_z_times_must_be_sample_times() {
    // NCA-LZ-08, LZ-09: a time that is not a sample is a typo, not a silent no-op.
    let mut options = NcaOptions::default();
    options.lambda_z_selection.exclude = vec![3.0];
    assert_invalid_option(options, "lambda_z_selection.exclude");
    let mut options = NcaOptions::default();
    options.lambda_z_selection.manual = Some(LambdaZManual::Times(vec![2.0, 4.5]));
    assert_invalid_option(options, "lambda_z_selection.manual");
}

#[test]
fn lambda_z_manual_range_must_be_ordered_and_finite() {
    for (start, end) in [(4.0, 2.0), (f64::NAN, 8.0), (2.0, f64::INFINITY)] {
        let mut options = NcaOptions::default();
        options.lambda_z_selection.manual = Some(LambdaZManual::Range { start, end });
        assert_invalid_option(options, "lambda_z_selection.manual");
    }
}

#[test]
fn lambda_z_manual_point_without_a_quantified_value() {
    // NCA-LZ-08: zero (BLQ) points cannot be used; the message says which one.
    let mut i = valid();
    i.conc = vec![0.0, 6.0, 5.0, 0.0, 1.0];
    i.options.lambda_z_selection.manual = Some(LambdaZManual::Times(vec![2.0, 4.0, 8.0]));
    match run(&i) {
        Err(NcaError::InvalidOption { option, reason }) => {
            assert_eq!(option, "lambda_z_selection.manual");
            assert!(reason.contains("time 4"), "{reason}");
        }
        other => panic!("expected InvalidOption, got {other:?}"),
    }
}
