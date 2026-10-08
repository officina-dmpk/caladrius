//! Unit tests of the AUC rules, the observed parameters and the data cleaning. Expected values are
//! computed by hand in the comments, or by the independent naive implementation of
//! `caladrius-testkit` (never by this crate). Rule ids refer to `specs/nca.md`.

use caladrius_testkit::Tolerance;
use caladrius_testkit::naive::{self, Rule};

use crate::*;

const LN2: f64 = std::f64::consts::LN_2;

fn input(time: &[f64], conc: &[f64], route: Route, options: NcaOptions) -> NcaInput {
    NcaInput {
        time: time.to_vec(),
        conc: conc.to_vec(),
        dose: 100.0,
        route,
        options,
    }
}

fn with_method(auc_method: AucMethod) -> NcaOptions {
    NcaOptions {
        auc_method,
        ..NcaOptions::default()
    }
}

fn ev(time: &[f64], conc: &[f64], options: NcaOptions) -> NcaResult {
    run(&input(time, conc, Route::Extravascular, options)).unwrap()
}

#[track_caller]
fn assert_close(result: &NcaResult, name: &str, expected: f64) {
    let actual = result
        .get(name)
        .unwrap_or_else(|| panic!("{name} is not calculated: {:?}", result.parameter(name)));
    assert!(
        Tolerance::NCA_VS_PKNCA.accepts(actual, expected),
        "{name}: expected {expected}, got {actual}"
    );
}

#[track_caller]
fn assert_nc(result: &NcaResult, name: &str, reason: NcReason) {
    assert_eq!(
        result.parameter(name),
        Some(ParamValue::NotCalculated(reason)),
        "{name}"
    );
}

// ---- AUC and AUMC rules (NCA-AUC-02 to 09) ----

const T: [f64; 4] = [0.0, 1.0, 2.0, 4.0];
const C: [f64; 4] = [0.0, 10.0, 5.0, 2.5];

#[test]
fn linear_method_uses_trapezoids_everywhere() {
    // NCA-AUC-02, AUC-05, AUC-08. AUC: 1*10/2 + 1*15/2 + 2*7.5/2 = 5 + 7.5 + 7.5 = 20.
    // AUMC: 1*(0 + 10)/2 + 1*(10 + 10)/2 + 2*(10 + 10)/2 = 5 + 10 + 20 = 35.
    let r = ev(&T, &C, with_method(AucMethod::Linear));
    assert_close(&r, "auclast", 20.0);
    assert_close(&r, "aumclast", 35.0);
    assert_close(&r, "aucall", 20.0);
    assert_close(&r, "aumcall", 35.0);
}

#[test]
fn lin_up_log_down_uses_log_on_falling_positive_segments() {
    // NCA-AUC-03, AUC-06, AUC-08. Rising 0 -> 10: linear, 5. Falling halvings: (C1 - C2) dt / ln 2,
    // 5/ln2 + 5/ln2. AUMC of the log segments: (t1 C1 - t2 C2)/k + (C1 - C2)/k^2 with k = ln2/dt:
    // 0 + 5/ln2^2, then 0 + 2.5 * 4/ln2^2; total 5 + 15/ln2^2.
    let r = ev(&T, &C, NcaOptions::default());
    assert_close(&r, "auclast", 5.0 + 10.0 / LN2);
    assert_close(&r, "aumclast", 5.0 + 15.0 / (LN2 * LN2));
}

#[test]
fn lin_log_is_linear_up_to_tmax_then_log_both_ways() {
    // NCA-AUC-07. Tmax = 1. [0,1] ends at Tmax: linear (1 + 4)/2 = 2.5. [1,2] falls 4 -> 2: 2/ln2.
    // [2,3] rises 2 -> 3 after Tmax: log, (2 - 3)/ln(2/3) = 1/ln(1.5).
    let t = [0.0, 1.0, 2.0, 3.0];
    let c = [1.0, 4.0, 2.0, 3.0];
    let r = ev(&t, &c, with_method(AucMethod::LinLog));
    assert_close(&r, "auclast", 2.5 + 2.0 / LN2 + 1.0 / 1.5_f64.ln());
    // Under lin-up/log-down the rising segment after Tmax stays linear: 2.5.
    let r = ev(&t, &c, NcaOptions::default());
    assert_close(&r, "auclast", 2.5 + 2.0 / LN2 + 2.5);
}

