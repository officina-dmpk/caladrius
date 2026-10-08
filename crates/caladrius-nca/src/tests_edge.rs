//! Unit tests of the parameters and rules added by task T-013 (Tlag, replaced values, plain Vss,
//! Vss to Tlast, AUMCall per dose, PKNCA's IV areas). Expected values are computed by hand in the
//! comments; the oracle cases `edge_*` check the same rules against PKNCA.

use caladrius_testkit::Tolerance;

use crate::*;

const LN2: f64 = std::f64::consts::LN_2;

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

fn linear() -> NcaOptions {
    NcaOptions {
        auc_method: AucMethod::Linear,
        ..NcaOptions::default()
    }
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

// ---- Tlag (NCA-OBS-04) ----

#[test]
fn tlag_is_the_sample_before_the_first_rise() {
    let t = [0.0, 1.0, 2.0, 4.0];
    assert_close(&ev(&t, &[0.0, 0.0, 3.0, 2.0], linear()), "tlag", 1.0);
    // Rising at once: the first sample.
    assert_close(&ev(&t, &[0.0, 5.0, 3.0, 2.0], linear()), "tlag", 0.0);
    // Never rising.
    assert_nc(
        &ev(&t, &[5.0, 3.0, 2.0, 1.0], linear()),
        "tlag",
        NcReason::NoRise,
    );
    assert!(NcReason::NoRise.to_string().contains("never rises"));
}

#[test]
fn tlag_reads_the_samples_before_the_blq_policy() {
    // Leading zeros dropped by the BLQ policy still mark the lag (PKNCA, edge_blq_first_drop).
    let options = NcaOptions {
        blq: BlqPolicy::all(BlqAction::Drop),
        ..linear()
    };
    let r = ev(&[0.0, 1.0, 2.0, 4.0], &[0.0, 0.0, 3.0, 2.0], options);
    assert_close(&r, "tlag", 1.0);
}

#[test]
fn tlag_is_extravascular_only() {
    let r = run_with(
        &[0.0, 1.0, 2.0],
        &[0.0, 5.0, 3.0],
        Route::IvBolus,
        NcaOptions::default(),
    );
    assert_nc(&r, "tlag", NcReason::NotApplicableToRoute);
}

// ---- Replaced values (T-012, PKNCA profile) ----

#[test]
fn a_replaced_missing_value_is_integrated_but_is_not_tlast() {
    let options = NcaOptions {
        missing: MissingPolicy::Replace(1.0),
        ..linear()
    };
    let r = ev(
        &[0.0, 1.0, 2.0, 4.0, 8.0],
        &[0.0, 5.0, 3.0, 2.0, f64::NAN],
        options,
    );
    assert_close(&r, "tlast", 4.0);
    assert_close(&r, "clast.obs", 2.0);
    // [0,1]: 2.5; [1,2]: 4; [2,4]: 5; [4,8] to the replaced 1: 6.
    assert_close(&r, "auclast", 17.5);
    assert_close(&r, "aucall", 17.5);
    // A replaced first value is not Tfirst either.
    let r = ev(
        &[0.0, 1.0, 2.0, 4.0],
        &[f64::NAN, 5.0, 3.0, 2.0],
        NcaOptions {
            missing: MissingPolicy::Replace(1.0),
            ..linear()
        },
    );
    assert_close(&r, "tfirst", 1.0);
}

// ---- Volumes and dose-normalised AUMCall ----

/// W2 of `specs/nca.md`: C = 10 e^(-0.2 t), D = 100.
fn w2() -> (Vec<f64>, Vec<f64>) {
    let t = vec![0.0, 1.0, 2.0, 4.0, 8.0, 12.0];
    let c = t.iter().map(|t: &f64| 10.0 * (-0.2 * t).exp()).collect();
    (t, c)
}

#[test]
fn plain_vss_uses_the_uncorrected_mrt() {
    let (t, c) = w2();
    let r = run_with(&t, &c, Route::IvBolus, NcaOptions::default());
    assert_close(&r, "vss.obs", 10.0);
    // As a 2-hour infusion: the plain Vss stays CL·AUMC/AUC = 10, the IV one is 8.
    let r = run_with(
        &t,
        &c,
        Route::IvInfusion { duration: 2.0 },
        NcaOptions::default(),
    );
    assert_close(&r, "vss.obs", 10.0);
    assert_close(&r, "vss.iv.obs", 8.0);
    // Vss to Tlast: D/AUClast · (AUMClast/AUClast − T_inf/2).
    let (auc, aumc) = (r.get("auclast").unwrap(), r.get("aumclast").unwrap());
    assert_close(&r, "vss.iv.last", 100.0 / auc * (aumc / auc - 1.0));
    // Extravascular: plain Vss (apparent), no IV form.
    let r = ev(&t, &c, NcaOptions::default());
    let mrt = r.get("aumcinf.obs").unwrap() / r.get("aucinf.obs").unwrap();
    assert_close(&r, "vss.obs", r.get("cl.obs").unwrap() * mrt);
    assert_nc(&r, "vss.iv.last", NcReason::NotApplicableToRoute);
    assert_close(&r, "aumcall.dn", r.get("aumcall").unwrap() / 100.0);
}

// ---- PKNCA IV areas (NCA-IV-02, IV-03) ----

#[test]
fn iv_areas_with_a_zero_record_at_the_dose_time() {
    // C0 = 8·2 = 16 (method 2). The record segment (0 → 8) is linear, 4; the C0 segment is the
    // exponential 16 → 8 over 1 h, 8/ln 2, whatever the AUC method. AUClast (linear) = 4 + 6 + 6 + 6.
    let r = run_with(
        &[0.0, 1.0, 2.0, 4.0, 8.0],
        &[0.0, 8.0, 4.0, 2.0, 1.0],
        Route::IvBolus,
        linear(),
    );
    assert_close(&r, "c0", 16.0);
    assert_close(&r, "auclast", 22.0);
    let auciv = 22.0 + 8.0 / LN2 - 4.0;
    assert_close(&r, "aucivlast", auciv);
    assert_close(&r, "aucivall", auciv);
    assert_close(&r, "aucivpbextlast", 100.0 * (1.0 - 22.0 / auciv));
    let inf = r.get("aucinf.obs").unwrap();
    assert_close(&r, "aucivinf.obs", inf + 8.0 / LN2 - 4.0);
}

#[test]
fn iv_areas_with_an_observed_c0_equal_the_ordinary_ones() {
    for options in [linear(), NcaOptions::default()] {
        let r = run_with(
            &[0.0, 1.0, 2.0, 4.0],
            &[10.0, 5.0, 2.5, 1.25],
            Route::IvBolus,
            options,
        );
        assert_close(&r, "aucivlast", r.get("auclast").unwrap());
        assert_close(&r, "aucivpbextlast", 0.0);
    }
}

#[test]
fn iv_areas_need_a_record_at_the_dose_time_and_a_bolus() {
    // No record at the dose time: the start policy inserts C0, but PKNCA's IV areas need a record.
    let r = run_with(
        &[0.25, 0.5, 1.0, 2.0],
        &[8.0, 6.4, 4.1, 1.7],
        Route::IvBolus,
        NcaOptions::default(),
    );
    assert_nc(&r, "aucivlast", NcReason::NoStartConcentration);
    assert_nc(&r, "aucivpbextinf.obs", NcReason::NoStartConcentration);
    let r = ev(&[0.0, 1.0, 2.0], &[0.0, 5.0, 3.0], NcaOptions::default());
    assert_nc(&r, "aucivlast", NcReason::NotApplicableToRoute);
}
