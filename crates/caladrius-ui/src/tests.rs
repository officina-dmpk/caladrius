//! Tests of the application as a whole: the flows through the engine, the plot's safety on zeros,
//! the rules of the UI contract (no colour outside the theme tokens, state as data), and the
//! screens drawn by a headless harness without a panic.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::app::{Action, Request, Selection, UiApp, UiState};
use crate::model::Status;
use crate::theme::ThemeMode;

const ORAL: &str = "Time (h),Conc (mg/L),Dose (mg)\n0,0,100\n0.25,1.279,100\n0.5,2.195,100\n1,3.293,100\n2,3.971,100\n4,3.611,100\n6,2.989,100\n8,2.451,100\n12,1.643,100\n24,0.495,100\n";

fn with_oral() -> UiApp {
    let mut app = UiApp::new();
    app.load_csv("oral.csv", ORAL.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

fn with_analysis() -> UiApp {
    let mut app = with_oral();
    app.perform(vec![Action::NewAnalysis]);
    app
}

fn number(app: &UiApp, name: &str) -> Option<f64> {
    let page = app.nca_page()?;
    let subject = page.view.as_ref()?.subject.as_ref()?;
    match &subject.outcome {
        crate::model::Outcome::Ok(ok) => ok.number(name),
        _ => None,
    }
}

// ---- the flow of an import ---------------------------------------------------------------

#[test]
fn an_import_stores_nothing_until_it_is_confirmed() {
    let mut app = UiApp::new();
    app.load_csv("study.csv", ORAL.as_bytes());
    assert_eq!(app.state.selection, Selection::Import);
    let pending = app.pending_import().unwrap();
    assert_eq!(pending.readings.len(), 1);
    assert!(pending.can_import());
    // The engine has seen the preview and nothing else.
    assert!(app.engine().project().worksheets().is_empty());
    app.perform(vec![Action::ImportConfirm]);
    assert_eq!(app.engine().project().worksheets().len(), 1);
    assert_eq!(app.state.selection, Selection::Worksheet(1));
    assert!(app.pending_import().is_none());
    // Cancelling an import leaves the project as it was.
    app.load_csv("again.csv", ORAL.as_bytes());
    app.perform(vec![Action::ImportCancel]);
    assert_eq!(app.engine().project().worksheets().len(), 1);
}

#[test]
fn a_decimal_comma_file_is_read_with_the_right_marks_and_shown_before_import() {
    let mut app = UiApp::new();
    app.load_csv(
        "fr.csv",
        "Temps (h);Concentration (mg/L)\n0;0\n0,25;1,279\n0,5;2,195\n".as_bytes(),
    );
    let reading = app
        .pending_import()
        .unwrap()
        .chosen_reading()
        .unwrap()
        .clone();
    assert_eq!(
        (reading.delimiter.as_str(), reading.decimal_comma),
        (";", true)
    );
    assert_eq!(
        reading.preview.get(1),
        Some(&vec![json!(0.25), json!(1.279)])
    );
    app.perform(vec![Action::ImportConfirm]);
    let table = app
        .engine_mut()
        .execute(
            "export.table",
            json!({ "table": "worksheet", "worksheet": 1 }),
        )
        .unwrap();
    assert_eq!(table["rows"][1], json!([0.25, 1.279]));
}

#[test]
fn a_reading_with_a_check_is_not_imported_without_a_yes() {
    let mut app = UiApp::new();
    app.load_csv("quarter.csv", b"time,conc\n0,25\n0,5\n1,2\n2,1\n");
    assert!(app.pending_import().unwrap().needs_choice);
    app.perform(vec![Action::ImportConfirm]);
    assert!(
        app.engine().project().worksheets().is_empty(),
        "no yes, no import"
    );
    assert!(app.notice_text().unwrap().contains("checks"));
    // The person ticks the box: now it goes through.
    let mut pending = app.pending_import().unwrap().clone();
    assert!(!pending.can_import());
    pending.acknowledged = true;
    assert!(pending.can_import());
}

#[test]
fn changing_the_separator_reads_the_file_again() {
    use crate::import::ReadOptions;
    let mut app = UiApp::new();
    app.load_csv("x.csv", b"time;conc\n0,25;3,4\n0,5;5,1\n");
    // Forcing a point as the decimal mark leaves no usable reading: the error says what to change.
    let mut pending = app.pending_import().unwrap().clone();
    pending.options = ReadOptions {
        delimiter: Some(';'),
        decimal_comma: Some(false),
    };
    let answer = app
        .engine_mut()
        .execute("data.preview", pending.preview_params())
        .map_err(|e| e.message);
    pending.adopt(answer);
    assert!(pending.readings.is_empty());
    assert!(pending.error.as_deref().is_some_and(|e| !e.is_empty()));
}

#[test]
fn a_file_that_is_not_text_is_refused_with_what_to_do() {
    let mut app = UiApp::new();
    app.load_csv("bad.csv", &[0xff, 0xfe, 0x00]);
    assert!(app.pending_import().is_none());
    assert!(app.notice_text().unwrap().contains("UTF-8"));
    assert_eq!(app.state.selection, Selection::Welcome);
}

// ---- the NCA page ------------------------------------------------------------------------

#[test]
fn a_new_analysis_runs_at_once_and_shows_its_result() {
    let app = with_analysis();
    let page = app.nca_page().unwrap();
    assert_eq!(page.worksheet, 1);
    let view = page.view.as_ref().unwrap();
    assert_eq!(view.status, Status::Fresh);
    assert!(view.label.starts_with("NCA of oral"), "{}", view.label);
    assert!((number(&app, "cmax").unwrap() - 3.971).abs() < 1e-9);
    assert_eq!(app.state.selection, Selection::Analysis(view.id));
    // The options on the page are the engine's own, defaults included.
    assert_eq!(page.options["lambda_z"]["min_points"], 3);
}

#[test]
fn an_option_change_asks_the_engine_again_and_the_numbers_follow() {
    let mut app = with_analysis();
    let log_down = number(&app, "auclast").unwrap();
    let mut page = app.state.nca.clone().unwrap();
    page.options["auc_method"] = json!("linear");
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    let linear = number(&app, "auclast").unwrap();
    assert!(
        linear > log_down,
        "linear trapezoids overestimate a falling curve: {linear} {log_down}"
    );
    // The analysis stored by the engine has the new options and is fresh.
    let spec = &app.engine().project().analyses()[0];
    assert_eq!(
        app.engine().project().status(spec.id()).unwrap(),
        caladrius_engine::project::AnalysisStatus::Fresh
    );
}

#[test]
fn clicking_points_chooses_the_terminal_phase_by_hand() {
    let mut app = with_analysis();
    let automatic = app.nca_page().unwrap().used_times();
    assert_eq!(automatic, vec![4.0, 6.0, 8.0, 12.0, 24.0]);
    // A click on the point at 4 h takes it out; the engine refits with the four that remain.
    let mut page = app.state.nca.clone().unwrap();
    page.toggle_point(4.0).unwrap();
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    let page = app.nca_page().unwrap();
    assert!(page.manual());
    assert_eq!(page.used_times(), vec![6.0, 8.0, 12.0, 24.0]);
    let selected = page
        .view
        .as_ref()
        .unwrap()
        .subject
        .as_ref()
        .and_then(|s| match &s.outcome {
            crate::model::Outcome::Ok(ok) => ok.selected().cloned(),
            _ => None,
        });
    assert_eq!(selected.unwrap().n_points, 4);
    // A click on a point outside adds it back.
    let mut page = page.clone();
    page.toggle_point(4.0).unwrap();
    assert_eq!(page.used_times(), vec![4.0, 6.0, 8.0, 12.0, 24.0]);
    // Down to two points is the floor: the third removal is refused and nothing changes.
    let mut page = app.nca_page().unwrap().clone();
    page.toggle_point(6.0).unwrap();
    page.toggle_point(8.0).unwrap();
    let before = page.clone();
    let refused = page.toggle_point(12.0).unwrap_err();
    assert!(refused.contains("at least 2"), "{refused}");
    assert_eq!(page, before);
    // Back to automatic.
    let mut page = app.nca_page().unwrap().clone();
    page.options["lambda_z_selection"]["manual"] = serde_json::Value::Null;
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    assert!(!app.nca_page().unwrap().manual());
    assert_eq!(app.nca_page().unwrap().used_times(), automatic);
}

#[test]
fn a_click_on_a_hand_picked_point_that_the_engine_refuses_shows_its_message() {
    let mut app = with_analysis();
    let mut page = app.state.nca.clone().unwrap();
    // The point at time 0 has concentration 0: it cannot be in a log-linear fit.
    page.options["lambda_z_selection"]["manual"] = json!({ "times": [0.0, 24.0] });
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    let page = app.nca_page().unwrap();
    // The engine answers for the subject: the message is in the result, and the way back is offered.
    let outcome = &page
        .view
        .as_ref()
        .unwrap()
        .subject
        .as_ref()
        .unwrap()
        .outcome;
    assert!(
        matches!(outcome, crate::model::Outcome::Error(m) if !m.is_empty()),
        "{outcome:?}"
    );
    assert!(page.manual());
    let mut page = page.clone();
    page.options["lambda_z_selection"]["manual"] = serde_json::Value::Null;
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    assert!(matches!(
        app.nca_page()
            .unwrap()
            .view
            .as_ref()
            .unwrap()
            .subject
            .as_ref()
            .unwrap()
            .outcome,
        crate::model::Outcome::Ok(_)
    ));
}

#[test]
fn editing_the_data_marks_the_analysis_stale_and_run_again_refreshes_it() {
    let mut app = with_analysis();
    let id = app.nca_page().unwrap().analysis.unwrap();
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 4,
        column: "Conc".to_owned(),
        text: "4,2".to_owned(),
    }]);
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let page = app.nca_page().unwrap();
    assert!(page.view.as_ref().unwrap().status.is_stale());
    let sentence = page.view.as_ref().unwrap().status.sentence().unwrap();
    assert!(sentence.contains("the data changed"), "{sentence}");
    // The old result is still shown (Cmax of the old data) until the person runs it again.
    assert!((number(&app, "cmax").unwrap() - 3.971).abs() < 1e-9);
    app.perform(vec![Action::RunAgain(id)]);
    assert_eq!(
        app.nca_page().unwrap().view.as_ref().unwrap().status,
        Status::Fresh
    );
    assert!(
        (number(&app, "cmax").unwrap() - 4.2).abs() < 1e-9,
        "the comma was read as a decimal mark"
    );
}

