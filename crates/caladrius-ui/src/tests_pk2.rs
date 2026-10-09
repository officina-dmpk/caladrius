//! Two-compartment models in the interface (task T-034b): the model picker offers the number of
//! compartments and the parameter set, the fit page asks for every starting value (the engine
//! cannot generate them for two compartments) and still draws the live curve and the objective,
//! the simulation page accepts the pk2 ids, and the palette offers both. No number is computed
//! here: every curve and statistic below is the answer of an engine command.

use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::app::{Action, Selection, UiApp};
use crate::modelinfo::{Compartments, Input, ParameterSet};
use crate::tests::harness;

const TIMES: [f64; 14] = [
    0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0, 48.0,
];

/// A worksheet holding a simulated pk2 profile (public parameters of the oracle's base case,
/// dose 100) with a small alternating error of 1 %.
pub(crate) fn with_pk2_profile(model: &str, params: Value) -> UiApp {
    let mut app = UiApp::new();
    let sim = app
        .engine_mut()
        .execute(
            "model.simulate",
            json!({ "model": model, "dose": 100.0, "params": params, "times": TIMES }),
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
        csv.push_str(&format!("{t},{},100\n", c * noise));
    }
    app.load_csv("pk2.csv", csv.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

fn bolus_app() -> UiApp {
    with_pk2_profile(
        "pk2.iv_bolus",
        json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8 }),
    )
}

