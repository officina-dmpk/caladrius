#![allow(clippy::unwrap_used, clippy::panic)] // test code: helpers panic on purpose
//! Quality flags (`specs/nca.md` NCA-LZ-12b) and the T-013 review follow-ups (task T-015).
//! Profiles are the worked examples of `specs/nca.md` section 11 where possible.

use caladrius_nca::{
    BlqAction, BlqPolicy, LambdaZManual, LambdaZSelection, NcReason, NcaError, NcaInput,
    NcaOptions, NcaResult, ParamValue, QualityFlag, QualityThresholds, Route, run,
};

fn run_with(time: &[f64], conc: &[f64], route: Route, options: NcaOptions) -> NcaResult {
    run(&NcaInput {
        time: time.to_vec(),
        conc: conc.to_vec(),
        dose: 100.0,
        route,
        options,
    })
    .unwrap()
}

fn ev(time: &[f64], conc: &[f64], options: NcaOptions) -> NcaResult {
    run_with(time, conc, Route::Extravascular, options)
}

fn no_thresholds() -> NcaOptions {
    NcaOptions {
        quality: QualityThresholds {
            min_adj_r_squared: None,
            min_span_ratio: None,
            max_extrapolated_percent: None,
            min_points: None,
        },
        ..NcaOptions::default()
    }
}

/// W1: adjusted R² 0.637, span ratio 0.776, AUC extrapolated about 44 %.
const W1_T: [f64; 9] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 12.0, 24.0];
const W1_C: [f64; 9] = [0.0, 2.5, 3.0, 2.0, 1.5, 1.2, 1.1, 0.0, 0.0];

fn codes(result: &NcaResult) -> Vec<String> {
    result
        .flags()
        .iter()
        .map(|f| {
            let json = serde_json::to_value(f).unwrap();
            json["code"].as_str().unwrap().to_string()
        })
        .collect()
}

#[test]
fn w1_raises_fit_span_and_extrapolation_flags() {
    let r = ev(&W1_T, &W1_C, NcaOptions::default());
    assert_eq!(
        codes(&r),
        vec![
            "low_adjusted_r_squared",
            "short_span",
            "high_extrapolation",
            "high_extrapolation"
        ]
    );
    match r.flags().first() {
        Some(QualityFlag::LowAdjustedRSquared { value, threshold }) => {
            assert!((value - 0.6370368).abs() < 1e-6);
            assert_eq!(*threshold, 0.9);
        }
        other => panic!("{other:?}"),
    }
    // Every flag says what to check.
    for flag in r.flags() {
        assert!(!flag.to_string().is_empty());
    }
}

#[test]
fn flags_never_change_a_number() {
    let flagged = ev(&W1_T, &W1_C, NcaOptions::default());
    let silent = ev(&W1_T, &W1_C, no_thresholds());
    assert!(silent.flags().is_empty());
    assert_eq!(flagged.parameters(), silent.parameters());
    assert_eq!(flagged.lambda_z_candidates(), silent.lambda_z_candidates());
}

#[test]
fn a_clean_profile_has_no_flag() {
    // W2: exact mono-exponential, 5 points over 3.2 half-lives, 9 % extrapolated.
    let t = [0.0, 1.0, 2.0, 4.0, 8.0, 12.0];
    let c: Vec<f64> = t.iter().map(|t: &f64| 10.0 * (-0.2 * t).exp()).collect();
    let r = run_with(&t, &c, Route::IvBolus, NcaOptions::default());
    assert!(r.flags().is_empty(), "{:?}", r.flags());
}

#[test]
fn a_two_point_manual_fit_is_flagged() {
    let mut options = no_thresholds();
    options.quality.min_points = Some(3);
    options.lambda_z_selection.manual = Some(LambdaZManual::Times(vec![8.0, 24.0]));
    let r = ev(
        &[0.0, 1.0, 2.0, 4.0, 8.0, 24.0],
        &[0.0, 5.0, 8.0, 6.0, 4.0, 1.0],
        options,
    );
    assert!(r.get("lambda.z").is_some());
    assert_eq!(
        r.flags(),
        &[QualityFlag::FewPoints {
            value: 2,
            threshold: 3
        }]
    );
}