#[test]
fn a_refused_cell_entry_is_kept_and_explained() {
    let mut app = with_oral();
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 2,
        column: "Conc".to_owned(),
        text: "1.2.3".to_owned(),
    }]);
    let message = app.notice_text().unwrap().to_owned();
    assert!(
        message.contains("Conc") && message.contains("decimal mark"),
        "{message}"
    );
    // The worksheet is unchanged.
    let table = app
        .engine_mut()
        .execute(
            "export.table",
            json!({ "table": "worksheet", "worksheet": 1 }),
        )
        .unwrap();
    assert_eq!(table["rows"][2][1], json!(2.195));
}

#[test]
fn a_subject_change_forgets_the_hand_picked_phase_of_the_previous_subject() {
    let mut app = UiApp::new();
    app.load_csv(
        "two.csv",
        b"id,time,conc,dose\n1,0,0,10\n1,1,8,10\n1,2,6,10\n1,4,3,10\n1,6,1.5,10\n1,8,0.7,10\n2,0,0,10\n2,1,7,10\n2,2,5,10\n2,4,2.5,10\n2,6,1.2,10\n2,8,0.6,10\n",
    );
    app.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
    let mut page = app.state.nca.clone().unwrap();
    page.toggle_point(2.0).unwrap();
    assert!(page.manual());
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    assert!(app.nca_page().unwrap().manual());
    // What the subject combo does on a change:
    let mut page = app.state.nca.clone().unwrap();
    page.subject = "2".to_owned();
    page.options["lambda_z_selection"]["manual"] = serde_json::Value::Null;
    app.state.nca = Some(page);
    app.perform(vec![Action::RunNca]);
    assert!(!app.nca_page().unwrap().manual());
    assert_eq!(
        app.nca_page()
            .unwrap()
            .view
            .as_ref()
            .unwrap()
            .subject
            .as_ref()
            .unwrap()
            .subject,
        "2"
    );
}