/// The fit page with the two-compartment bolus model picked.
fn fit_page_two(mut app: UiApp) -> UiApp {
    app.perform(vec![Action::NewFitWith(Compartments::Two)]);
    if let Some(page) = app.state.fit.as_mut() {
        page.input = Input::Bolus;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    app
}

#[test]
fn picking_two_compartments_asks_for_every_starting_value_and_draws_the_live_curve() {
    let mut app = fit_page_two(bolus_app());
    let page = app.fit_page().unwrap().clone();
    assert_eq!(page.model().id, "pk2.iv_bolus");
    assert!(!page.automatic_estimates());
    // No call to fit.initial_estimates: a value for every parameter of the set, none of them set.
    assert_eq!(page.start_error, None);
    let names: Vec<&str> = page.initial.keys().map(String::as_str).collect();
    assert_eq!(names, ["cl", "q", "vc", "vp"]);
    assert_eq!(page.pending.len(), 4);
    assert!(!page.starting_values_set());
    // The curve and the objective come from model.simulate and fit.evaluate at those values.
    assert_eq!(
        page.preview.curve.len(),
        crate::fit::defaults::PREVIEW_POINTS
    );
    assert_eq!(page.preview.dose, Some(100.0));
    assert!(page.preview.wrss.is_some_and(|w| w > 0.0));
    let evaluated = app
        .engine_mut()
        .execute("fit.evaluate", page.evaluate_params())
        .unwrap();
    assert_eq!(page.preview.wrss, evaluated["wrss"].as_f64());
    let simulated = app
        .engine_mut()
        .execute("model.simulate", page.simulate_params(48.0).unwrap())
        .unwrap();
    let curve: crate::model::Simulation = crate::model::read(&simulated);
    assert_eq!(page.preview.curve, curve.curve());
    // Nothing was stored by looking.
    assert!(app.engine().project().analyses().is_empty());
}

#[test]
fn the_page_says_so_in_one_sentence_and_the_fit_waits_for_the_values() {
    let app = fit_page_two(bolus_app());
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label_contains("cannot be generated from the data for two compartments");
    h.get_by_label_contains("4 still to set");
    h.get_by_label_contains("Compartments: 2.  Model pk2.iv_bolus.  Fitted: CL, Vc, Q, Vp.");
    h.get_by_label_contains("Parameters as");
    // The fit button is disabled until the values are set; accepting them enables it.
    assert!(!h.state().fit_page().unwrap().starting_values_set());
    h.get_by_label("Use the values shown").click();
    h.run_steps(3);
    assert!(h.state().fit_page().unwrap().starting_values_set());
}

#[test]
fn a_two_compartment_fit_runs_from_the_values_the_person_set_and_comes_back_from_the_project() {
    let mut app = fit_page_two(bolus_app());
    if let Some(page) = app.state.fit.as_mut() {
        page.initial = [("cl", 1.6), ("vc", 11.0), ("q", 3.2), ("vp", 9.0)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        page.accept_placeholders();
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    let Selection::Analysis(id) = app.state.selection.clone() else {
        panic!("the fit page should now show the analysis");
    };
    let page = app.fit_page().unwrap().clone();
    let ok = page.ok().expect("the fit has a result");
    assert_eq!(ok.status, "converged");
    // The engine lists the fitted parameters by name.
    assert_eq!(ok.parameters, ["cl", "q", "vc", "vp"]);
    for (name, truth) in [("cl", 2.0), ("vc", 10.0), ("q", 4.0), ("vp", 8.0)] {
        let got = ok.value(&format!("estimate.{name}")).unwrap();
        assert!((got / truth - 1.0).abs() < 0.1, "{name}: {got}");
    }
    // The page comes back from the stored analysis with its model, set and values.
    app.perform(vec![Action::Select(Selection::Worksheet(1))]);
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let back = app.fit_page().unwrap();
    assert_eq!(back.model().id, "pk2.iv_bolus");
    assert_eq!(back.set, ParameterSet::Clearance);
    assert_eq!(back.initial, page.initial);
    assert!(back.ok().is_some() && back.preview.curve.len() > 2);
}

#[test]
fn the_parameter_set_is_a_choice_and_each_set_fits() {
    for (set, start) in [
        (
            ParameterSet::Micro,
            json!({ "k10": 0.18, "k12": 0.35, "k21": 0.45, "vc": 11.0 }),
        ),
        (
            ParameterSet::Macro,
            json!({ "a": 5.0, "b": 5.0, "alpha": 1.1, "beta": 0.11 }),
        ),
    ] {
        let mut app = fit_page_two(bolus_app());
        if let Some(page) = app.state.fit.as_mut() {
            page.set = set;
        }
        app.perform(vec![Action::FitChanged { regenerate: true }]);
        let page = app.fit_page().unwrap().clone();
        let mut names: Vec<&str> = page.initial.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut want: Vec<&str> = set.names().to_vec();
        want.sort_unstable();
        assert_eq!(names, want, "{set:?}");
        assert!(page.preview.error.is_none(), "{:?}", page.preview.error);
        assert!(page.preview.curve.len() > 2);
        if let Some(page) = app.state.fit.as_mut() {
            page.initial = start
                .as_object()
                .unwrap()
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .collect();
            page.accept_placeholders();
        }
        app.perform(vec![Action::RunFit]);
        let page = app.fit_page().unwrap();
        assert_eq!(
            page.ok().map(|o| o.status.as_str()),
            Some("converged"),
            "{set:?}"
        );
        assert_eq!(page.set, set);
    }
}

#[test]
fn going_back_to_one_compartment_asks_the_engine_for_starting_values_again() {
    let mut app = fit_page_two(with_pk2_profile(
        "pk2.oral_1",
        json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8, "ka": 2 }),
    ));
    assert!(!app.fit_page().unwrap().initial.is_empty());
    if let Some(page) = app.state.fit.as_mut() {
        page.compartments = Compartments::One;
        page.input = Input::FirstOrder;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    let page = app.fit_page().unwrap();
    assert_eq!(page.model().id, "pk1.oral_1");
    assert!(page.pending.is_empty());
    assert!(page.initial.contains_key("v") && page.initial.contains_key("k"));
    assert!(!page.initial.contains_key("cl"));
}

#[test]
fn the_simulation_page_accepts_two_compartments_in_the_three_sets() {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(Compartments::Two)]);
    let page = app.sim_page().unwrap().clone();
    assert_eq!(page.model().id, "pk2.oral_1");
    let sent = page.params(false);
    assert_eq!(
        sent["params"],
        json!({ "cl": 2.0, "vc": 10.0, "q": 4.0, "vp": 8.0, "ka": 1.2 })
    );
    // The curve is the engine's, with the three sets among the derived values.
    let direct = app.engine_mut().execute("model.simulate", sent).unwrap();
    let want: crate::model::Simulation = crate::model::read(&direct);
    assert_eq!(page.result.as_ref().map(|r| r.curve()), Some(want.curve()));
    let secondary = &page.result.as_ref().unwrap().secondary;
    for name in ["alpha", "beta", "k10", "vss", "half_life"] {
        assert!(secondary.contains_key(name), "{name}");
    }
    for (set, names) in [
        (ParameterSet::Micro, ["k10", "k12", "k21", "vc"]),
        (ParameterSet::Macro, ["a", "alpha", "b", "beta"]),
    ] {
        if let Some(p) = app.state.sim.as_mut() {
            p.set = set;
            p.input = Input::Bolus;
        }
        app.perform(vec![Action::SimChanged]);
        let page = app.sim_page().unwrap();
        assert!(page.error.is_none(), "{set:?}: {:?}", page.error);
        let sent = page.params(false);
        let keys: Vec<&str> = sent["params"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut want = names.to_vec();
        want.sort_unstable();
        assert_eq!(keys, want, "{set:?}");
        assert_eq!(sent["model"], "pk2.iv_bolus");
    }
    // Saved, then opened again: the set and the model come back.
    app.perform(vec![Action::SimSave]);
    let id = app.sim_page().unwrap().analysis.unwrap();
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let back = app.sim_page().unwrap();
    assert_eq!(
        (back.model().id, back.set),
        ("pk2.iv_bolus", ParameterSet::Macro)
    );
}

#[test]
fn a_value_the_engine_refuses_is_shown_as_its_sentence() {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(Compartments::Two)]);
    if let Some(p) = app.state.sim.as_mut() {
        p.params.insert("q".to_owned(), 0.0);
    }
    app.perform(vec![Action::SimChanged]);
    let page = app.sim_page().unwrap();
    assert!(page.result.is_none());
    let sentence = page.error.as_deref().unwrap_or_default();
    assert!(sentence.contains("pk1"), "{sentence}");
}

