//! Three-compartment models in the interface (task "demo/ui-quick"): the model picker offers three
//! compartments and the three parameter sets, the fit page asks for every starting value (the
//! engine refuses to generate them for pk3, as for pk2), and a fit run through the page gives the
//! same estimates as `fit.run` called with the same request. No number is computed here: every
//! curve and statistic below is the answer of an engine command. The case is a public one of
//! `oracle/expected/models/pk3/` (clearance set of the base case, dose 100).

use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::app::{Action, UiApp};
use crate::model::FitView;
use crate::modelinfo::{Compartments, Input, ParameterSet};
use crate::tests::harness;

/// The oracle case: model, dose and parameters, as the oracle file gives them.
const CASE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/expected/models/pk3/model_pk3_conv_base_clearance.options.json"
));

/// Times that see the three phases (the engine's own tests use the same).
const TIMES: [f64; 20] = [
    0.05, 0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0, 48.0,
    72.0, 96.0, 120.0,
];

fn case() -> Value {
    serde_json::from_str(CASE).unwrap_or(Value::Null)
}

/// A worksheet holding the profile of the oracle case with a small alternating error of 1 %.
fn with_pk3_profile() -> UiApp {
    let case = case();
    let dose = case["dose"].as_f64().unwrap();
    let mut app = UiApp::new();
    let sim = app
        .engine_mut()
        .execute(
            "model.simulate",
            json!({ "model": case["model"], "dose": dose, "params": case["parameters"], "times": TIMES }),
        )
        .unwrap();
    let conc: Vec<f64> = sim["conc"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let mut csv = String::from("Time (h),Conc (mg/L),Dose (mg)\n");
    for (i, (t, c)) in TIMES.iter().zip(conc).enumerate() {
        let noise = if i % 2 == 0 { 1.01 } else { 0.99 };
        csv.push_str(&format!("{t},{},{dose}\n", c * noise));
    }
    app.load_csv("pk3.csv", csv.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

/// The fit page with the three-compartment bolus model picked.
fn fit_page_three() -> UiApp {
    let mut app = with_pk3_profile();
    app.perform(vec![Action::NewFitWith(Compartments::Three)]);
    if let Some(page) = app.state.fit.as_mut() {
        page.input = Input::Bolus;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    app
}

#[test]
fn the_oracle_case_is_the_one_the_test_reads() {
    let case = case();
    assert_eq!(case["model"], "pk3.iv_bolus");
    assert_eq!(case["parameters"]["q3"], 6.3);
}

#[test]
fn the_engine_refuses_starting_values_for_pk3_and_the_page_asks_for_every_one() {
    let mut app = fit_page_three();
    let page = app.fit_page().unwrap().clone();
    assert_eq!(page.model().id, "pk3.iv_bolus");
    assert!(!page.automatic_estimates());
    // The engine says to give them; the page does not call it and shows editable fields instead.
    let refused = app
        .engine_mut()
        .execute("fit.initial_estimates", page.initial_params());
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(page.start_error, None);
    let names: Vec<&str> = page.initial.keys().map(String::as_str).collect();
    assert_eq!(names, ["cl", "q2", "q3", "vc", "vp2", "vp3"]);
    assert_eq!(page.pending.len(), 6);
    assert!(!page.starting_values_set());
    // The live curve and the objective are the engine's, at the values shown.
    assert!(page.preview.curve.len() > 2);
    assert!(page.preview.wrss.is_some_and(|w| w > 0.0));
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label_contains("cannot be generated from the data for three compartments");
    h.get_by_label_contains("6 still to set");
    h.get_by_label_contains(
        "Compartments: 3.  Model pk3.iv_bolus.  Fitted: CL, Vc, Q2, Vp2, Q3, Vp3.",
    );
    h.get_by_label_contains("Parameters as");
}

#[test]
fn a_pk3_fit_through_the_page_gives_the_estimates_of_fit_run_bit_for_bit() {
    let mut app = fit_page_three();
    let start = [
        ("cl", 3.2),
        ("vc", 8.5),
        ("q2", 9.0),
        ("vp2", 10.0),
        ("q3", 6.0),
        ("vp3", 30.0),
    ];
    if let Some(page) = app.state.fit.as_mut() {
        page.initial = start.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
        page.accept_placeholders();
    }
    app.perform(vec![Action::FitChanged { regenerate: false }]);
    let request = app.fit_page().unwrap().run_params();
    app.perform(vec![Action::RunFit]);
    let page = app.fit_page().unwrap().clone();
    let ours = page.ok().expect("the fit has a result").clone();
    assert_eq!(ours.status, "converged", "{:?}", page.error);
    assert_eq!(ours.parameters, ["cl", "q2", "q3", "vc", "vp2", "vp3"]);
    // The same request to the engine.
    let direct = app.engine_mut().execute("fit.run", request).unwrap();
    let theirs = FitView::from_value(&direct).expect("a fit");
    let theirs = theirs.ok().expect("the engine's fit has a result");
    assert_eq!(ours.values, theirs.values);
    assert_eq!(ours.curve, theirs.curve);
    assert!(ours.value("estimate.cl").is_some());
    // The estimates are near the public parameters of the case.
    let case = case();
    for name in &ours.parameters {
        let truth = case["parameters"][name.as_str()].as_f64().unwrap();
        let got = ours.value(&format!("estimate.{name}")).unwrap();
        assert!(
            (got / truth - 1.0).abs() < 0.5,
            "{name}: {got} against {truth}"
        );
    }
}

#[test]
fn the_three_sets_and_the_six_models_are_picked_and_simulated() {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(Compartments::Three)]);
    assert_eq!(app.sim_page().unwrap().model().id, "pk3.oral_1");
    for set in ParameterSet::ALL {
        for (input, lag) in [
            (Input::Bolus, false),
            (Input::Infusion, false),
            (Input::FirstOrder, false),
            (Input::FirstOrder, true),
            (Input::ZeroOrder, false),
            (Input::ZeroOrder, true),
        ] {
            if let Some(p) = app.state.sim.as_mut() {
                p.set = set;
                p.input = input;
                p.lag = lag;
            }
            app.perform(vec![Action::SimChanged]);
            let page = app.sim_page().unwrap();
            assert!(
                page.error.is_none(),
                "{set:?} {}: {:?}",
                page.model().id,
                page.error
            );
            assert!(page.result.is_some());
            let sent = page.params(false);
            let keys = sent["params"].as_object().unwrap().len();
            let want = page.model().parameters_in(set).len() + usize::from(input.has_duration());
            assert_eq!(keys, want, "{set:?} {}", page.model().id);
        }
    }
}

#[test]
fn the_three_compartment_screens_draw_in_both_themes_and_both_axes() {
    use crate::theme::ThemeMode;
    for dark in [false, true] {
        for log in [false, true] {
            let mut fitted = fit_page_three();
            if let Some(page) = fitted.state.fit.as_mut() {
                page.accept_placeholders();
            }
            fitted.perform(vec![Action::FitChanged { regenerate: false }]);
            let mut sim = UiApp::new();
            sim.perform(vec![Action::NewSimulationWith(Compartments::Three)]);
            for mut app in [fitted, sim] {
                if dark {
                    app.state.mode = ThemeMode::Dark;
                }
                app.state.log_axis = log;
                harness(app).run_steps(3);
            }
        }
    }
}

#[test]
fn the_palette_offers_both_three_compartment_pages() {
    let app = with_pk3_profile();
    let entries = app.palette_entries();
    let find = |key: &str| entries.iter().find(|e| e.key == key).cloned();
    let fit = find("ui:new_fit_three").unwrap();
    assert_eq!(fit.action, Action::NewFitWith(Compartments::Three));
    let sim = find("ui:new_simulation_three").unwrap();
    assert_eq!(sim.action, Action::NewSimulationWith(Compartments::Three));
}
