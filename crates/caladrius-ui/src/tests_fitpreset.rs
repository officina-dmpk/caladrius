//! The preset choice in "More options" of the fit page (task T-044): the values are the engine's,
//! choosing one fills the option fields, an edit reads "Custom", and a stored fit reopens with the
//! label of its explicit values. No number is computed here.

use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

use crate::app::{Action, Selection, UiApp};
use crate::fit::FitPage;
use crate::fitpreset::{self, CUSTOM, KEYS};
use crate::tests::{harness, with_oral};

fn with_fit() -> UiApp {
    let mut app = with_oral();
    app.perform(vec![Action::NewFit]);
    app
}

fn preset(id: &str) -> &'static fitpreset::Preset {
    fitpreset::all().iter().find(|p| p.id == id).unwrap()
}

fn values(id: &str) -> Value {
    Value::Object(preset(id).options.clone())
}

/// The five settings of a page, as the page would send them to the engine.
fn settings(page: &FitPage) -> Value {
    let mut out = serde_json::Map::new();
    for key in KEYS {
        if let Some(v) = page.options.get(key) {
            out.insert(key.to_owned(), v.clone());
        }
    }
    Value::Object(out)
}

/// The five settings of an engine fit run with `options`, as stored in its analysis.
fn engine_settings(app: &mut UiApp, options: Value) -> Value {
    let page = app.fit_page().unwrap().clone();
    let mut params = page.run_params();
    params["options"] = options;
    let stored = app.engine_mut().execute("fit.run", params).unwrap();
    let mut out = serde_json::Map::new();
    for key in KEYS {
        out.insert(key.to_owned(), stored["spec"]["options"][key].clone());
    }
    Value::Object(out)
}

/// A window with "More options" open, so the preset choice is reachable.
fn open_more_options(app: UiApp) -> egui_kittest::Harness<'static, UiApp> {
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("More options").click();
    h.run_steps(3);
    h
}

/// Picks a preset by clicking its name.
fn choose(h: &mut egui_kittest::Harness<'static, UiApp>, wanted: &str) {
    h.get_by_label(wanted).click();
    h.run_steps(3);
}

/// The names of the preset choice that are selected right now (one, or none).
fn chosen(h: &egui_kittest::Harness<'static, UiApp>) -> Vec<String> {
    let mut names: Vec<String> = fitpreset::all().iter().map(|p| p.label.clone()).collect();
    names.push(CUSTOM.to_owned());
    names
        .into_iter()
        .filter(|name| {
            h.query_by_label(name).is_some_and(|n| {
                n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
            })
        })
        .collect()
}

#[test]
fn the_presets_are_the_ones_of_the_engines_schema_with_the_engines_values() {
    let described = caladrius_engine::describe();
    let schema = &described
        .iter()
        .find(|c| c.id == "fit.run")
        .unwrap()
        .params_schema;
    let ids: Vec<&str> = schema["$defs"]["FitOptions"]["properties"]["preset"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let ours: Vec<&str> = fitpreset::all().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ours);
    assert_eq!(ours, ["default", "reference_conventions"]);
    // Each preset is exactly what a fit asked for with `{"preset": id}` stores, on the page's own
    // engine and data, not only on the scratch profile.
    let mut app = with_fit();
    for id in ours {
        let stored = engine_settings(&mut app, json!({ "preset": id }));
        assert_eq!(stored, values(id), "{id}");
    }
    // The two presets differ in the derivatives, the increment and the stop (the point of them).
    let (a, b) = (preset("default"), preset("reference_conventions"));
    for key in ["derivatives", "increment", "convergence"] {
        assert_ne!(a.options[key], b.options[key], "{key}");
    }
    assert_eq!(a.options["max_iterations"], b.options["max_iterations"]);
}

#[test]
fn the_sentence_under_the_choice_is_written_from_the_engines_values() {
    let sentence = fitpreset::sentence();
    for p in fitpreset::all() {
        assert!(sentence.contains(&p.label), "{sentence}");
        let convergence = format!("{:e}", p.options["convergence"].as_f64().unwrap());
        assert!(
            sentence.contains(&convergence),
            "{convergence} in {sentence}"
        );
        let increment = format!("{:e}", p.options["increment"].as_f64().unwrap());
        assert!(sentence.contains(&increment), "{increment} in {sentence}");
    }
    assert!(sentence.contains("50 iterations at most"), "{sentence}");
    assert!(sentence.ends_with('.') && !sentence.contains(". "));
}

#[test]
fn a_new_fit_reads_as_the_default_preset() {
    let app = with_fit();
    let page = app.fit_page().unwrap();
    assert_eq!(fitpreset::matching(&page.options).unwrap().id, "default");
    let h = open_more_options(app);
    h.get_by_label("Preset");
    assert_eq!(chosen(&h), ["Caladrius default"]);
    h.get_by_label_contains("Reference conventions: forward differences of increment 1e-3");
}

