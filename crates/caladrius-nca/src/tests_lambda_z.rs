//! Unit tests of the terminal phase and the extrapolation (`specs/nca.md` sections 6 and 7).
//! Expected values come from the worked examples of `specs/nca.md` section 11 (W1, W2, W4, W6), or
//! from the independent naive regression of `caladrius-testkit` (never from this crate).

use caladrius_testkit::Tolerance;
use caladrius_testkit::naive;

use crate::*;

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

/// Every parameter that depends on λz (NCA-LZ-11).
const DEPENDANTS: [&str; 15] = [
    "lambda.z",
    "r.squared",
    "adj.r.squared",
    "lambda.z.time.first",
    "lambda.z.time.last",
    "lambda.z.n.points",
    "clast.pred",
    "half.life",
    "span.ratio",
    "aucinf.obs",
    "aucinf.pred",
    "aumcinf.obs",
    "aumcinf.pred",
    "aucpext.obs",
    "aumcpext.pred",
];

// ---- Worked examples ----

#[test]
fn w1_pknca_printed_example() {
    // W1: λz on 4 points (3, 4, 5, 8) with adjusted R² 0.637 (no R² floor, NCA-LZ-12).
    let t = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 12.0, 24.0];
    let c = [0.0, 2.5, 3.0, 2.0, 1.5, 1.2, 1.1, 0.0, 0.0];
    let r = ev(&t, &c, NcaOptions::default());
    assert_close(&r, "lambda.z", 0.1075592);
    assert_close(&r, "r.squared", 0.7580245);
    assert_close(&r, "adj.r.squared", 0.6370368);
    assert_close(&r, "lambda.z.n.points", 4.0);
    assert_close(&r, "lambda.z.time.first", 3.0);
    assert_close(&r, "lambda.z.time.last", 8.0);
    assert_close(&r, "clast.pred", 1.0216136);
    assert_close(&r, "half.life", 6.4443313);
    assert_close(&r, "span.ratio", 0.7758757);
    assert_close(&r, "auclast", 12.9965842);
    assert_close(&r, "aucall", 15.1965842);
    assert_close(&r, "aucinf.obs", 23.2235095);
    assert_close(&r, "aucinf.pred", 22.4947355);
    // Both candidates are returned, the larger one selected.
    let candidates = r.lambda_z_candidates();
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.n_points)
            .collect::<Vec<_>>(),
        vec![4]
    );
}

#[test]
fn w2_exact_mono_exponential_iv_bolus() {
    let t = [0.0, 1.0, 2.0, 4.0, 8.0, 12.0];
    let c: Vec<f64> = t.iter().map(|t: &f64| 10.0 * (-0.2 * t).exp()).collect();
    let r = run_with(&t, &c, Route::IvBolus, NcaOptions::default());
    assert_close(&r, "lambda.z", 0.2);
    assert_close(&r, "lambda.z.n.points", 5.0);
    assert_close(&r, "half.life", 3.4657359028);
    assert_close(&r, "aucinf.obs", 50.0);
    assert_close(&r, "aumcinf.obs", 250.0);
    assert_close(&r, "aucinf.pred", 50.0);
}

#[test]
fn w4_iv_bolus_without_a_sample_at_dose_time() {
    // The back-extrapolated C0 is integrated but never a regression point (NCA-LZ-13).
    let t = [0.25, 0.5, 1.0, 2.0, 4.0];
    let c = [8.0, 6.4, 4.1, 1.7, 0.29];
    let r = run_with(&t, &c, Route::IvBolus, NcaOptions::default());
    assert_close(&r, "c0", 10.0);
    assert_close(&r, "lambda.z", 0.88364213063);
    assert_close(&r, "lambda.z.n.points", 4.0);
    assert_close(&r, "lambda.z.time.first", 0.5);
    assert_close(&r, "r.squared", 0.99999862877);
    assert_close(&r, "clast.pred", 0.29004192056);
    assert_close(&r, "half.life", 0.78442070216);
    assert_close(&r, "auclast", 10.93647360532);
    assert_close(&r, "aucinf.obs", 11.26466076385);
    assert_close(&r, "aumcinf.obs", 12.73510240167);
    assert_close(&r, "aucpext.obs", 2.91342247585);
}