#[test]
fn replaced_values_in_the_terminal_phase_and_past_tlast_are_flagged() {
    let t = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let c = [0.0, 5.0, 4.0, 3.0, 2.0, 0.0, 0.0];
    let mut options = no_thresholds();
    options.blq = BlqPolicy::all(BlqAction::Set(0.5));
    let r = ev(&t, &c, options.clone());
    assert_eq!(
        r.flags(),
        &[
            QualityFlag::ReplacedPointsInLambdaZ { count: 2 },
            QualityFlag::AreaPastTlast {
                end: 6.0,
                tlast: 4.0
            },
        ]
    );
    // Excluded from λz: only the area flag remains.
    options.lambda_z_selection.exclude_replaced = true;
    let r = ev(&t, &c, options);
    assert_eq!(
        r.flags(),
        &[QualityFlag::AreaPastTlast {
            end: 6.0,
            tlast: 4.0
        }]
    );
}

#[test]
fn flags_and_thresholds_round_trip_through_json() {
    let r = ev(&W1_T, &W1_C, NcaOptions::default());
    for flag in r.flags() {
        let text = serde_json::to_string(flag).unwrap();
        assert_eq!(&serde_json::from_str::<QualityFlag>(&text).unwrap(), flag);
    }
    let back: NcaResult = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back.flags(), r.flags());
    // null turns a flag off; a typo is refused.
    let q: QualityThresholds = serde_json::from_str(r#"{"min_span_ratio":null}"#).unwrap();
    assert_eq!(q.min_span_ratio, None);
    assert_eq!(q.min_adj_r_squared, Some(0.9));
    assert!(serde_json::from_str::<QualityThresholds>(r#"{"min_span":2}"#).is_err());
}

#[test]
fn invalid_thresholds_are_refused() {
    for q in [
        QualityThresholds {
            min_span_ratio: Some(-1.0),
            ..QualityThresholds::default()
        },
        QualityThresholds {
            max_extrapolated_percent: Some(f64::NAN),
            ..QualityThresholds::default()
        },
    ] {
        let input = NcaInput {
            time: W1_T.to_vec(),
            conc: W1_C.to_vec(),
            dose: 100.0,
            route: Route::Extravascular,
            options: NcaOptions {
                quality: q,
                ..NcaOptions::default()
            },
        };
        match run(&input) {
            Err(NcaError::InvalidOption { option, .. }) => assert!(option.starts_with("quality.")),
            other => panic!("expected InvalidOption, got {other:?}"),
        }
    }
}

// ---- T-013 review follow-ups ----

#[test]
fn no_area_or_lambda_z_from_replaced_values_alone() {
    // Every value is BLQ, replaced by 5: no measured positive concentration.
    let options = NcaOptions {
        blq: BlqPolicy::all(BlqAction::Set(5.0)),
        ..NcaOptions::default()
    };
    let r = ev(&[0.0, 1.0, 2.0, 4.0], &[0.0, 0.0, 0.0, 0.0], options);
    for name in [
        "auclast",
        "aucall",
        "aumclast",
        "lambda.z",
        "aucinf.obs",
        "tlast",
    ] {
        assert_eq!(
            r.parameter(name),
            Some(ParamValue::NotCalculated(NcReason::NoPositiveConcentration)),
            "{name}"
        );
    }
    // Kept as zeros, the areas are 0 (NCA-DAT-09).
    let r = ev(&[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0], NcaOptions::default());
    assert_eq!(r.get("auclast"), Some(0.0));
}

#[test]
fn tlag_of_an_empty_or_single_sample_profile() {
    let r = ev(&[0.0, 1.0], &[f64::NAN, f64::NAN], NcaOptions::default());
    assert_eq!(
        r.parameter("tlag"),
        Some(ParamValue::NotCalculated(NcReason::NoDataAfterCleaning))
    );
    let r = ev(&[1.0], &[3.0], NcaOptions::default());
    assert_eq!(
        r.parameter("tlag"),
        Some(ParamValue::NotCalculated(NcReason::SinglePoint))
    );
}

#[test]
fn exclude_replaced_round_trips_and_typos_are_refused() {
    let selection = LambdaZSelection {
        exclude_replaced: true,
        ..LambdaZSelection::default()
    };
    let text = serde_json::to_string(&selection).unwrap();
    assert!(text.contains(r#""exclude_replaced":true"#), "{text}");
    assert_eq!(
        serde_json::from_str::<LambdaZSelection>(&text).unwrap(),
        selection
    );
    let typo = serde_json::from_str::<LambdaZSelection>(r#"{"exclude_replace":true}"#);
    assert!(typo.unwrap_err().to_string().contains("unknown field"));
}