#[test]
fn choosing_each_preset_fills_the_fields_with_the_engines_values() {
    let mut h = open_more_options(with_fit());
    choose(&mut h, "Reference conventions");
    let page = h.state().fit_page().unwrap().clone();
    assert_eq!(settings(&page), values("reference_conventions"));
    // The fields show the preset's values (read back from the page that draws them).
    assert_eq!(page.option_f64("/increment", f64::NAN), 0.001);
    assert_eq!(page.option_f64("/convergence", f64::NAN), 0.0001);
    assert_eq!(page.option_str("/derivatives", ""), "forward_difference");
    // The choice shows its name, and the fit that would run has these options.
    assert_eq!(chosen(&h), ["Reference conventions"]);
    assert_eq!(
        page.run_params()["options"]["convergence"],
        preset("reference_conventions").options["convergence"]
    );
    // And back.
    choose(&mut h, "Caladrius default");
    assert_eq!(chosen(&h), ["Caladrius default"]);
    let page = h.state().fit_page().unwrap().clone();
    assert_eq!(settings(&page), values("default"));
    assert_eq!(
        page.run_params()["options"]["convergence"],
        preset("default").options["convergence"]
    );
}

#[test]
fn choosing_a_preset_keeps_the_other_options() {
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        page.set_option(&["confidence_level"], json!(0.9));
        page.set_option(&["flags", "max_cv_percent"], json!(20.0));
    }
    let mut h = open_more_options(app);
    choose(&mut h, "Reference conventions");
    let page = h.state().fit_page().unwrap();
    assert_eq!(page.option_f64("/confidence_level", 0.0), 0.9);
    assert_eq!(page.option_f64("/flags/max_cv_percent", 0.0), 20.0);
    assert_eq!(page.option_str("/derivatives", ""), "forward_difference");
}

#[test]
fn an_edit_after_a_preset_reads_custom_and_editing_back_restores_the_name() {
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        assert!(fitpreset::apply(&mut page.options, "reference_conventions"));
    }
    let mut h = open_more_options(app);
    assert_eq!(chosen(&h), ["Reference conventions"]);
    if let Some(page) = h.state_mut().state.fit.as_mut() {
        page.set_option(&["max_iterations"], json!(80));
    }
    h.run_steps(3);
    assert_eq!(chosen(&h), [CUSTOM]);
    assert!(fitpreset::matching(&h.state().fit_page().unwrap().options).is_none());
    // The fields stay as they were but for the edit.
    let page = h.state().fit_page().unwrap();
    assert_eq!(page.option_f64("/increment", f64::NAN), 0.001);
    assert_eq!(page.option_f64("/max_iterations", 0.0), 80.0);
    // Back to the preset's value: the name returns.
    if let Some(page) = h.state_mut().state.fit.as_mut() {
        page.set_option(&["max_iterations"], json!(50));
    }
    h.run_steps(3);
    assert_eq!(chosen(&h), ["Reference conventions"]);
}

#[test]
fn a_stored_fit_with_explicit_values_reopens_with_the_right_label() {
    for (id, label) in [
        ("reference_conventions", "Reference conventions"),
        ("default", "Caladrius default"),
    ] {
        let mut app = with_fit();
        if let Some(page) = app.state.fit.as_mut() {
            assert!(fitpreset::apply(&mut page.options, id));
        }
        app.perform(vec![
            Action::FitChanged { regenerate: false },
            Action::RunFit,
        ]);
        let Selection::Analysis(analysis) = app.state.selection.clone() else {
            panic!("the fit page should now show the analysis");
        };
        // The stored analysis records the explicit values, not the preset's name.
        let spec = &app.fit_page().unwrap().view.as_ref().unwrap().spec;
        assert!(spec["options"].get("preset").is_none());
        assert_eq!(settings(app.fit_page().unwrap()), values(id));
        // Away and back: the page is read from the stored analysis.
        app.perform(vec![Action::Select(Selection::Worksheet(1))]);
        app.perform(vec![Action::Select(Selection::Analysis(analysis))]);
        let back = app.fit_page().unwrap();
        assert_eq!(back.analysis, Some(analysis));
        assert_eq!(fitpreset::matching(&back.options).unwrap().label, label);
        assert_eq!(chosen(&open_more_options(app)), [label]);
    }
}

#[test]
fn a_stored_fit_with_other_values_reopens_as_custom_and_an_old_one_as_the_default() {
    // Explicit values that are no preset.
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        page.set_option(&["increment"], json!(0.002));
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    let Selection::Analysis(analysis) = app.state.selection.clone() else {
        panic!("the fit page should now show the analysis");
    };
    app.perform(vec![Action::Select(Selection::Worksheet(1))]);
    app.perform(vec![Action::Select(Selection::Analysis(analysis))]);
    assert!(fitpreset::matching(&app.fit_page().unwrap().options).is_none());
    assert_eq!(chosen(&open_more_options(app)), [CUSTOM]);
    // An older project file that stored few options (the engine fills its defaults): the page
    // reads it as the default preset.
    let mut page = FitPage::new(1, String::new());
    page.options = json!({ "max_iterations": 50, "flags": { "max_cv_percent": 30.0 } });
    assert_eq!(fitpreset::matching(&page.options).unwrap().id, "default");
    page.options = Value::Null;
    assert_eq!(fitpreset::matching(&page.options).unwrap().id, "default");
}

#[test]
fn the_simulation_page_and_the_palette_have_no_preset() {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulation]);
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label("Preset").is_none());
    let lines = crate::palette::registry();
    assert!(lines.iter().all(|l| !l.id.contains("preset")));
}