// ---- contract ----------------------------------------------------------------------------

#[test]
fn the_state_of_the_ui_round_trips_as_data() {
    let mut app = with_analysis();
    app.state.mode = ThemeMode::Dark;
    app.state.log_axis = true;
    let text = serde_json::to_string(&app.state).unwrap();
    let back: UiState = serde_json::from_str(&text).unwrap();
    // The cache of the engine's last answer is not state; everything else comes back.
    assert_eq!(back.mode, app.state.mode);
    assert_eq!(back.selection, app.state.selection);
    assert_eq!(back.log_axis, app.state.log_axis);
    let (a, b) = (back.nca.unwrap(), app.state.nca.clone().unwrap());
    assert_eq!(
        (a.analysis, a.worksheet, &a.subject, a.route),
        (b.analysis, b.worksheet, &b.subject, b.route)
    );
    assert_eq!(a.options, b.options);
}

#[test]
fn no_colour_radius_or_margin_is_written_outside_the_theme() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        "Color32::",
        "from_rgb",
        "from_gray",
        "from_black_alpha",
        "from_white_alpha",
        "Rgba(",
        "CornerRadius::same(",
        "Margin::same(",
        "Margin::symmetric(",
        "rounding(",
        "corner_radius(",
    ];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "theme.rs" || name == "tests.rs" || !name.ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // Ignore the test modules at the end of each file.
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for pattern in forbidden {
            assert!(
                !code.contains(pattern),
                "{name} writes `{pattern}`: use a theme token"
            );
        }
    }
}