// ---- Selection rules (NCA-LZ-05 to 07, open items O-01 and O-02 settled by T-005) ----

const D_T: [f64; 9] = [0.0, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 24.0];
const D1_C: [f64; 9] = [0.0, 5.0, 9.0, 8.817, 6.294, 4.557, 3.466, 1.85, 0.274];
const D2_C: [f64; 9] = [0.0, 5.0, 9.0, 6.0, 4.0, 3.0, 2.0, 2.5, 3.0];

fn with_selection(selection: LambdaZSelection) -> NcaOptions {
    NcaOptions {
        lambda_z_selection: selection,
        ..NcaOptions::default()
    }
}

#[test]
fn d1_tolerance_reading_is_the_default() {
    let r = ev(&D_T, &D1_C, NcaOptions::default());
    assert_close(&r, "lambda.z.n.points", 3.0);
    assert_close(&r, "lambda.z", 0.15872853);
    // A wider tolerance admits every fit; the most points win.
    let mut wide = NcaOptions::default();
    wide.lambda_z.adj_r_squared_factor = 1e-3;
    let r = ev(&D_T, &D1_C, wide);
    assert_close(&r, "lambda.z.n.points", 6.0);
    assert_close(&r, "lambda.z", 0.15703919);
}

#[test]
fn d1_bonus_reading_on_request() {
    let r = ev(
        &D_T,
        &D1_C,
        with_selection(LambdaZSelection {
            tie_rule: LambdaZTieRule::Bonus,
            ..LambdaZSelection::default()
        }),
    );
    assert_close(&r, "lambda.z.n.points", 6.0);
    assert_close(&r, "lambda.z", 0.15703919);
}

#[test]
fn d2_positive_filter_after_selection_gives_no_terminal_phase() {
    // PKNCA order (default): the best fit rises, so λz is not calculated, and so is every
    // dependant (NCA-LZ-11); the areas to Tlast are still computed.
    let r = ev(&D_T, &D2_C, NcaOptions::default());
    for name in DEPENDANTS {
        assert_nc(&r, name, NcReason::NoValidFit);
    }
    assert!(r.get("auclast").is_some());
    assert_eq!(r.lambda_z_candidates().len(), 4);
    assert!(r.lambda_z_candidates().iter().all(|c| !c.selected));
    // Filter first: only the decreasing fits compete.
    let r = ev(
        &D_T,
        &D2_C,
        with_selection(LambdaZSelection {
            positive_filter_first: true,
            ..LambdaZSelection::default()
        }),
    );
    assert_close(&r, "lambda.z", 0.02068158);
    assert_close(&r, "lambda.z.n.points", 6.0);
}

#[test]
fn tmax_point_joins_the_candidates_only_when_allowed() {
    let r = ev(&D_T, &D1_C, NcaOptions::default());
    assert_eq!(r.lambda_z_candidates().len(), 4);
    let mut allow = NcaOptions::default();
    allow.lambda_z.allow_tmax = true;
    let r = ev(&D_T, &D1_C, allow);
    assert_eq!(r.lambda_z_candidates().len(), 5);
    assert_eq!(
        r.lambda_z_candidates().last().map(|c| c.time_first),
        Some(1.0)
    );
}

// ---- Not estimable (NCA-LZ-03, LZ-11) ----