#[test]
fn log_segments_are_exact_on_a_mono_exponential() {
    // NCA-AUC-10 (a): C = 10 exp(-0.3 t). AUC(0, T) = 10 (1 - e^-kT)/k,
    // AUMC(0, T) = 10 (1 - e^-kT (1 + kT))/k^2.
    let k: f64 = 0.3;
    let t = [0.0, 0.5, 1.0, 2.0, 4.0, 8.0, 12.0];
    let c: Vec<f64> = t.iter().map(|t| 10.0 * (-k * t).exp()).collect();
    let r = ev(&t, &c, NcaOptions::default());
    let end = 12.0;
    let decay = (-k * end).exp();
    assert_close(&r, "auclast", 10.0 * (1.0 - decay) / k);
    assert_close(
        &r,
        "aumclast",
        10.0 * (1.0 - decay * (1.0 + k * end)) / (k * k),
    );
}

#[test]
fn areas_agree_with_the_naive_implementation() {
    // Independent computation (AGENTS.md section 5, source 3) on a profile with a plateau, a rise
    // after a fall and a non-zero first sample.
    let t = [
        0.0, 0.25, 0.57, 1.12, 2.02, 3.82, 5.1, 7.03, 9.05, 12.12, 24.37,
    ];
    let c = [
        0.74, 2.84, 6.57, 10.5, 9.66, 8.58, 8.58, 7.47, 7.9, 5.94, 3.28,
    ];
    for (method, rule) in [
        (AucMethod::Linear, Rule::Linear),
        (AucMethod::LinUpLogDown, Rule::LinUpLogDown),
    ] {
        let r = ev(&t, &c, with_method(method));
        assert_close(&r, "auclast", naive::auc(rule, &t, &c).unwrap());
        assert_close(&r, "aumclast", naive::aumc(rule, &t, &c).unwrap());
    }
}

#[test]
fn log_down_never_exceeds_linear() {
    // NCA-AUC-10 (b).
    let t = [0.0, 1.0, 2.0, 3.0, 6.0, 12.0];
    let c = [0.0, 8.0, 6.0, 7.0, 3.0, 0.5];
    let lin = ev(&t, &c, with_method(AucMethod::Linear));
    let log = ev(&t, &c, NcaOptions::default());
    assert!(log.get("auclast").unwrap() <= lin.get("auclast").unwrap());
    assert!(log.get("auclast").unwrap() <= log.get("aucall").unwrap());
}

#[test]
fn aucall_adds_one_linear_segment_to_the_first_zero_after_tlast() {
    // NCA-AUC-04, AUC-09. [0,1] falls 4 -> 2: log, 2/ln2. Tlast = 1. AUCall adds 1*(2 + 0)/2 = 1,
    // AUMCall adds 1*(1*2 + 2*0)/2 = 1. The trailing zero at t = 3 is not reached.
    let t = [0.0, 1.0, 2.0, 3.0];
    let c = [4.0, 2.0, 0.0, 0.0];
    let r = ev(&t, &c, NcaOptions::default());
    assert_close(&r, "tlast", 1.0);
    assert_close(&r, "auclast", 2.0 / LN2);
    assert_close(&r, "aucall", 2.0 / LN2 + 1.0);
    // AUMC of the log segment: (0*4 - 1*2)/ln2 + 2/ln2^2.
    let aumclast = -2.0 / LN2 + 2.0 / (LN2 * LN2);
    assert_close(&r, "aumclast", aumclast);
    assert_close(&r, "aumcall", aumclast + 1.0);
}