/// The text inside the parentheses of each call of `prefix` in `code` (balanced).
fn call_arguments<'a>(code: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find(prefix) {
        let start = from + at + prefix.len();
        let mut depth = 1;
        let mut end = start;
        for (i, c) in code[start..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(&code[start..end]);
        from = start;
    }
    out
}

/// True when `text` holds a number written in the code: a digit that does not belong to a name.
fn has_number_literal(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.iter().enumerate().any(|(i, c)| {
        c.is_ascii_digit()
            && !chars
                .get(i.wrapping_sub(1))
                .is_some_and(|p| p.is_alphanumeric() || *p == '_' || *p == '.')
    })
}

#[test]
fn sizes_and_spacing_are_tokens_not_numbers_in_the_screens() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let calls = [
        "add_space(",
        ".width(",
        ".size(",
        ".desired_width(",
        ".max_height(",
        ".min_height(",
        ".min_width(",
        ".default_width(",
        ".exact_width(",
        ".width_range(",
        ".add_sized(",
        ".height(",
        ".radius(",
        "Stroke::new(",
        "vec2(",
        ".spacing(",
        ".indent(",
    ];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "theme.rs" || name == "tests.rs" || !name.ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for call in calls {
            for args in call_arguments(code, call) {
                // `add_sized(size, widget)`: only the size is a layout number.
                let args = if call == ".add_sized(" {
                    args.split(']').next().unwrap_or(args)
                } else {
                    args
                };
                assert!(
                    !has_number_literal(args),
                    "{name}: `{call}{args})` has a number written in the code: use a size, spacing, stroke or font token"
                );
            }
        }
    }
}

#[test]
fn the_literal_finder_sees_numbers_and_not_names() {
    assert!(has_number_literal("tokens.spacing.large * 2.0"));
    assert!(has_number_literal("96"));
    assert!(has_number_literal("[40.0, row]"));
    assert!(!has_number_literal("tokens.size.cell_width"));
    assert!(!has_number_literal("self.tokens.stroke.thin"));
    assert!(!has_number_literal("x1 + a_2"));
    assert_eq!(call_arguments("f(a(1), b) g(c)", "f("), vec!["a(1), b"]);
}

#[test]
fn the_code_of_the_ui_has_no_file_or_clock_access() {
    // L3 must compile for wasm (golden rule 9): no files, threads, processes or clock here.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "tests.rs" || !name.ends_with(".rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for pattern in [
            "std::fs",
            "std::process",
            "std::thread",
            "std::time",
            "std::env",
            "File::open",
        ] {
            assert!(!code.contains(pattern), "{name} uses `{pattern}`");
        }
    }
}

#[test]
fn the_app_asks_for_a_file_picker_instead_of_opening_files() {
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenCsv]);
    assert_eq!(app.take_requests(), vec![Request::PickCsv]);
    assert!(app.take_requests().is_empty());
}

// ---- drawn by a harness ------------------------------------------------------------------

fn harness(app: UiApp) -> Harness<'static, UiApp> {
    Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_state(|ctx, app: &mut UiApp| app.ui(ctx), app)
}

#[test]
fn every_screen_draws_without_a_panic_in_both_themes_and_both_axes() {
    for dark in [false, true] {
        for log in [false, true] {
            let mut scenes: Vec<UiApp> = Vec::new();
            scenes.push(UiApp::new());
            let mut importing = UiApp::new();
            importing.load_csv("quarter.csv", b"time,conc\n0,25\n0,5\n1,2\n");
            scenes.push(importing);
            scenes.push(with_oral());
            scenes.push(with_analysis());
            let mut zeros = UiApp::new();
            zeros.load_csv("z.csv", b"time,conc,dose\n0,0,10\n1,0,10\n2,0,10\n3,0,10\n");
            zeros.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
            scenes.push(zeros);
            for mut app in scenes {
                app.state.mode = if dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                };
                app.state.log_axis = log;
                let mut h = harness(app);
                h.run_steps(3);
            }
        }
    }
}