#[test]
fn too_few_points_after_tmax() {
    let r = ev(
        &[0.0, 1.0, 2.0, 3.0],
        &[0.0, 5.0, 3.0, 1.0],
        NcaOptions::default(),
    );
    for name in DEPENDANTS {
        assert_nc(&r, name, NcReason::TooFewPoints);
    }
    assert!(r.get("auclast").is_some());
    assert!(r.lambda_z_candidates().is_empty());
    // Tmax is the last sample: no eligible point at all.
    let r = ev(
        &[0.0, 1.0, 2.0, 3.0],
        &[0.0, 1.0, 2.0, 3.0],
        NcaOptions::default(),
    );
    assert_nc(&r, "lambda.z", NcReason::TooFewPoints);
    // Every reason reads as a sentence.
    assert!(NcReason::TooFewPoints.to_string().contains("too few"));
    assert!(NcReason::NoValidFit.to_string().contains("slope"));
}

#[test]
fn points_during_an_infusion_are_not_eligible() {
    // NCA-LZ-02 rule 3: only points strictly after the end of the infusion (3.5 h).
    let t = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0];
    let c = [0.0, 10.0, 9.0, 8.0, 6.0, 4.0, 2.5];
    let r = run_with(
        &t,
        &c,
        Route::IvInfusion { duration: 3.5 },
        NcaOptions::default(),
    );
    assert_close(&r, "lambda.z.time.first", 4.0);
    assert!(r.lambda_z_candidates().iter().all(|c| c.time_first > 3.5));
    let r = ev(&t, &c, NcaOptions::default());
    assert!(r.lambda_z_candidates().iter().any(|c| c.time_first < 3.5));
}

#[test]
fn blq_values_set_to_a_number_enter_the_regression_unless_excluded() {
    // T-012 (PKNCA, open item O-07): a replaced value is an ordinary terminal-phase point; the
    // `exclude_replaced` option restores the NCA-LZ-02b reading.
    let t = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let c = [0.0, 5.0, 4.0, 3.0, 2.0, 0.0, 0.0];
    let options = NcaOptions {
        blq: BlqPolicy::all(BlqAction::Set(0.5)),
        ..NcaOptions::default()
    };
    let r = ev(&t, &c, options.clone());
    assert_close(&r, "lambda.z.time.last", 6.0);
    assert_close(&r, "tlast", 4.0);
    let mut excluded = options;
    excluded.lambda_z_selection.exclude_replaced = true;
    let r = ev(&t, &c, excluded);
    assert_close(&r, "lambda.z.time.last", 4.0);
}

// ---- Exclusions and manual selection (NCA-LZ-08, LZ-09), against the naive regression ----

const TH_T: [f64; 11] = [
    0.0, 0.25, 0.57, 1.12, 2.02, 3.82, 5.1, 7.03, 9.05, 12.12, 24.37,
];
const TH_C: [f64; 11] = [
    0.74, 2.84, 6.57, 10.5, 9.66, 8.58, 8.36, 7.47, 6.89, 5.94, 3.28,
];

/// λz and R² of the naive regression of ln C on t over the samples at `times`.
fn naive_fit(times: &[f64]) -> (f64, f64) {
    let (x, y): (Vec<f64>, Vec<f64>) = TH_T
        .iter()
        .zip(TH_C)
        .filter(|(t, _)| times.contains(t))
        .map(|(t, c)| (*t, c.ln()))
        .unzip();
    let fit = naive::linear_regression(&x, &y).unwrap();
    (-fit.slope, fit.r_squared)
}

#[test]
fn w8_default_selection_and_one_excluded_point() {
    let r = ev(&TH_T, &TH_C, NcaOptions::default());
    assert_close(&r, "lambda.z.time.first", 9.05);
    assert_close(&r, "lambda.z", naive_fit(&[9.05, 12.12, 24.37]).0);
    let r = ev(
        &TH_T,
        &TH_C,
        with_selection(LambdaZSelection {
            exclude: vec![12.12],
            ..LambdaZSelection::default()
        }),
    );
    assert_close(&r, "lambda.z.n.points", 4.0);
    assert_close(&r, "lambda.z.time.first", 5.1);
    assert_close(&r, "lambda.z", naive_fit(&[5.1, 7.03, 9.05, 24.37]).0);
}