#[test]
fn equal_positive_neighbours_use_the_linear_formula() {
    // NCA-AUC-04: C1 = C2 > 0 is linear (the limit of the log formula), no division by ln 1 = 0.
    let r = ev(&[0.0, 2.0, 4.0], &[5.0, 5.0, 5.0], NcaOptions::default());
    assert_close(&r, "auclast", 20.0);
    assert_close(&r, "aumclast", 40.0);
}

// ---- Observed parameters (NCA-OBS-01 to 03, OBS-06) ----

#[test]
fn observed_parameters_of_a_simple_profile() {
    let r = ev(
        &[0.0, 1.0, 2.0, 4.0, 8.0],
        &[0.0, 0.0, 6.0, 3.0, 0.0],
        NcaOptions::default(),
    );
    assert_close(&r, "cmax", 6.0);
    assert_close(&r, "tmax", 2.0);
    // NCA-OBS-06: the first positive concentration, later than the first sample.
    assert_close(&r, "tfirst", 2.0);
    assert_close(&r, "tlast", 4.0);
    assert_close(&r, "clast.obs", 3.0);
    // C0 is reported for an IV bolus only.
    assert_nc(&r, "c0", NcReason::NotApplicableToRoute);
}

#[test]
fn tmax_tie_rule_is_an_option() {
    // NCA-OBS-02.
    let t = [0.0, 1.0, 2.0, 3.0];
    let c = [0.0, 5.0, 5.0, 1.0];
    assert_close(&ev(&t, &c, NcaOptions::default()), "tmax", 1.0);
    let last = NcaOptions {
        tmax_tie: TmaxTie::Last,
        ..NcaOptions::default()
    };
    assert_close(&ev(&t, &c, last), "tmax", 2.0);
}

#[test]
fn terminal_phase_parameters_are_not_computed_yet() {
    let r = ev(&T, &C, NcaOptions::default());
    for name in ["lambda.z", "half.life", "aucinf.obs", "cl.obs", "mrt.obs"] {
        assert_eq!(r.get(name), None, "{name}");
        assert_eq!(r.parameter(name), None, "{name}");
    }
}

// ---- Degenerate profiles (NCA-DAT-09) ----

#[test]
fn one_point_gives_no_area() {
    let r = ev(&[0.0], &[3.0], NcaOptions::default());
    assert_close(&r, "cmax", 3.0);
    assert_close(&r, "tlast", 0.0);
    for name in ["auclast", "aucall", "aumclast", "aumcall"] {
        assert_nc(&r, name, NcReason::SinglePoint);
    }
}

#[test]
fn all_zero_profile() {
    let r = ev(&[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0], NcaOptions::default());
    assert_close(&r, "cmax", 0.0);
    assert_close(&r, "clast.obs", 0.0);
    for name in ["auclast", "aucall", "aumclast", "aumcall"] {
        assert_close(&r, name, 0.0);
    }
    for name in ["tmax", "tlast", "tfirst"] {
        assert_nc(&r, name, NcReason::NoPositiveConcentration);
    }
}

#[test]
fn all_missing_profile_is_not_an_error_but_nothing_is_calculated() {
    // NCA-DAT-03 then NCA-DAT-09: no point left.
    let r = ev(&[0.0, 1.0], &[f64::NAN, f64::NAN], NcaOptions::default());
    for name in ["cmax", "tmax", "tlast", "clast.obs", "auclast", "aumclast"] {
        assert_nc(&r, name, NcReason::NoDataAfterCleaning);
    }
    assert_eq!(r.removed().len(), 2);
    assert!(r.profile().is_empty());
}

// ---- Missing and negative values (NCA-DAT-03, DAT-04) ----

