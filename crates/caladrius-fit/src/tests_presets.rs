//! The default settings and the `reference_conventions` preset (task T-040, Q-014 option 2;
//! `specs/fit.md` FIT-JAC-01, FIT-CNV-01, CNV-02; `specs/differences.md` D-03, D-04).

use std::collections::BTreeMap;

use caladrius_testkit::load_fit_case;

use crate::*;

/// Subject 2 of the public reference fit `fit_indometh_inv_y` (IV bolus, weights 1/y), with the
/// case's initial estimates and `options`.
fn indometh_inv_y_subject_2(options: FitOptions) -> FitInput {
    let case = load_fit_case("fit_indometh_inv_y").unwrap();
    let profile = case.profile("2").unwrap();
    let (time, conc) = case.fitted_observations(profile);
    FitInput {
        model: ModelId::from_id(&case.model).unwrap(),
        dose: profile.dose,
        time,
        conc,
        weighting: Weighting::InvY,
        initial: case.initial_estimates["2"].clone(),
        options,
    }
}

#[test]
fn the_defaults_are_the_preset_default() {
    let o = FitOptions::default();
    assert_eq!(o, FitOptions::preset(FitPreset::Default));
    assert_eq!(o.derivatives, Derivatives::Auto);
    assert_eq!(o.increment, 1e-5);
    assert_eq!(o.criterion, Criterion::RelativeDecrease);
    assert_eq!(o.convergence, 1e-10);
    assert_eq!(o.max_iterations, 50);
    assert_eq!(o.confidence_level, 0.95);
    assert_eq!(o.n_curve, 1000);
}

#[test]
fn the_reference_conventions_are_the_previous_defaults() {
    let o = FitOptions::reference_conventions();
    assert_eq!(o.derivatives, Derivatives::ForwardDifference);
    assert_eq!(o.increment, 0.001);
    assert_eq!(o.criterion, Criterion::RelativeDecrease);
    assert_eq!(o.convergence, 0.0001);
    assert_eq!(o.max_iterations, 50);
    // Only three of the five iteration settings differ between the presets.
    let d = FitOptions::default();
    assert_eq!(
        FitOptions {
            derivatives: d.derivatives,
            increment: d.increment,
            convergence: d.convergence,
            ..o
        },
        d
    );
}

/// The preset reproduces the defaults of v0.1.0: the trace below was recorded with
/// `FitOptions::default()` before T-040 (commit 2fee9b9), on the reference fit where those
/// defaults are furthest from the exact minimum (D-03). Values are compared to 1e-12 relative
/// (the last bits of `exp` may differ between platforms); the step, damping and halvings exactly.
#[test]
fn the_reference_conventions_reproduce_the_previous_default_trace() {
    #[rustfmt::skip]
    let previous: [(f64, [f64; 2], Option<f64>); 11] = [
        (0.9683503034376565, [11.4, 1.015], None),
        (0.8995613230556219, [12.416582254398591, 0.8554543097268401], Some(0.07646947308536213)),
        (0.8604445315947185, [13.538748082893523, 0.7515046308142364], Some(0.04546114249619983)),
        (0.8372529725535429, [14.452753767760278, 0.6806901391254313], Some(0.027699583998422195)),
        (0.8253827629618923, [15.128246236866591, 0.6344277048836051], Some(0.014381460486349753)),
        (0.8200978739641188, [15.587919896477299, 0.6054909100515243], Some(0.006444217410572983)),
        (0.8179755372197494, [15.881772161500075, 0.5879436501536047], Some(0.0025946212909779124)),
        (0.8171775439008715, [16.062138666695787, 0.5775043582417576], Some(0.0009765237980825704)),
        (0.8168884089279458, [16.17020079873899, 0.5713621405901458], Some(0.0003539467199750805)),
        (0.8167853216123054, [16.234047302506447, 0.5677708739510228], Some(0.00012621102866645323)),
        (0.8167485533382147, [16.271469815283513, 0.5656785974348557], Some(4.5017862523827723e-5)),
    ];
    let same = |a: f64, b: f64| (a - b).abs() <= 1e-12 * b.abs();
    let r = run(&indometh_inv_y_subject_2(
        FitOptions::reference_conventions(),
    ))
    .unwrap();
    assert_eq!(r.status(), FitStatus::Converged);
    assert_eq!(r.derivatives(), Some(Derivatives::ForwardDifference));
    assert_eq!(r.trace().len(), previous.len(), "{:?}", r.trace());
    for (i, (row, (wrss, estimates, decrease))) in r.trace().iter().zip(previous).enumerate() {
        assert_eq!(row.iteration, i);
        assert!(
            same(row.wrss, wrss),
            "iteration {i}: {} against {wrss}",
            row.wrss
        );
        for (a, e) in row.estimates.iter().zip(estimates) {
            assert!(same(*a, e), "iteration {i}: {a} against {e}");
        }
        assert_eq!(row.step, (i > 0).then_some(1.0));
        assert_eq!((row.lambda, row.halvings), (0.0, 0));
        match (row.relative_decrease, decrease) {
            (Some(a), Some(e)) => assert!(same(a, e), "iteration {i}: {a} against {e}"),
            (a, e) => assert_eq!(a, e),
        }
    }
    assert!(same(r.get("se.v").unwrap(), 3.144324828366111));
    assert!(same(r.get("se.k").unwrap(), 0.1368797681616916));
}