#[test]
fn w8_manual_selection_by_times_or_range() {
    let after_3h = [3.82, 5.1, 7.03, 9.05, 12.12, 24.37];
    let (lambda_z, r_squared) = naive_fit(&after_3h);
    for manual in [
        LambdaZManual::Times(after_3h.to_vec()),
        LambdaZManual::Range {
            start: 3.0,
            end: 30.0,
        },
    ] {
        let r = ev(
            &TH_T,
            &TH_C,
            with_selection(LambdaZSelection {
                manual: Some(manual),
                ..LambdaZSelection::default()
            }),
        );
        assert_close(&r, "lambda.z.n.points", 6.0);
        assert_close(&r, "lambda.z", lambda_z);
        assert_close(&r, "r.squared", r_squared);
        assert_eq!(r.lambda_z_candidates().len(), 1);
    }
}

#[test]
fn manual_selection_with_two_points_or_a_rising_phase() {
    let manual = |times: &[f64]| {
        with_selection(LambdaZSelection {
            manual: Some(LambdaZManual::Times(times.to_vec())),
            ..LambdaZSelection::default()
        })
    };
    // Two points: λz is given, adjusted R² is not defined.
    let r = ev(&TH_T, &TH_C, manual(&[12.12, 24.37]));
    assert_close(&r, "lambda.z", naive_fit(&[12.12, 24.37]).0);
    assert_nc(&r, "adj.r.squared", NcReason::Undefined);
    // A rising phase: not estimable.
    let r = ev(&TH_T, &TH_C, manual(&[0.25, 0.57, 1.12]));
    assert_nc(&r, "lambda.z", NcReason::NoValidFit);
    // One point.
    let r = ev(&TH_T, &TH_C, manual(&[24.37]));
    assert_nc(&r, "lambda.z", NcReason::TooFewPoints);
}

// ---- Review findings of T-004b ----

/// Cmax at 1; the last 3 to 5 eligible points decrease, the full set of 6 rises (low point at 2).
const SKIP_T: [f64; 8] = [0.0, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0];
const SKIP_C: [f64; 8] = [0.0, 10.0, 1.0, 2.0, 4.0, 3.0, 2.5, 2.0];

#[test]
fn an_admissible_but_rising_larger_fit_is_skipped() {
    // With a factor of 10 every fit is within tolerance; the 6-point fit has λz <= 0, so the
    // largest valid fit (5 points) is selected, under both tie rules.
    for tie_rule in [LambdaZTieRule::Tolerance, LambdaZTieRule::Bonus] {
        let mut options = with_selection(LambdaZSelection {
            tie_rule,
            ..LambdaZSelection::default()
        });
        options.lambda_z.adj_r_squared_factor = 10.0;
        let r = ev(&SKIP_T, &SKIP_C, options);
        let six = r
            .lambda_z_candidates()
            .iter()
            .find(|c| c.n_points == 6)
            .unwrap();
        assert!(!six.valid && !six.selected, "{tie_rule:?}");
        assert_eq!(six.half_life(), None);
        assert_close(&r, "lambda.z.n.points", 5.0);
    }
}

#[test]
fn flat_terminal_phase_has_undefined_r_squared() {
    let r = ev(
        &TH_T,
        &[0.0, 5.0, 9.0, 4.0, 3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0],
        with_selection(LambdaZSelection {
            manual: Some(LambdaZManual::Times(vec![9.05, 12.12, 24.37])),
            ..LambdaZSelection::default()
        }),
    );
    assert_nc(&r, "lambda.z", NcReason::NoValidFit);
    let fit = r.lambda_z_candidates().first().unwrap();
    assert_eq!(fit.r_squared, None);
    assert_eq!(fit.half_life(), None);
    assert!(NcReason::Undefined.to_string().contains("3 points"));
}

#[test]
fn overflowing_regression_sums_give_no_candidate() {
    let t = [0.0, 1.0, 1e200, 2e200, 3e200, 4e200];
    let c = [0.0, 9.0, 8.0, 4.0, 2.0, 1.0];
    let r = ev(&t, &c, NcaOptions::default());
    assert!(r.get("lambda.z").is_none());
    assert!(
        r.lambda_z_candidates()
            .iter()
            .all(|c| c.adj_r_squared.is_none_or(f64::is_finite))
    );
}