#[test]
fn missing_values_are_dropped_or_replaced() {
    let t = [0.0, 1.0, 2.0, 3.0];
    let c = [0.0, 4.0, f64::NAN, 1.0];
    let r = ev(&t, &c, with_method(AucMethod::Linear));
    // Dropped: [1,3] is one segment, 2*(4 + 1)/2 = 5; total 2 + 5 = 7.
    assert_close(&r, "auclast", 7.0);
    assert_eq!(
        r.removed(),
        &[RemovedPoint {
            index: 2,
            time: 2.0,
            reason: RemovalReason::Missing
        }]
    );
    let replace = NcaOptions {
        missing: MissingPolicy::Replace(2.0),
        ..with_method(AucMethod::Linear)
    };
    // Replaced by 2: 2 + 3 + 1.5 = 6.5.
    let r = ev(&t, &c, replace);
    assert_close(&r, "auclast", 6.5);
    assert_eq!(
        r.profile().get(2).map(|p| p.origin),
        Some(PointOrigin::MissingReplaced)
    );
}

#[test]
fn negative_values_allowed_enter_linearly_and_are_never_tlast() {
    // NCA-DAT-04 `allow`. [0,1]: 2; [1,2] 4 -> -1 linear under lin-up/log-down: 1.5; [2,3]: 0.5.
    let t = [0.0, 1.0, 2.0, 3.0, 4.0];
    let c = [0.0, 4.0, -1.0, 2.0, -0.5];
    let allow = NcaOptions {
        negative: NegativePolicy::Allow,
        ..NcaOptions::default()
    };
    let r = ev(&t, &c, allow);
    assert_close(&r, "tlast", 3.0);
    assert_close(&r, "clast.obs", 2.0);
    assert_close(&r, "auclast", 2.0 + 1.5 + 0.5);
}

#[test]
fn negative_values_set_to_zero_follow_the_blq_policy() {
    // NCA-DAT-04 `set_zero`: the middle zero is then dropped by the default BLQ policy.
    let set_zero = NcaOptions {
        negative: NegativePolicy::SetZero,
        ..with_method(AucMethod::Linear)
    };
    let r = ev(&[0.0, 1.0, 2.0, 3.0], &[0.0, 4.0, -1.0, 2.0], set_zero);
    // [0,1]: 2; [1,3]: 2*(4 + 2)/2 = 6.
    assert_close(&r, "auclast", 8.0);
    assert_eq!(
        r.removed().first().map(|p| p.reason),
        Some(RemovalReason::Blq)
    );
}

// ---- BLQ policy (NCA-DAT-05 to 07) ----

const BT: [f64; 6] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
const BC: [f64; 6] = [0.0, 5.0, 0.0, 4.0, 2.0, 0.0];

#[test]
fn default_blq_policy_keeps_first_and_last_and_drops_middle() {
    let r = ev(&BT, &BC, with_method(AucMethod::Linear));
    let kept: Vec<f64> = r.profile().iter().map(|p| p.time).collect();
    assert_eq!(kept, vec![0.0, 1.0, 3.0, 4.0, 5.0]);
    // [0,1]: 2.5; [1,3] across the gap: 2*(5 + 4)/2 = 9; [3,4]: 3. AUCall adds [4,5]: 1.
    assert_close(&r, "auclast", 14.5);
    assert_close(&r, "aucall", 15.5);
    assert_close(&r, "tlast", 4.0);
}

#[test]
fn blq_policy_by_tmax() {
    // Tmax = 1: the zero at 0 is before (kept), the zeros at 2 and 5 are after (dropped).
    let tmax = NcaOptions {
        blq: BlqPolicy::Tmax {
            before: BlqAction::Keep,
            after: BlqAction::Drop,
        },
        ..with_method(AucMethod::Linear)
    };
    let r = ev(&BT, &BC, tmax);
    let kept: Vec<f64> = r.profile().iter().map(|p| p.time).collect();
    assert_eq!(kept, vec![0.0, 1.0, 3.0, 4.0]);
    // Nothing after Tlast: AUCall = AUClast.
    assert_close(&r, "aucall", 14.5);
}