#[test]
fn the_semi_log_toggle_never_fails_on_zeros_and_says_what_is_hidden() {
    // Every concentration is zero: the log view has nothing to draw and says why.
    let mut app = UiApp::new();
    app.load_csv("z.csv", b"time,conc,dose\n0,0,10\n1,0,10\n2,0,10\n3,0,10\n");
    app.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
    app.state.log_axis = true;
    let mut h = harness(app);
    h.run_steps(3);
    // A profile with one zero: the note counts it.
    let mut app = with_analysis();
    app.state.log_axis = true;
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("1 of 10 points not shown on a log axis")
            .is_some()
    );
    // The data was not changed by looking at it on a log axis.
    let table = h
        .state_mut()
        .engine_mut()
        .execute(
            "export.table",
            json!({ "table": "worksheet", "worksheet": 1 }),
        )
        .unwrap();
    assert_eq!(table["rows"][0], json!([0.0, 0.0, 100.0]));
}

#[test]
fn the_buttons_do_what_they_say() {
    let mut h = harness(UiApp::new());
    h.run_steps(2);
    h.get_by_label("Open CSV…").click();
    h.run_steps(2);
    assert_eq!(h.state_mut().take_requests(), vec![Request::PickCsv]);
    h.get_by_label("Dark theme").click();
    h.run_steps(3);
    assert_eq!(h.state().state.mode, ThemeMode::Dark);

    let mut h = harness(with_analysis());
    h.run_steps(2);
    h.get_by_label("Semi-log").click();
    h.run_steps(2);
    assert!(h.state().state.log_axis);
    h.get_by_label("Linear").click();
    h.run_steps(2);
    assert!(!h.state().state.log_axis);
}

#[test]
fn the_number_the_engine_understood_is_shown_when_it_differs_from_what_was_typed() {
    let mut app = with_oral();
    let set = |app: &mut UiApp, text: &str| {
        app.perform(vec![Action::SetCell {
            worksheet: 1,
            row: 2,
            column: "Conc".to_owned(),
            text: text.to_owned(),
        }]);
    };
    set(&mut app, "3,25");
    assert_eq!(app.notice_text(), Some("Read `3,25` as 3.25."));
    set(&mut app, "NA");
    assert_eq!(app.notice_text(), Some("Read `NA` as a missing value."));
    // What was typed as the engine writes it needs no remark, and the old remark goes away.
    set(&mut app, "2.5");
    assert_eq!(app.notice_text(), None);
    // A refused entry shows the engine's message instead.
    set(&mut app, "1,500");
    assert!(app.notice_text().unwrap().contains("thousands separator"));
}

// ---- model fit and simulation ------------------------------------------------------------

fn with_fit() -> UiApp {
    let mut app = with_oral();
    app.perform(vec![Action::NewFit]);
    app
}

fn fitted() -> UiApp {
    let mut app = with_fit();
    app.perform(vec![Action::RunFit]);
    app
}

#[test]
fn a_new_fit_prefills_the_starting_values_and_draws_the_live_curve() {
    let mut app = with_fit();
    assert_eq!(app.state.selection, Selection::NewFit);
    let page = app.fit_page().unwrap().clone();
    assert_eq!(page.model().id, "pk1.oral_1");
    // The starting values are the ones the engine generates from the data.
    let generated = app
        .engine_mut()
        .execute("fit.initial_estimates", page.initial_params())
        .unwrap();
    for name in ["v", "k", "ka"] {
        assert_eq!(
            page.initial.get(name).copied(),
            generated["initial"][name].as_f64()
        );
    }
    // The curve and the objective come from the engine's commands, at the starting values.
    assert_eq!(
        page.preview.curve.len(),
        crate::fit::defaults::PREVIEW_POINTS
    );
    let simulated = app
        .engine_mut()
        .execute("model.simulate", page.simulate_params(24.0).unwrap())
        .unwrap();
    let curve: crate::model::Simulation = crate::model::read(&simulated);
    assert_eq!(page.preview.curve, curve.curve());
    let evaluated = app
        .engine_mut()
        .execute("fit.evaluate", page.evaluate_params())
        .unwrap();
    assert_eq!(page.preview.wrss, evaluated["wrss"].as_f64());
    assert_eq!(page.preview.dose, Some(100.0));
    // Nothing was stored by looking: the project holds the worksheet only.
    assert!(app.engine().project().analyses().is_empty());
}