#[test]
fn lambda_z_selection_and_candidates_round_trip_through_json() {
    let selection = LambdaZSelection {
        tie_rule: LambdaZTieRule::Bonus,
        positive_filter_first: true,
        exclude: vec![12.12],
        manual: Some(LambdaZManual::Range {
            start: 3.0,
            end: 30.0,
        }),
        exclude_replaced: true,
    };
    let text = serde_json::to_string(&selection).unwrap();
    assert_eq!(
        serde_json::from_str::<LambdaZSelection>(&text).unwrap(),
        selection
    );
    let r = ev(&TH_T, &TH_C, NcaOptions::default());
    for candidate in r.lambda_z_candidates() {
        let text = serde_json::to_string(candidate).unwrap();
        assert_eq!(
            &serde_json::from_str::<LambdaZCandidate>(&text).unwrap(),
            candidate
        );
    }
}

// ---- Derived parameters (NCA-EXT-04 to 07, OBS-05) ----

#[test]
fn w2_derived_parameters_iv_bolus_and_infusion() {
    // W2 (D = 100): MRT 5, CL 2, Vz 10, Vss 10.
    let t = [0.0, 1.0, 2.0, 4.0, 8.0, 12.0];
    let c: Vec<f64> = t.iter().map(|t: &f64| 10.0 * (-0.2 * t).exp()).collect();
    let r = run_with(&t, &c, Route::IvBolus, NcaOptions::default());
    assert_close(&r, "mrt.iv.obs", 5.0);
    assert_close(&r, "cl.obs", 2.0);
    assert_close(&r, "vz.obs", 10.0);
    assert_close(&r, "vss.iv.obs", 10.0);
    assert_close(&r, "cmax.dn", 0.1);
    assert_close(&r, "aucinf.obs.dn", 0.5);
    assert_nc(&r, "mrt.obs", NcReason::NotApplicableToRoute);
    // The same areas read as a 2-hour infusion: MRT loses T_inf/2 = 1 (W5), Vss = CL·MRT = 8.
    let r = run_with(
        &t,
        &c,
        Route::IvInfusion { duration: 2.0 },
        NcaOptions::default(),
    );
    assert_close(&r, "lambda.z", 0.2);
    assert_close(&r, "mrt.iv.obs", 4.0);
    assert_close(&r, "vss.iv.obs", 8.0);
}

#[test]
fn w4_derived_parameters() {
    let r = run(&NcaInput {
        time: vec![0.25, 0.5, 1.0, 2.0, 4.0],
        conc: vec![8.0, 6.4, 4.1, 1.7, 0.29],
        dose: 50.0,
        route: Route::IvBolus,
        options: NcaOptions::default(),
    })
    .unwrap();
    assert_close(&r, "mrt.iv.obs", 1.13053581183);
    assert_close(&r, "cl.obs", 4.43866007581);
    assert_close(&r, "vz.obs", 5.02314219973);
    assert_close(&r, "vss.iv.obs", 5.01806417225);
}

#[test]
fn extravascular_mrt_includes_absorption_and_has_no_vss() {
    let r = ev(&TH_T, &TH_C, NcaOptions::default());
    let ratio = |a: &str, b: &str| r.get(a).unwrap() / r.get(b).unwrap();
    assert_close(&r, "mrt.obs", ratio("aumcinf.obs", "aucinf.obs"));
    assert_close(&r, "mrt.last", ratio("aumclast", "auclast"));
    assert_close(&r, "cl.pred", 100.0 / r.get("aucinf.pred").unwrap());
    assert_close(&r, "vz.obs", ratio("cl.obs", "lambda.z"));
    assert_nc(&r, "vss.iv.obs", NcReason::NotApplicableToRoute);
    assert_nc(&r, "mrt.iv.obs", NcReason::NotApplicableToRoute);
}