#[test]
fn blq_values_set_to_a_number_are_never_tlast() {
    // NCA-DAT-06 `set`, NCA-DAT-07: Tlast and Clast depend only on quantified values.
    let half = NcaOptions {
        blq: BlqPolicy::all(BlqAction::Set(0.5)),
        ..with_method(AucMethod::Linear)
    };
    let r = ev(&BT, &BC, half);
    assert_close(&r, "tlast", 4.0);
    assert_close(&r, "clast.obs", 2.0);
    assert_close(&r, "tfirst", 1.0);
    // [0,1]: 2.75; [1,2]: 2.75; [2,3]: 2.25; [3,4]: 3. AUCall adds 1*(2 + 0.5)/2 = 1.25.
    assert_close(&r, "auclast", 10.75);
    assert_close(&r, "aucall", 12.0);
    assert_eq!(
        r.profile().last().map(|p| p.origin),
        Some(PointOrigin::BlqSet)
    );
}

#[test]
fn dropping_every_blq_point_still_starts_at_the_dose_time() {
    // NCA-DAT-06 drop + NCA-DAT-08: the leading zero is dropped, the start policy puts it back.
    let r = ev(
        &BT,
        &BC,
        NcaOptions {
            blq: BlqPolicy::all(BlqAction::Drop),
            ..with_method(AucMethod::Linear)
        },
    );
    assert_eq!(
        r.profile().first().map(|p| p.origin),
        Some(PointOrigin::InsertedStart)
    );
    assert_close(&r, "auclast", 14.5);
}

// ---- Start of the profile and C0 (NCA-DAT-08, IV-01) ----

#[test]
fn start_policies_for_a_profile_without_a_sample_at_dose_time() {
    let t = [1.0, 2.0];
    let c = [4.0, 2.0];
    let none = NcaOptions {
        start: StartPolicy::None,
        ..NcaOptions::default()
    };
    let r = ev(&t, &c, none);
    assert_nc(&r, "auclast", NcReason::NoStartConcentration);
    assert_close(&r, "cmax", 4.0);
    // Zero (and C0 for an extravascular route): [0,1] linear 2, [1,2] log 2/ln2.
    for start in [StartPolicy::Zero, StartPolicy::C0] {
        let r = ev(
            &t,
            &c,
            NcaOptions {
                start,
                ..NcaOptions::default()
            },
        );
        assert_close(&r, "auclast", 2.0 + 2.0 / LN2);
    }
}

fn bolus(time: &[f64], conc: &[f64]) -> NcaResult {
    run(&input(time, conc, Route::IvBolus, NcaOptions::default())).unwrap()
}

#[test]
fn c0_of_an_iv_bolus_by_back_extrapolation() {
    // NCA-IV-01 method 2: k = ln(8/4)/1, C0 = 8 e^(k*1) = 16. NCA-IV-02: the point (0, 16) is
    // integrated, log down: 8/ln2 + 4/ln2; Cmax and Tmax stay observed.
    let r = bolus(&[1.0, 2.0], &[8.0, 4.0]);
    assert_close(&r, "c0", 16.0);
    assert_close(&r, "cmax", 8.0);
    assert_close(&r, "tmax", 1.0);
    assert_close(&r, "tfirst", 1.0);
    assert_close(&r, "auclast", 12.0 / LN2);
}

#[test]
fn c0_of_an_iv_bolus_falls_back_to_the_first_sample() {
    // NCA-IV-01 method 3, the first pair rises.
    let r = bolus(&[1.0, 2.0, 3.0], &[4.0, 6.0, 3.0]);
    assert_close(&r, "c0", 4.0);
    // A zero second value is unusable as well.
    let r = bolus(&[1.0, 2.0], &[4.0, 0.0]);
    assert_close(&r, "c0", 4.0);
}