#[test]
fn editing_a_starting_value_moves_the_curve_and_the_objective() {
    let mut app = with_fit();
    let before = app.fit_page().unwrap().preview.clone();
    if let Some(page) = app.state.fit.as_mut() {
        if let Some(v) = page.initial.get_mut("ka") {
            *v *= 3.0;
        }
    }
    app.perform(vec![Action::FitChanged { regenerate: false }]);
    let after = app.fit_page().unwrap().preview.clone();
    assert_ne!(before.curve, after.curve);
    assert!(
        after.wrss.unwrap() > before.wrss.unwrap(),
        "a worse start costs more"
    );
    // The values the person typed survive; they are not generated again.
    assert!(app.fit_page().unwrap().initial["ka"] > 3.0);
}

#[test]
fn fitting_stores_an_analysis_and_shows_estimates_with_precision_and_both_intervals() {
    let mut app = fitted();
    let Selection::Analysis(id) = app.state.selection.clone() else {
        panic!("the fit page should now show the analysis");
    };
    assert_eq!(app.engine().project().analyses().len(), 1);
    let page = app.fit_page().unwrap().clone();
    let ok = page.ok().unwrap();
    assert_eq!(ok.status, "converged");
    assert!((ok.value("estimate.v").unwrap() / 20.0 - 1.0).abs() < 0.01);
    for key in [
        "se.v",
        "cv_percent.v",
        "ci_lo.v",
        "ci_hi.v",
        "planar_lo.v",
        "planar_hi.v",
        "estimate.cl",
        "estimate.half_life",
        "wrss",
        "aic",
    ] {
        assert!(ok.value(key).is_some(), "{key}");
    }
    assert_eq!(page.view.as_ref().unwrap().status_message, "converged");
    // The page comes back from the stored analysis, with the values it was run from.
    app.perform(vec![Action::Select(Selection::Worksheet(1))]);
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let back = app.fit_page().unwrap();
    assert_eq!(back.analysis, Some(id));
    assert_eq!(back.initial, page.initial);
    assert_eq!(back.model().id, "pk1.oral_1");
    assert!(back.ok().is_some());
    assert!(app.nca_page().is_none(), "the NCA page is not left behind");
}

#[test]
fn a_poor_fit_is_flagged_with_sentences_that_say_what_to_check() {
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        page.input = crate::modelinfo::Input::Bolus;
    }
    app.perform(vec![
        Action::FitChanged { regenerate: true },
        Action::RunFit,
    ]);
    let page = app.fit_page().unwrap();
    let view = page.view.as_ref().unwrap();
    assert!(!view.flag_messages.is_empty());
    assert!(view.ok().unwrap().flags.iter().any(|f| f.code == "high_cv"));
    assert!(view.flag_messages.iter().any(|m| m.contains("CV")));
}

#[test]
fn the_route_picks_the_model_and_the_starting_values_follow() {
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        page.lag = true;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    let page = app.fit_page().unwrap();
    assert_eq!(page.model().id, "pk1.oral_1_lag");
    assert!(page.initial.contains_key("tlag"));
    // An infusion has a fixed duration: it is not among the fitted parameters.
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        page.input = crate::modelinfo::Input::Infusion;
        page.duration = 0.5;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    let page = app.fit_page().unwrap();
    assert_eq!(page.model().id, "pk1.iv_infusion");
    assert!(!page.initial.contains_key("dur") && page.initial.contains_key("v"));
    assert_eq!(page.run_params()["options"]["fixed"]["dur"], 0.5);
    assert!(page.preview.curve.len() > 2, "{:?}", page.preview.error);
}

const NO_DOSE: &[u8] = b"time,conc\n0,0\n0.5,2.2\n1,3.3\n2,4\n4,3.6\n8,2.5\n24,0.5\n";