/// The default reaches the exact minimum (the reference: R, analytic derivatives, tight stop) on
/// the same subject, with closed-form derivatives, and is the explicit analytic fit bit for bit.
#[test]
fn the_default_fit_is_the_analytic_tight_fit() {
    let r = run(&indometh_inv_y_subject_2(FitOptions::default())).unwrap();
    assert_eq!(r.status(), FitStatus::Converged);
    assert_eq!(r.derivatives(), Some(Derivatives::Analytic));
    let explicit = run(&indometh_inv_y_subject_2(FitOptions {
        derivatives: Derivatives::Analytic,
        ..FitOptions::default()
    }))
    .unwrap();
    assert_eq!(r, explicit);
    let case = load_fit_case("fit_indometh_inv_y").unwrap();
    for name in ["estimate.v", "estimate.k"] {
        let reference = case.expected.get("2", name).flatten().unwrap();
        let x = r.get(name).unwrap();
        assert!(
            (x / reference - 1.0).abs() < 1e-5,
            "{name}: {x} against {reference}"
        );
    }
}

/// `auto` without closed forms (the CL parameterisation) is forward differences with the
/// default increment 1e-5, said in the result; `analytic` there stays an error (FIT-JAC-02).
#[test]
fn auto_falls_back_to_forward_differences_and_says_so() {
    let input = FitInput {
        model: ModelId::IvBolus,
        dose: 100.0,
        time: vec![0.5, 1.0, 2.0, 4.0, 8.0],
        conc: vec![9.31, 7.92, 6.85, 4.31, 2.11],
        weighting: Weighting::Uniform,
        initial: BTreeMap::from([("v".to_string(), 12.0), ("cl".to_string(), 1.8)]),
        options: FitOptions::default(),
    };
    let r = run(&input).unwrap();
    assert_eq!(r.status(), FitStatus::Converged);
    assert_eq!(r.derivatives(), Some(Derivatives::ForwardDifference));
    // The F2 minimum (V = 9.9186407, k = 0.20405768, so CL = V·k), within 1e-4.
    let v = r.get("estimate.v").unwrap();
    let cl = r.get("estimate.cl").unwrap();
    assert!((v / 9.9186407 - 1.0).abs() < 1e-4, "{v}");
    assert!((cl / (9.9186407 * 0.20405768) - 1.0).abs() < 1e-4, "{cl}");
    let analytic = FitInput {
        options: FitOptions {
            derivatives: Derivatives::Analytic,
            ..FitOptions::default()
        },
        ..input
    };
    assert!(matches!(
        run(&analytic),
        Err(FitError::AnalyticDerivativesUnavailable { .. })
    ));
}

/// In JSON the preset gives the starting values and explicit keys replace them; the options are
/// then plain values, so they serialise without the preset and read back the same.
#[test]
fn the_preset_is_selected_in_json() {
    let read = |text: &str| serde_json::from_str::<FitOptions>(text);
    assert_eq!(read("{}").unwrap(), FitOptions::default());
    assert_eq!(
        read(r#"{"preset":"default"}"#).unwrap(),
        FitOptions::default()
    );
    let reference = read(r#"{"preset":"reference_conventions"}"#).unwrap();
    assert_eq!(reference, FitOptions::reference_conventions());
    let mixed =
        read(r#"{"preset":"reference_conventions","convergence":1e-6,"n_curve":0}"#).unwrap();
    assert_eq!(
        mixed,
        FitOptions {
            convergence: 1e-6,
            n_curve: 0,
            ..FitOptions::reference_conventions()
        }
    );
    // The explicit values alone give the same options as the preset.
    let explicit =
        read(r#"{"derivatives":"forward_difference","increment":0.001,"convergence":0.0001}"#)
            .unwrap();
    assert_eq!(explicit, reference);
    let text = serde_json::to_string(&reference).unwrap();
    assert!(!text.contains("preset"), "{text}");
    assert_eq!(read(&text).unwrap(), reference);
    assert!(read(r#"{"derivatives":"auto"}"#).is_ok());
    let e = read(r#"{"preset":"reference"}"#).unwrap_err().to_string();
    assert!(e.contains("reference_conventions"), "{e}");
    assert!(read(r#"{"presets":"default"}"#).is_err());
}