#[test]
fn c0_of_an_iv_bolus_with_a_sample_at_dose_time() {
    // Method 1 when positive; a zero at the dose time is used as it is in the profile (NCA-DAT-08)
    // but C0 comes from method 2: 8 * 2 = 16.
    assert_close(&bolus(&[0.0, 1.0, 2.0], &[20.0, 8.0, 4.0]), "c0", 20.0);
    let r = bolus(&[0.0, 1.0, 2.0], &[0.0, 8.0, 4.0]);
    assert_close(&r, "c0", 16.0);
    assert_eq!(r.profile().len(), 3);
}

#[test]
fn points_before_the_dose_are_left_out() {
    // NCA-DAT-01.
    let r = ev(&[-1.0, 0.0, 1.0], &[0.3, 0.0, 2.0], NcaOptions::default());
    assert_eq!(
        r.removed(),
        &[RemovedPoint {
            index: 0,
            time: -1.0,
            reason: RemovalReason::BeforeDose
        }]
    );
    assert_close(&r, "auclast", 1.0);
}

// ---- Options and results are data (golden rule 4) ----

#[test]
fn default_options_are_the_pknca_profile() {
    let o = NcaOptions::default();
    assert_eq!(o.auc_method, AucMethod::LinUpLogDown);
    assert_eq!(o.lambda_z.min_points, 3);
    assert!(!o.lambda_z.allow_tmax);
    assert_eq!(o.lambda_z.adj_r_squared_factor, 1e-4);
    assert_eq!(o.missing, MissingPolicy::Drop);
    assert_eq!(o.negative, NegativePolicy::Error);
    assert_eq!(o.tmax_tie, TmaxTie::First);
    // The one deliberate deviation from the PKNCA profile (spec O-08): see `StartPolicy::C0`.
    assert_eq!(o.start, StartPolicy::C0);
    assert_eq!(
        o.blq,
        BlqPolicy::Position {
            first: BlqAction::Keep,
            middle: BlqAction::Drop,
            last: BlqAction::Keep
        }
    );
}

#[test]
fn input_and_result_round_trip_through_json() {
    let i = input(
        &BT,
        &BC,
        Route::IvInfusion { duration: 0.5 },
        NcaOptions {
            blq: BlqPolicy::all(BlqAction::Set(0.5)),
            ..NcaOptions::default()
        },
    );
    let text = serde_json::to_string(&i).unwrap();
    assert_eq!(serde_json::from_str::<NcaInput>(&text).unwrap(), i);
    let r = run(&i).unwrap();
    let text = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<NcaResult>(&text).unwrap(), r);
    // Partial options fill in the defaults.
    let o: NcaOptions = serde_json::from_str(r#"{"auc_method":"linear"}"#).unwrap();
    assert_eq!(o, with_method(AucMethod::Linear));
}

// ---- Review fixes (T-004a review, findings 1 to 4) ----

#[test]
fn one_sample_after_the_dose_gives_no_area() {
    // NCA-DAT-09: the point inserted at the dose time is not a second observation.
    for route in [Route::Extravascular, Route::IvBolus] {
        let r = run(&input(&[2.0], &[5.0], route, NcaOptions::default())).unwrap();
        for name in ["auclast", "aucall", "aumclast", "aumcall"] {
            assert_nc(&r, name, NcReason::SinglePoint);
        }
        assert_close(&r, "cmax", 5.0);
    }
}

#[test]
fn log_segment_with_an_extreme_ratio_stays_exact() {
    // NCA-AUC-03 with C1/C2 = 1e600 (the ratio overflows a double; ln C1 - ln C2 = 600 ln 10 does
    // not). AUC = (C1 - C2)/(600 ln 10); AUMC = (0 - C2)/k + (C1 - C2)/k^2 with k = 600 ln 10.
    let k = 600.0 * std::f64::consts::LN_10;
    let r = ev(&[0.0, 1.0], &[1e300, 1e-300], NcaOptions::default());
    assert_close(&r, "auclast", 1e300 / k);
    assert_close(&r, "aumclast", 1e300 / (k * k));
}

