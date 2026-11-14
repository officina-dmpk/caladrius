//! The population-fit page (demo of 2026-11-16): the Theoph table without its observations at time
//! 0 (the demo dataset), the demo request run through the page's own actions, and the numbers the
//! page shows checked against `pop.fit` called directly on another engine: the same to the bit.
//! The second look in R appears for that reference case and not for another fit.

use caladrius_engine::Engine;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::app::{Action, Selection, UiApp};
use crate::fmt;
use crate::pop::{self, PopRun};
use crate::tests::harness;

/// The oracle's Theoph table in the event layout (12 subjects, a dose row each).
const THEOPH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/data/population/event_theoph.csv"
));

/// The table with the observation rows at time 0 left out (EVID 0 and TIME 0): the demo dataset,
/// 12 dose rows and 120 observations.
fn demo_csv() -> String {
    let mut out = String::new();
    for (i, line) in THEOPH.lines().enumerate() {
        let fields: Vec<&str> = line.split(',').collect();
        let observation_at_zero = i > 0
            && fields.get(4).map(|f| f.trim()) == Some("0")
            && fields
                .get(1)
                .and_then(|t| t.trim().parse::<f64>().ok())
                .is_some_and(|t| t == 0.0);
        if !observation_at_zero {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

thread_local! {
    static TICKS: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The program's clock in these tests: a quarter of a second more at each reading, so a call
/// timed by the page took 0.25 s exactly.
fn ticking() -> f64 {
    TICKS.with(|t| {
        let now = t.get() + 0.25;
        t.set(now);
        now
    })
}

/// An application with `csv` imported through the import screen, given the clock above.
fn imported(name: &str, csv: &str) -> UiApp {
    let mut app = UiApp::new();
    app.set_stopwatch(ticking);
    app.load_csv(name, csv.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    let sheet = app.overview.worksheets.first().cloned().unwrap();
    assert_eq!(sheet.subjects.len(), 12, "{:?}", app.notice_text());
    app
}

fn with_demo() -> UiApp {
    let app = imported("theophylline_pop.csv", &demo_csv());
    assert_eq!(app.overview.worksheets[0].rows, 132);
    app
}

/// The page's run after the actions a person takes: a new population fit, then Run.
fn run_demo() -> UiApp {
    let mut app = with_demo();
    app.perform(vec![Action::NewPopFit, Action::RunPopFit]);
    app
}

fn run_of(app: &UiApp) -> &PopRun {
    let page = app.pop_page().unwrap();
    assert_eq!(page.error, None);
    page.run.as_ref().unwrap()
}

fn without_analysis(mut v: Value) -> Value {
    if let Some(map) = v.as_object_mut() {
        map.remove("analysis");
    }
    v
}

/// The stored result has the answer's fields and its kind.
fn with_kind(mut v: Value) -> Value {
    if let Some(map) = v.as_object_mut() {
        map.insert("kind".to_owned(), json!("pop_fit"));
    }
    v
}

fn labels(h: &egui_kittest::Harness<'_, UiApp>, text: &str) -> usize {
    h.query_all_by_label(text).count()
}

#[test]
fn the_demo_table_is_the_theoph_table_without_its_samples_at_time_zero() {
    let csv = demo_csv();
    assert_eq!(csv.lines().count(), 1 + 12 + 120);
    assert_eq!(THEOPH.lines().count(), 1 + 12 + 132);
}

#[test]
fn the_demo_request_through_the_page_shows_the_engines_answer_bit_for_bit() {
    let mut app = run_demo();
    let run = run_of(&app);
    assert!(matches!(app.state.selection, Selection::Analysis(_)));
    // The request the page sent is the demo request.
    assert_eq!(
        run.request,
        json!({
            "worksheet": 1, "model": "pk1.oral_1", "random_effects": ["cl", "v", "ka"],
            "error_model": "proportional", "options": { "method": "foce_i" },
        })
    );
    // The same request to another engine, on the same table imported directly.
    let mut engine = Engine::new();
    engine
        .execute(
            "data.import",
            json!({ "name": "theophylline_pop", "csv": demo_csv() }),
        )
        .unwrap();
    let direct = engine.execute("pop.fit", run.request.clone()).unwrap();
    let ours = without_analysis(run.answer.clone());
    let theirs = without_analysis(direct.clone());
    assert_eq!(
        serde_json::to_string(&ours).unwrap(),
        serde_json::to_string(&theirs).unwrap()
    );

    // Every number of the tables is the engine's, to the bit.
    let order = ["cl", "v", "ka"];
    let s = pop::summary(&run.answer, &order);
    let bits = |x: Option<f64>| x.map(f64::to_bits);
    let at = |path: &str| direct.pointer(path).and_then(Value::as_f64);
    let names: Vec<&str> = s.typical.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, order);
    for e in &s.typical {
        assert_eq!(bits(e.value), bits(at(&format!("/estimate/{}", e.name))));
        assert_eq!(bits(e.se), bits(at(&format!("/se/{}", e.name))));
    }
    for (i, v) in s.variability.iter().enumerate() {
        assert_eq!(v.name, order[i]);
        for (got, path) in [
            (v.variance, format!("/estimate/omega_diagonal/{}", v.name)),
            (v.variance_se, format!("/se/omega_diagonal/{}", v.name)),
            (v.sd, format!("/estimate/omega_sd/{}", v.name)),
            (v.sd_se, format!("/se/omega_sd/{}", v.name)),
            (v.shrinkage, format!("/eta_shrinkage/{i}")),
        ] {
            assert_eq!(bits(got), bits(at(&path)), "{path}");
        }
    }
    assert_eq!(s.residual.len(), 1);
    assert_eq!(bits(s.residual[0].value), bits(at("/estimate/xi/b")));
    assert_eq!(bits(s.residual[0].se), bits(at("/se/xi/b")));
    for (got, path) in [
        (s.eps_shrinkage, "/eps_shrinkage"),
        (s.ofv, "/ofv"),
        (s.aic, "/aic"),
        (s.bic, "/bic"),
    ] {
        assert_eq!(bits(got), bits(at(path)), "{path}");
    }
    assert_eq!((s.n_subjects, s.n_observations), (Some(12), Some(120)));
    assert_eq!(s.status, "converged");
    assert_eq!(s.iterations, direct["iterations"].as_u64());

    // The result of the demo (the brief's figures, to their printed digits).
    let close = |x: Option<f64>, want: f64, tol: f64| {
        let x = x.unwrap();
        assert!((x - want).abs() <= tol, "{x} against {want}");
    };
    close(s.typical[0].value, 2.786, 5e-4);
    close(s.typical[1].value, 32.32, 5e-3);
    close(s.typical[2].value, 1.484, 5e-4);
    close(s.variability[0].variance, 0.0591, 5e-5);
    close(s.variability[1].variance, 0.0163, 5e-5);
    close(s.variability[2].variance, 0.418, 5e-4);
    close(s.residual[0].value, 0.1546, 5e-5);
    close(s.ofv, 346.29, 5e-3);
    close(s.aic, 360.29, 5e-3);
    close(s.bic, 379.80, 5e-3);
    close(s.variability[0].shrinkage, 0.003, 5e-4);
    close(s.variability[1].shrinkage, 0.147, 5e-4);
    close(s.variability[2].shrinkage, 0.033, 5e-4);
    close(s.eps_shrinkage, 0.099, 5e-4);

    // The wall time is the window's measure, kept beside the result and not in the analysis.
    assert_eq!(run.wall_seconds, Some(0.25));
    let id = app.pop_page().unwrap().analysis.unwrap();
    let stored = app
        .engine_mut()
        .execute("analysis.get", json!({ "analysis": id }))
        .unwrap();
    assert_eq!(
        serde_json::to_string(&without_analysis(stored["result"].clone())).unwrap(),
        serde_json::to_string(&with_kind(theirs)).unwrap()
    );

    // What the screen shows is the engine's number written by `fmt`.
    let mut h = harness(app);
    h.run();
    for x in [
        at("/estimate/cl"),
        at("/estimate/v"),
        at("/estimate/omega_diagonal/ka"),
        at("/estimate/xi/b"),
        at("/ofv"),
        at("/bic"),
        at("/eps_shrinkage"),
    ] {
        let text = fmt::number(x.unwrap());
        assert!(labels(&h, &text) >= 1, "{text} is not on the screen");
    }
    assert!(labels(&h, "in validation") == 1);
    assert!(labels(&h, "·  250 ms") == 1);
    assert!(labels(&h, "Converged") == 1);
}

#[test]
fn the_second_look_appears_for_the_reference_case_and_not_for_another_fit() {
    let app = run_demo();
    let look = run_of(&app).second_look.clone().unwrap();
    assert_eq!(look.case, "theoph_proportional");
    let mut h = harness(app);
    h.run();
    assert_eq!(
        labels(&h, "Second look in R"),
        2,
        "the heading and its link"
    );
    assert_eq!(labels(&h, "nlme 3.1.168 (Lindstrom-Bates)"), 1);
    assert_eq!(labels(&h, "saemix 3.5 (SAEM)"), 1);
    // nlme's Ω of ka, by its name `lka`, as the file writes it.
    assert!(labels(&h, &fmt::number(0.381401388765892)) >= 1);

    // The additive model on the whole table (the 12 samples at time 0 kept: 132 observations).
    let mut app = imported("theoph.csv", THEOPH);
    app.perform(vec![Action::NewPopFit]);
    if let Some(page) = app.state.pop.as_mut() {
        page.error_model = "additive".to_owned();
    }
    app.perform(vec![Action::RunPopFit]);
    let run = run_of(&app);
    assert_eq!(run.answer["n_observations"], 132);
    assert_eq!(run.second_look, None);
    let mut h = harness(app);
    h.run();
    assert_eq!(labels(&h, "Second look in R"), 0);
}

#[test]
fn a_refusal_says_what_to_change_and_stores_nothing() {
    let mut app = with_demo();
    app.perform(vec![Action::NewPopFit]);
    if let Some(page) = app.state.pop.as_mut() {
        page.set_random_effect("ka", false);
        assert_eq!(page.missing_values(), ["ka"]);
    }
    app.perform(vec![Action::RunPopFit]);
    let page = app.pop_page().unwrap();
    let error = page.error.clone().unwrap();
    assert!(
        error.contains("ka") && error.contains("give its value"),
        "{error}"
    );
    assert!(page.run.is_none() && page.analysis.is_none());
    assert!(app.overview.analyses.is_empty());
    assert_eq!(app.state.selection, Selection::NewPopFit);
    // A starting value that is not a number is refused by the page, before the engine.
    if let Some(page) = app.state.pop.as_mut() {
        page.initial.insert("ka".to_owned(), "fast".to_owned());
    }
    app.perform(vec![Action::RunPopFit]);
    assert!(
        app.pop_page()
            .unwrap()
            .error
            .clone()
            .unwrap()
            .contains("fast")
    );
    let mut h = harness(app);
    h.run();
}

#[test]
fn a_stored_fit_opens_with_its_result_and_the_palette_offers_a_new_one() {
    let mut app = run_demo();
    let id = app.pop_page().unwrap().analysis.unwrap();
    let before = pop::summary(&run_of(&app).answer, &["cl", "v", "ka"]);
    // Away to the worksheet, a new page, then back to the analysis from the tree.
    app.perform(vec![
        Action::Select(Selection::Worksheet(1)),
        Action::NewPopFit,
    ]);
    assert!(app.pop_page().unwrap().run.is_none());
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let page = app.pop_page().unwrap();
    assert_eq!(page.analysis, Some(id));
    assert_eq!(
        (page.model.as_str(), page.error_model.as_str()),
        ("pk1.oral_1", "proportional")
    );
    let run = run_of(&app);
    assert_eq!(run.wall_seconds, None);
    assert_eq!(pop::summary(&run.answer, &["cl", "v", "ka"]), before);
    assert_eq!(
        run.second_look.as_ref().map(|l| l.case.as_str()),
        Some("theoph_proportional")
    );
    // The palette.
    let entries = app.palette_entries();
    let entry = entries.iter().find(|e| e.key == "ui:new_pop_fit").unwrap();
    assert!(entry.label.starts_with("New population fit"));
    assert_eq!(entry.action, Action::NewPopFit);
    let command = entries.iter().find(|e| e.key == "cmd:pop.fit").unwrap();
    assert_eq!(command.action, Action::NewPopFit);
    app.perform(vec![Action::PaletteRun("ui:new_pop_fit".to_owned())]);
    assert_eq!(app.state.selection, Selection::NewPopFit);
    // The state of the page is data.
    let text = serde_json::to_string(&app.state).unwrap();
    let back: crate::app::UiState = serde_json::from_str(&text).unwrap();
    assert_eq!(back.pop.map(|p| p.model), Some("pk1.oral_1".to_owned()));
}

#[test]
fn a_new_project_leaves_no_population_page_behind() {
    let mut app = run_demo();
    app.perform(vec![Action::NewProject, Action::GuardDiscard]);
    assert!(app.overview.worksheets.is_empty() && app.overview.analyses.is_empty());
    assert!(app.pop_page().is_none());
    app.load_csv("theophylline_pop.csv", demo_csv().as_bytes());
    app.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
    assert!(app.pop_page().is_none());
    assert!(app.nca_page().is_some());
    let mut h = harness(app);
    h.run();
}

#[test]
fn every_part_draws_after_fo_in_both_themes_and_both_axes() {
    let mut app = with_demo();
    app.perform(vec![Action::NewPopFit]);
    if let Some(page) = app.state.pop.as_mut() {
        page.method = "fo".to_owned();
    }
    app.perform(vec![Action::RunPopFit]);
    let run = run_of(&app);
    assert_eq!(run.answer["method"], "fo");
    assert!(
        pop::subjects(&run.answer)
            .iter()
            .all(|s| s.observations.iter().all(|o| o.ipred.is_none()))
    );
    let mut h = harness(app);
    for (dark, log) in [(false, false), (true, true), (false, true)] {
        h.state_mut().state.mode = if dark {
            crate::theme::ThemeMode::Dark
        } else {
            crate::theme::ThemeMode::Light
        };
        h.state_mut().state.log_axis = log;
        if let Some(page) = h.state_mut().state.pop.as_mut() {
            page.jump = Some(pop::Section::Individual);
        }
        h.run();
    }
    assert!(h.get_all_by_label("No individual prediction after FO (it fits with η = 0): run FOCE or FOCEI for this plot.").count() == 3);
}