#[test]
fn a_worksheet_without_a_dose_says_what_to_do_and_the_person_can_enter_one() {
    let mut app = UiApp::new();
    app.load_csv("nodose.csv", NO_DOSE);
    app.perform(vec![Action::ImportConfirm, Action::NewFit]);
    let page = app.fit_page().unwrap();
    assert!(page.initial.is_empty() || page.start_error.is_some());
    assert!(page.start_error.as_deref().unwrap().contains("dose"));
    assert!(page.preview.curve.is_empty());
    // Both states draw.
    let mut h = harness(app);
    h.run_steps(3);
    let mut app = UiApp::new();
    app.load_csv("nodose.csv", NO_DOSE);
    app.perform(vec![Action::ImportConfirm, Action::NewFit]);
    // Entering the dose here fixes it.
    if let Some(page) = app.state.fit.as_mut() {
        page.dose_override = Some(100.0);
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    let page = app.fit_page().unwrap();
    assert!(page.start_error.is_none() && !page.initial.is_empty());
    assert!(!page.preview.curve.is_empty());
    harness(app).run_steps(3);
}

#[test]
fn too_few_points_give_a_sentence_not_a_crash() {
    let mut app = UiApp::new();
    app.load_csv("two.csv", b"time,conc,dose\n1,5,10\n2,3,10\n");
    app.perform(vec![Action::ImportConfirm, Action::NewFit]);
    let page = app.fit_page().unwrap();
    assert!(page.start_error.is_some() || page.preview.error.is_some());
    app.perform(vec![Action::RunFit]);
    let page = app.fit_page().unwrap();
    assert!(page.error.is_some() || page.ok().is_some());
    let mut h = harness(app);
    h.run_steps(3);
}

#[test]
fn a_data_edit_makes_the_fit_stale_and_run_again_refreshes_it() {
    let mut app = fitted();
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 4,
        column: "Conc".to_owned(),
        text: "4,2".to_owned(),
    }]);
    assert!(app.fit_page().unwrap().preview.wrss.is_some());
    let id = app.fit_page().unwrap().analysis.unwrap();
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    assert!(app.fit_page().unwrap().status().is_stale());
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_all_by_label_contains("Out of date").count() >= 1);
    h.get_by_label("Run again").click();
    h.run_steps(3);
    assert_eq!(h.state().fit_page().unwrap().status(), Status::Fresh);
}

#[test]
fn the_defaults_the_page_shows_are_the_engines() {
    use crate::fit::defaults;
    let app = fitted();
    let options = &app.fit_page().unwrap().view.as_ref().unwrap().spec["options"];
    assert_eq!(options["max_iterations"], defaults::MAX_ITERATIONS);
    assert_eq!(options["convergence"], defaults::CONVERGENCE);
    assert_eq!(options["increment"], defaults::INCREMENT);
    assert_eq!(options["confidence_level"], defaults::CONFIDENCE_LEVEL);
    assert_eq!(options["flags"]["max_cv_percent"], defaults::MAX_CV_PERCENT);
    assert_eq!(
        options["flags"]["max_abs_correlation"],
        defaults::MAX_ABS_CORRELATION
    );
    assert_eq!(
        options["flags"]["max_condition_number"],
        defaults::MAX_CONDITION_NUMBER
    );
    assert_eq!(
        options["flags"]["min_degrees_of_freedom"].as_f64(),
        Some(defaults::MIN_DEGREES_OF_FREEDOM)
    );
}

#[test]
fn options_and_weighting_reach_the_engine() {
    // No zero concentration, so every weighting can be used.
    let mut app = UiApp::new();
    app.load_csv(
        "positive.csv",
        b"time,conc,dose
0.25,1.279,100
0.5,2.195,100
1,3.293,100
2,3.971,100
4,3.611,100
6,2.989,100
8,2.451,100
12,1.643,100
24,0.495,100
",
    );
    app.perform(vec![Action::ImportConfirm, Action::NewFit]);
    if let Some(page) = app.state.fit.as_mut() {
        page.weighting = "inv_y".to_owned();
        page.set_option(&["max_iterations"], json!(7));
        page.set_option(&["flags", "max_cv_percent"], json!(0.5));
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    let page = app.fit_page().unwrap();
    assert!(page.error.is_none(), "{:?}", page.error);
    let spec = &page.view.as_ref().unwrap().spec;
    assert_eq!(spec["weighting"], "inv_y");
    assert_eq!(spec["options"]["max_iterations"], 7);
    assert_eq!(spec["options"]["flags"]["max_cv_percent"], 0.5);
}

#[test]
fn a_weighting_the_data_cannot_take_says_what_to_change() {
    let mut app = with_fit();
    if let Some(page) = app.state.fit.as_mut() {
        page.weighting = "inv_y".to_owned();
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    let page = app.fit_page().unwrap();
    let message = page.error.as_deref().unwrap();
    assert!(message.contains("choose another weighting"), "{message}");
    assert!(page.view.is_none() && app.engine().project().analyses().is_empty());
    harness(app).run_steps(3);
}

#[test]
fn the_simulation_page_draws_the_engines_curve_and_saves_an_analysis() {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulation]);
    assert_eq!(app.state.selection, Selection::NewSimulation);
    let page = app.sim_page().unwrap().clone();
    let result = page.result.clone().unwrap();
    assert_eq!(result.times.len(), crate::fit::defaults::PREVIEW_POINTS);
    let direct = app
        .engine_mut()
        .execute("model.simulate", page.params(false))
        .unwrap();
    assert_eq!(
        crate::model::read::<crate::model::Simulation>(&direct).conc,
        result.conc
    );
    assert!(result.secondary.contains_key("half_life"));
    assert!(app.engine().project().analyses().is_empty());
    // A change of parameter changes the curve.
    if let Some(p) = app.state.sim.as_mut() {
        p.params.insert("k".to_owned(), 0.3);
    }
    app.perform(vec![Action::SimChanged]);
    assert_ne!(
        app.sim_page().unwrap().result.as_ref().unwrap().conc,
        result.conc
    );
    // Saving stores a simulation analysis, which opens as the same page.
    app.perform(vec![Action::SimSave]);
    assert_eq!(app.engine().project().analyses().len(), 1);
    let Selection::Analysis(id) = app.state.selection.clone() else {
        panic!("a saved simulation is shown as an analysis");
    };
    app.perform(vec![Action::Select(Selection::Welcome)]);
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let back = app.sim_page().unwrap();
    assert_eq!(back.params["k"], 0.3);
    assert_eq!(back.analysis, Some(id));
}