#[test]
fn missing_concentrations_travel_as_json_null() {
    // NCA-DAT-03 through JSON (golden rule 4): NaN is written as null and read back as NaN.
    let i = input(
        &[0.0, 1.0, 2.0],
        &[0.0, f64::NAN, 3.0],
        Route::Extravascular,
        NcaOptions::default(),
    );
    let text = serde_json::to_string(&i).unwrap();
    assert!(text.contains(r#""conc":[0.0,null,3.0]"#), "{text}");
    let back: NcaInput = serde_json::from_str(&text).unwrap();
    assert_eq!(back.time, i.time);
    assert!(back.conc.get(1).unwrap().is_nan());
    assert_eq!(back.conc.first(), Some(&0.0));
    assert_eq!(back.conc.get(2), Some(&3.0));

    let literal =
        r#"{"time":[0,1,2],"conc":[0,null,3],"dose":10,"route":"extravascular","options":{}}"#;
    let r = run(&serde_json::from_str::<NcaInput>(literal).unwrap()).unwrap();
    assert_eq!(
        r.removed().first().map(|p| p.reason),
        Some(RemovalReason::Missing)
    );
    // Texts for the other non-finite values; anything else is refused with a readable error.
    let texts =
        r#"{"time":[0,"inf"],"conc":["NaN","-inf"],"dose":"nan","route":"iv_bolus","options":{}}"#;
    let parsed: NcaInput = serde_json::from_str(texts).unwrap();
    assert_eq!(parsed.time.get(1), Some(&f64::INFINITY));
    assert!(parsed.conc.first().unwrap().is_nan());
    assert_eq!(parsed.conc.get(1), Some(&f64::NEG_INFINITY));
    assert!(parsed.dose.is_nan());
    let bad = r#"{"time":[0],"conc":["abc"],"dose":1,"route":"iv_bolus","options":{}}"#;
    let message = serde_json::from_str::<NcaInput>(bad)
        .unwrap_err()
        .to_string();
    assert!(message.contains("is not a number"), "{message}");
}

#[test]
fn non_finite_numbers_in_errors_serialize_as_text() {
    let e = NcaError::InvalidDose { value: f64::NAN };
    let text = serde_json::to_string(&e).unwrap();
    assert_eq!(text, r#"{"code":"invalid_dose","value":"NaN"}"#);
    match serde_json::from_str::<NcaError>(&text).unwrap() {
        NcaError::InvalidDose { value } => assert!(value.is_nan()),
        other => panic!("{other:?}"),
    }
    let e = NcaError::InvalidInfusionDuration {
        value: f64::NEG_INFINITY,
    };
    let text = serde_json::to_string(&e).unwrap();
    assert!(text.contains(r#""value":"-inf""#), "{text}");
    assert_eq!(serde_json::from_str::<NcaError>(&text).unwrap(), e);
}

#[test]
fn misspelled_fields_are_rejected() {
    // A typo must not silently fall back to a default.
    let options = [
        r#"{"auc_methd":"linear"}"#,
        r#"{"lambda_z":{"min_point":4}}"#,
        r#"{"blq":{"position":{"first":"keep","middle":"drop","lst":"keep"}}}"#,
    ];
    for text in options {
        let message = serde_json::from_str::<NcaOptions>(text)
            .unwrap_err()
            .to_string();
        assert!(message.contains("unknown field"), "{text}: {message}");
    }
    let route = serde_json::from_str::<Route>(r#"{"iv_infusion":{"duraton":1}}"#);
    assert!(route.is_err());
    let input = r#"{"time":[0],"conc":[1],"dose":1,"route":"iv_bolus","options":{},"dose_tme":0}"#;
    let message = serde_json::from_str::<NcaInput>(input)
        .unwrap_err()
        .to_string();
    assert!(message.contains("unknown field"), "{message}");
}