#[test]
fn the_palette_offers_both_two_compartment_pages() {
    let mut app = bolus_app();
    let entries = app.palette_entries();
    let find = |key: &str| entries.iter().find(|e| e.key == key).cloned();
    let fit = find("ui:new_fit_two").unwrap();
    assert_eq!(fit.label, "New two-compartment fit from pk2");
    assert_eq!(fit.action, Action::NewFitWith(Compartments::Two));
    let sim = find("ui:new_simulation_two").unwrap();
    assert_eq!(sim.action, Action::NewSimulationWith(Compartments::Two));
    app.state.palette.open = true;
    app.state.palette.query = "two-compartment".to_owned();
    let results = app.palette_results();
    assert!(results.iter().any(|e| e.key == "ui:new_fit_two"));
    app.perform(vec![Action::PaletteRun("ui:new_fit_two".to_owned())]);
    assert_eq!(app.fit_page().unwrap().model().id, "pk2.oral_1");
}

#[test]
fn the_two_compartment_screens_draw_in_both_themes_and_both_axes() {
    use crate::theme::ThemeMode;
    for dark in [false, true] {
        for log in [false, true] {
            let mut fitted = fit_page_two(bolus_app());
            if let Some(page) = fitted.state.fit.as_mut() {
                page.accept_placeholders();
            }
            fitted.perform(vec![Action::FitChanged { regenerate: false }]);
            let mut sim = UiApp::new();
            sim.perform(vec![Action::NewSimulationWith(Compartments::Two)]);
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