#[test]
fn a_simulation_with_odd_values_gives_a_curve_or_a_sentence_never_a_crash() {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulation]);
    if let Some(p) = app.state.sim.as_mut() {
        p.params.insert("ka".to_owned(), 0.1);
        p.end = 1.0e-9;
    }
    app.perform(vec![Action::SimChanged]);
    let page = app.sim_page().unwrap();
    assert!(page.result.is_some() || page.error.is_some());
    harness(app).run_steps(3);
}

#[test]
fn the_new_analysis_menu_offers_nca_fit_and_simulation() {
    let mut h = harness(with_oral());
    h.run_steps(2);
    h.get_by_label("New analysis").click();
    h.run_steps(3);
    h.get_by_label("Model fit").click();
    h.run_steps(3);
    assert_eq!(h.state().state.selection, Selection::NewFit);
    h.get_by_label("New analysis").click();
    h.run_steps(3);
    h.get_by_label("Model simulation").click();
    h.run_steps(3);
    assert_eq!(h.state().state.selection, Selection::NewSimulation);
    h.get_by_label("New analysis").click();
    h.run_steps(3);
    h.get_by_label("Non-compartmental analysis (NCA)").click();
    h.run_steps(3);
    assert!(matches!(h.state().state.selection, Selection::Analysis(_)));
    assert!(h.state().fit_page().is_none() && h.state().sim_page().is_none());
}

#[test]
fn the_fit_page_buttons_do_what_they_say() {
    let mut h = harness(with_fit());
    h.run_steps(3);
    h.get_by_label("Fit the model").click();
    h.run_steps(3);
    assert!(h.state().fit_page().unwrap().ok().is_some());
    assert!(matches!(h.state().state.selection, Selection::Analysis(_)));
    h.get_by_label("Generate from the data").click();
    h.run_steps(3);
    assert!(h.state().fit_page().unwrap().preview.curve.len() > 2);
    h.get_by_label("Semi-log").click();
    h.run_steps(3);
    assert!(h.state().state.log_axis);
}

#[test]
fn every_fit_and_simulation_screen_draws_in_both_themes_and_both_axes() {
    for dark in [false, true] {
        for log in [false, true] {
            let mut scenes: Vec<UiApp> = vec![with_fit(), fitted()];
            let mut sim = UiApp::new();
            sim.perform(vec![Action::NewSimulation]);
            scenes.push(sim);
            let mut poor = with_fit();
            if let Some(page) = poor.state.fit.as_mut() {
                page.input = crate::modelinfo::Input::ZeroOrder;
                page.lag = true;
            }
            poor.perform(vec![
                Action::FitChanged { regenerate: true },
                Action::RunFit,
            ]);
            scenes.push(poor);
            for mut app in scenes {
                app.state.mode = if dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                };
                app.state.log_axis = log;
                harness(app).run_steps(3);
            }
        }
    }
}

#[test]
fn the_pages_state_is_data() {
    let app = fitted();
    let text = serde_json::to_string(&app.state).unwrap();
    let back: UiState = serde_json::from_str(&text).unwrap();
    assert_eq!(back.fit.as_ref().map(|p| p.model().id), Some("pk1.oral_1"));
    assert_eq!(back.fit.unwrap().initial, app.fit_page().unwrap().initial);
}
