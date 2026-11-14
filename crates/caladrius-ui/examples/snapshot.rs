//! Renders each screen offscreen to a PNG (`target/snapshots/`), without a window (golden rule 8):
//! `cargo run -p caladrius-ui --example snapshot`. Each scene is set up through the application's
//! own actions and engine commands, then drawn by the real `UiApp::ui`.

use std::path::PathBuf;

use caladrius_ui::entry::EntryRow;
use caladrius_ui::modelinfo::{Compartments, Input, ParameterSet};
use caladrius_ui::{Action, Request, Selection, ThemeMode, UiApp};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

const SIZE: (f32, f32) = (1500.0, 1280.0);

/// One oral profile (100 mg, mg/L): a zero at time 0, a fit-friendly curve.
const ORAL: &str = "Time (h),Conc (mg/L),Dose (mg)\n0,0,100\n0.25,1.279,100\n0.5,2.195,100\n1,3.293,100\n2,3.971,100\n4,3.611,100\n6,2.989,100\n8,2.451,100\n12,1.643,100\n24,0.495,100\n";

/// Two subjects, semicolons and decimal commas (a French spreadsheet export).
const FRENCH: &str = "Sujet;Time (h);Conc (mg/L);Dose (mg)\n1;0;0;100\n1;0,25;1,279;100\n1;0,5;2,195;100\n1;1;3,293;100\n1;2;3,971;100\n1;4;3,611;100\n1;8;2,451;100\n1;24;0,495;100\n2;0;0;100\n2;0,5;1,8;100\n2;1;2,9;100\n2;2;3,5;100\n2;4;3,2;100\n2;8;2,2;100\n2;24;0,4;100\n";

/// The oral profile as cells copied from a French spreadsheet: a header line, tabs, decimal commas.
const PASTED: &str = "Temps (h)\tConc (mg/L)\tDose (mg)\r\n0\t0\t100\r\n0,25\t1,279\t\r\n0,5\t2,195\t\r\n1\t3,293\t\r\n2\t3,971\t\r\n4\t3,611\t\r\n6\t2,989\t\r\n8\t2,451\t\r\n12\t1,643\t\r\n24\t0,495\t\r\n";

/// Six invented oral profiles (no real subject): BLQ before the dose and at the end of some.
const SIX_SUBJECTS: &str = "Subject,Time (h),Conc (mg/L),Dose (mg)\nS1,0,BLQ,100\nS1,0.5,4.37,100\nS1,1,6.51,100\nS1,2,7.73,100\nS1,4,6.78,100\nS1,6,5.4,100\nS1,8,4.25,100\nS1,12,2.63,100\nS1,24,0.624,100\nS1,36,0.148,100\nS2,0,BLQ,100\nS2,0.5,2.68,100\nS2,1,4.36,100\nS2,2,5.95,100\nS2,4,6.17,100\nS2,6,5.39,100\nS2,8,4.55,100\nS2,12,3.19,100\nS2,24,1.08,100\nS2,36,0.368,100\nS3,0,BLQ,100\nS3,0.5,6.6,100\nS3,1,9.09,100\nS3,2,9.66,100\nS3,4,7.55,100\nS3,6,5.61,100\nS3,8,4.15,100\nS3,12,2.28,100\nS3,24,0.377,100\nS3,36,BLQ,100\nS4,0,BLQ,100\nS4,0.5,2.76,100\nS4,1,4.34,100\nS4,2,5.64,100\nS4,4,5.66,100\nS4,6,5.03,100\nS4,8,4.38,100\nS4,12,3.32,100\nS4,24,1.43,100\nS4,36,0.618,100\nS5,0,BLQ,100\nS5,0.5,6.63,100\nS5,1,8.44,100\nS5,2,8.05,100\nS5,4,5.54,100\nS5,6,3.72,100\nS5,8,2.49,100\nS5,12,1.12,100\nS5,24,0.102,100\nS5,36,BLQ,100\nS6,0,BLQ,100\nS6,0.5,2.3,100\nS6,1,3.88,100\nS6,2,5.65,100\nS6,4,6.32,100\nS6,6,5.69,100\nS6,8,4.81,100\nS6,12,3.28,100\nS6,24,0.99,100\nS6,36,0.298,100\n";

/// Invented data shaped like a course handout (T-108): times down the first column, one column per
/// subject, a Dose line, decimal commas and one value below the limit of quantification.
const WIDE: &str = "Temps (h)\t1\t2\t3\t4\r\nDose (mg/kg)\t30\t30\t45\t45\r\n0,5\t9,1\t12,4\t7,8\t10,2\r\n1\t8,4\t11,0\t7,5\t9,6\r\n2\t7,3\t9,2\t6,6\t8,1\r\n4\t5,6\t6,5\t5,9\t6,0\r\n8\t3,2\t3,4\t4,3\t3,5\r\n12\t1,9\t1,7\t3,1\t2,0\r\n24\t0,6\t< LQ\t1,6\t0,7\r\n";

/// The reported failure: a decimal comma read with a comma separator.
const AMBIGUOUS: &str = "time,conc\n0,25\n0,5\n1,2\n2,1\n";

fn out_dir() -> PathBuf {
    let dir = PathBuf::from("target").join("snapshots");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("cannot create {}: {e}", dir.display());
    }
    dir
}

/// The scenes whose page is longer than one screen are drawn on a taller canvas.
fn size_of(name: &str) -> (f32, f32) {
    // A narrow window: the lines of a page wrap instead of running off the edge.
    if name.contains("narrow") {
        return (800.0, 1600.0);
    }
    if name.starts_with("35-")
        || name.starts_with("36-")
        || name.contains("more-options")
        || name.contains("wide")
        || name.contains("per-subject")
    {
        (SIZE.0, 2000.0)
    } else {
        SIZE
    }
}

fn render(name: &str, mut app: UiApp) -> Result<(), String> {
    let size = size_of(name);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .with_pixels_per_point(1.25)
        .build_state(|ctx, app: &mut UiApp| app.ui(ctx), {
            // One frame first so the theme is applied before the scene is drawn.
            app.perform(Vec::new());
            app
        });
    harness.run_steps(6);
    // A scene named for "More options" shows the collapsed section open.
    if name.contains("more-options") {
        harness.get_by_label("More options").click_accesskit();
        harness.run_steps(6);
    }
    // A scene named for the unit menu shows the dose unit's list open.
    if name.contains("unit-menu") {
        if let Some(button) = harness.get_all_by_label("\u{23f7}").nth(2) {
            button.click_accesskit();
        }
        harness.run_steps(6);
    }
    let image = harness.render()?;
    let path = out_dir().join(format!("{name}.png"));
    image.save(&path).map_err(|e| e.to_string())?;
    println!("{}  {}x{}", path.display(), image.width(), image.height());
    Ok(())
}

fn with_oral() -> UiApp {
    let mut app = UiApp::new();
    app.load_csv("oral-dose.csv", ORAL.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

/// A simulated two-compartment profile (public parameters of the specification's worked example:
/// cl 2, vc 10, q 4, vp 8, dose 100) with a 1 % alternating error, imported as a worksheet. The
/// concentrations are the engine's `model.simulate`.
fn with_pk2_profile() -> UiApp {
    const TIMES: [f64; 14] = [
        0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0, 48.0,
    ];
    let mut app = UiApp::new();
    let sim = app.engine_mut().execute(
        "model.simulate",
        json!({ "model": "pk2.iv_bolus", "dose": 100.0,
                "params": { "cl": 2, "vc": 10, "q": 4, "vp": 8 }, "times": TIMES }),
    );
    let conc: Vec<f64> = sim
        .ok()
        .and_then(|v| v["conc"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(serde_json::Value::as_f64)
        .collect();
    let mut csv = String::from(
        "Time (h),Conc (mg/L),Dose (mg)
",
    );
    for (i, (t, c)) in TIMES.iter().zip(conc).enumerate() {
        let noise = if i % 2 == 0 { 1.01 } else { 0.99 };
        csv.push_str(&format!(
            "{t},{},100
",
            c * noise
        ));
    }
    app.load_csv("two-compartment-bolus.csv", csv.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

/// The fit page with the two-compartment intravenous bolus model picked; `start` are the values
/// the person set (placeholders are shown muted when it is `None`).
fn pk2_fit(start: Option<[(&str, f64); 4]>) -> UiApp {
    let mut app = with_pk2_profile();
    app.perform(vec![Action::NewFitWith(Compartments::Two)]);
    if let Some(page) = app.state.fit.as_mut() {
        page.input = Input::Bolus;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    if let Some(values) = start {
        if let Some(page) = app.state.fit.as_mut() {
            page.initial = values.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
            page.accept_placeholders();
        }
        app.perform(vec![Action::FitChanged { regenerate: false }]);
    }
    app
}

/// A simulated three-compartment profile (the public clearance set of the oracle's base case: cl 3,
/// vc 8, q2 9.5, vp2 9.5, q3 6.3, vp3 31.5, dose 100) with a 1 % alternating error, imported as a
/// worksheet. The concentrations are the engine's `model.simulate`.
fn with_pk3_profile() -> UiApp {
    const TIMES: [f64; 20] = [
        0.05, 0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0,
        48.0, 72.0, 96.0, 120.0,
    ];
    let mut app = UiApp::new();
    let sim = app.engine_mut().execute(
        "model.simulate",
        json!({ "model": "pk3.iv_bolus", "dose": 100.0,
                "params": { "cl": 3, "vc": 8, "q2": 9.5, "vp2": 9.5, "q3": 6.3, "vp3": 31.5 },
                "times": TIMES }),
    );
    let conc: Vec<f64> = sim
        .ok()
        .and_then(|v| v["conc"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(serde_json::Value::as_f64)
        .collect();
    let mut csv = String::from(
        "Time (h),Conc (mg/L),Dose (mg)
",
    );
    for (i, (t, c)) in TIMES.iter().zip(conc).enumerate() {
        let noise = if i % 2 == 0 { 1.01 } else { 0.99 };
        csv.push_str(&format!(
            "{t},{},100
",
            c * noise
        ));
    }
    app.load_csv("three-compartment-bolus.csv", csv.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

/// The fit page with the three-compartment intravenous bolus model picked; `start` are the values
/// the person set (placeholders are shown muted when it is `None`).
fn pk3_fit(start: Option<[(&str, f64); 6]>) -> UiApp {
    let mut app = with_pk3_profile();
    app.perform(vec![Action::NewFitWith(Compartments::Three)]);
    if let Some(page) = app.state.fit.as_mut() {
        page.input = Input::Bolus;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    if let Some(values) = start {
        if let Some(page) = app.state.fit.as_mut() {
            page.initial = values.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
            page.accept_placeholders();
        }
        app.perform(vec![Action::FitChanged { regenerate: false }]);
    }
    app
}

/// The saved file of a short session: an import, an analysis, a fit, a simulation, an edit of the
/// data and the analysis run again, as the application writes it.
fn saved_project() -> serde_json::Value {
    let mut app = nca(with_oral());
    app.perform(vec![
        Action::NewFit,
        Action::RunFit,
        Action::NewSimulation,
        Action::SimSave,
        Action::SetCell {
            worksheet: 1,
            row: 3,
            column: "Conc".to_owned(),
            text: "3,4".to_owned(),
        },
        Action::RunAgain(2),
        Action::SaveProject,
    ]);
    let bytes = app.take_requests().into_iter().find_map(|r| match r {
        Request::SaveProject { bytes, .. } => Some(bytes),
        _ => None,
    });
    bytes
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// An application that has just verified `document` as a file called `name`.
fn verifying(name: &str, document: &serde_json::Value, mode: ThemeMode) -> UiApp {
    let mut app = UiApp::new();
    app.verify_project(name, document.to_string().as_bytes());
    app.state.mode = mode;
    app
}

fn nca(mut app: UiApp) -> UiApp {
    app.perform(vec![Action::NewAnalysis]);
    app
}

fn scenes() -> Vec<(&'static str, UiApp)> {
    let mut scenes: Vec<(&'static str, UiApp)> = Vec::new();

    scenes.push(("01-welcome-light", UiApp::new()));

    let mut app = UiApp::new();
    app.load_csv("etude-francaise.csv", FRENCH.as_bytes());
    scenes.push(("02-import-semicolon-decimal-comma", app));

    let mut app = UiApp::new();
    app.load_csv("quarter.csv", AMBIGUOUS.as_bytes());
    scenes.push(("03-import-reading-with-a-check", app));

    scenes.push(("04-worksheet", with_oral()));

    // The raw data before any analysis (T-109): six subjects with BLQ points, both themes.
    for (name, mode) in [
        (
            "04a-worksheet-raw-plot-six-subjects-light",
            ThemeMode::Light,
        ),
        ("04b-worksheet-raw-plot-six-subjects-dark", ThemeMode::Dark),
    ] {
        let mut app = UiApp::new();
        app.load_csv("six-subjects.csv", SIX_SUBJECTS.as_bytes());
        app.perform(vec![Action::ImportConfirm]);
        app.state.mode = mode;
        scenes.push((name, app));
    }

    scenes.push(("05-nca-linear", nca(with_oral())));

    let mut app = nca(with_oral());
    app.state.log_axis = true;
    scenes.push(("06-nca-semilog-hidden-zero", app));

    // Terminal phase chosen by hand: the points from 6 h on.
    let mut app = nca(with_oral());
    app.state.log_axis = true;
    if let Some(page) = app.nca_page().cloned() {
        let mut page = page;
        page.options["lambda_z_selection"]["manual"] = json!({ "times": [6.0, 8.0, 12.0, 24.0] });
        app.state.nca = Some(page);
    }
    app.perform(vec![Action::RunNca]);
    scenes.push(("07-nca-manual-terminal-phase", app));

    // An edit of the data after the run: the result is out of date.
    let mut app = nca(with_oral());
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 4,
        column: "Conc".to_owned(),
        text: "4,2".to_owned(),
    }]);
    app.perform(vec![Action::Select(Selection::Analysis(2))]);
    scenes.push(("08-nca-stale-after-edit", app));

    let mut app = nca(with_oral());
    app.state.mode = ThemeMode::Dark;
    app.state.log_axis = true;
    scenes.push(("09-nca-dark-semilog", app));

    // An error the engine explains: a negative concentration.
    let mut app = UiApp::new();
    app.load_csv(
        "negative.csv",
        b"time,conc,dose\n0,0,10\n1,5,10\n2,-0.5,10\n4,1,10\n",
    );
    app.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
    scenes.push(("10-nca-error-negative-concentration", app));

    // Two subjects and the worksheet in the same page, with a flag (a short terminal phase).
    let mut app = UiApp::new();
    app.load_csv("etude-francaise.csv", FRENCH.as_bytes());
    app.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
    scenes.push(("11-nca-two-subjects-flags", app));

    // A short profile: the engine raises flags and the page says what to check.
    let mut app = UiApp::new();
    app.load_csv(
        "short-profile.csv",
        b"time,conc,dose
0,0,10
1,8,10
2,6,10
3,4.5,10
4,3.4,10
",
    );
    app.perform(vec![Action::ImportConfirm, Action::NewAnalysis]);
    scenes.push(("12-nca-flags-short-terminal-phase", app));

    // ---- model fit and simulation (T-026) ----
    let fit_app = || {
        let mut app = with_oral();
        app.perform(vec![Action::NewFit]);
        app
    };
    // The page before the first fit: starting values from the data, the live curve and the objective.
    scenes.push(("13-fit-setup-starting-values", fit_app()));

    // A starting value edited by hand: the curve and the objective follow.
    let mut app = fit_app();
    if let Some(page) = app.state.fit.as_mut() {
        if let Some(v) = page.initial.get_mut("ka") {
            *v *= 3.0;
        }
        if let Some(v) = page.initial.get_mut("v") {
            *v *= 1.4;
        }
    }
    app.perform(vec![Action::FitChanged { regenerate: false }]);
    scenes.push(("14-fit-setup-edited-starting-values", app));

    // After the fit: summary, fit plot and residuals.
    let mut app = fit_app();
    app.perform(vec![Action::RunFit]);
    scenes.push(("15-fit-results-linear", app));

    let mut app = fit_app();
    app.perform(vec![Action::RunFit]);
    app.state.log_axis = true;
    scenes.push(("16-fit-results-semilog", app));

    let mut app = fit_app();
    app.perform(vec![Action::RunFit]);
    app.state.mode = ThemeMode::Dark;
    scenes.push(("17-fit-results-dark", app));

    let mut app = fit_app();
    app.state.mode = ThemeMode::Dark;
    scenes.push(("18-fit-setup-dark", app));

    // A model with a lag and a fixed-duration input are picked by route; the diagram follows.
    let mut app = fit_app();
    if let Some(page) = app.state.fit.as_mut() {
        page.lag = true;
    }
    app.perform(vec![Action::FitChanged { regenerate: true }]);
    scenes.push(("19-fit-setup-oral-with-lag", app));

    // A poor fit: a model that cannot describe the data. Flags say what to check.
    let mut app = with_oral();
    app.perform(vec![Action::NewFit]);
    if let Some(page) = app.state.fit.as_mut() {
        page.input = caladrius_ui::modelinfo::Input::Bolus;
    }
    app.perform(vec![
        Action::FitChanged { regenerate: true },
        Action::RunFit,
    ]);
    scenes.push(("20-fit-poor-fit-flags", app));

    // The simulation page.
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulation]);
    scenes.push(("21-simulation-oral", app));

    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulation]);
    app.state.mode = ThemeMode::Dark;
    app.state.log_axis = true;
    scenes.push(("22-simulation-dark-semilog", app));

    // A simulation opened from a selected worksheet takes its units.
    let mut app = with_oral();
    app.perform(vec![
        Action::Select(Selection::Worksheet(1)),
        Action::NewSimulation,
    ]);
    scenes.push(("23-simulation-with-worksheet-units", app));

    // The project saved to a file, then the data edited: the tree names the file, marks the
    // project modified and shows both analyses out of date.
    let mut app = nca(with_oral());
    app.perform(vec![Action::NewFit, Action::RunFit]);
    app.perform(vec![Action::SaveProject]);
    let _ = app.take_requests();
    app.project_saved("pilot-study.caladrius.json");
    app.perform(vec![Action::SetCell {
        worksheet: 1,
        row: 4,
        column: "Conc".to_owned(),
        text: "4,2".to_owned(),
    }]);
    app.perform(vec![Action::Select(Selection::Worksheet(1))]);
    scenes.push(("24-tree-stale-and-modified", app));

    // Unsaved changes: the question with its three answers.
    let mut app = nca(with_oral());
    app.perform(vec![Action::OpenProject]);
    scenes.push(("25-unsaved-changes-question", app));

    let mut app = nca(with_oral());
    app.perform(vec![Action::NewProject]);
    app.state.mode = ThemeMode::Dark;
    scenes.push(("26-unsaved-changes-question-dark", app));

    // The command palette: the whole list, a query, and in the dark theme.
    let mut app = nca(with_oral());
    app.perform(vec![Action::OpenPalette]);
    scenes.push(("27-palette-all-entries", app));

    let mut app = nca(with_oral());
    app.perform(vec![Action::OpenPalette]);
    app.state.palette.query = "fit".to_owned();
    scenes.push(("28-palette-query-fit", app));

    let mut app = with_oral();
    app.perform(vec![Action::OpenPalette]);
    app.state.palette.query = "nca".to_owned();
    app.state.mode = ThemeMode::Dark;
    scenes.push(("29-palette-query-nca-dark", app));

    // The settings page: the defaults with the system locale shown as a suggestion; a changed
    // page in the dark theme where the locale's suggestion is in use; a search.
    let mut app = UiApp::new();
    app.set_system_locale(Some("fr-FR"));
    app.perform(vec![Action::OpenSettings]);
    scenes.push(("30-settings-defaults-locale-suggested", app));

    let mut app = nca(with_oral());
    app.set_system_locale(Some("fr-FR"));
    app.apply_locale_suggestion();
    app.perform(vec![
        Action::SetSetting {
            key: "fit.max_iterations".to_owned(),
            value: json!(80),
        },
        Action::SetSetting {
            key: "nca.auc_method".to_owned(),
            value: json!("linear"),
        },
        Action::SetSetting {
            key: "theme".to_owned(),
            value: json!("dark"),
        },
        Action::OpenSettings,
    ]);
    scenes.push(("31-settings-changed-comma-dark", app));

    let mut app = UiApp::new();
    app.perform(vec![Action::OpenSettings]);
    app.state.settings_query = "fit".to_owned();
    scenes.push(("32-settings-search-fit", app));

    // With a decimal comma and six digits the results follow.
    let mut app = nca(with_oral());
    app.perform(vec![
        Action::SetSetting {
            key: "decimal_mark".to_owned(),
            value: json!("comma"),
        },
        Action::SetSetting {
            key: "significant_digits".to_owned(),
            value: json!(6),
        },
    ]);
    scenes.push(("33-nca-decimal-comma-six-digits", app));

    // ---- two compartments (T-034b) ----
    let start = [("cl", 1.6), ("vc", 11.0), ("q", 3.2), ("vp", 9.0)];
    // The page before any value is set: the sentence, muted placeholders, the live curve.
    scenes.push(("34-fit-two-compartments-setup", pk2_fit(None)));

    let mut app = pk2_fit(Some(start));
    app.perform(vec![Action::RunFit]);
    scenes.push(("35-fit-two-compartments-results", app));

    let mut app = pk2_fit(Some(start));
    app.perform(vec![Action::RunFit]);
    app.state.mode = ThemeMode::Dark;
    app.state.log_axis = true;
    scenes.push(("36-fit-two-compartments-results-dark-semilog", app));

    let mut app = pk2_fit(None);
    app.state.mode = ThemeMode::Dark;
    scenes.push(("37-fit-two-compartments-setup-dark", app));

    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(Compartments::Two)]);
    scenes.push(("38-simulation-two-compartments-oral", app));

    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(Compartments::Two)]);
    if let Some(page) = app.state.sim.as_mut() {
        page.input = Input::Bolus;
        page.set = ParameterSet::Macro;
    }
    app.perform(vec![Action::SimChanged]);
    app.state.mode = ThemeMode::Dark;
    app.state.log_axis = true;
    scenes.push(("39-simulation-two-compartments-macro-dark", app));

    // ---- the preset of the fit options (T-044) ----
    scenes.push(("40-fit-more-options-preset", fit_app()));

    let mut app = fit_app();
    if let Some(page) = app.state.fit.as_mut() {
        caladrius_ui::fitpreset::apply(&mut page.options, "reference_conventions");
    }
    app.state.mode = ThemeMode::Dark;
    scenes.push(("41-fit-more-options-reference-dark", app));

    let mut app = fit_app();
    if let Some(page) = app.state.fit.as_mut() {
        caladrius_ui::fitpreset::apply(&mut page.options, "reference_conventions");
        page.set_option(&["max_iterations"], json!(80));
    }
    scenes.push(("42-fit-more-options-custom", app));

    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    scenes.push(("43-entry-empty", app));

    // Cells copied from a French spreadsheet (header line, tabs, decimal commas) and pasted.
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.name = "Oral 100 mg".to_owned();
        let note = entry.paste(None, PASTED);
        entry.note = Some(note);
    }
    scenes.push(("44-entry-pasted-from-a-spreadsheet", app));

    // Time 1 typed twice: the same check as a file, and the same "anyway" box.
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.rows = [
            ("0", "0", "100"),
            ("1", "3.2", ""),
            ("1", "3.0", ""),
            ("4", "2", ""),
            ("8", "1", ""),
        ]
        .iter()
        .map(|(t, c, d)| EntryRow {
            time: (*t).to_owned(),
            concentration: (*c).to_owned(),
            dose: (*d).to_owned(),
            ..EntryRow::default()
        })
        .collect();
    }
    app.perform(vec![Action::EntryCreate]);
    scenes.push(("45-entry-with-a-check", app));

    // Two subjects, dark theme.
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry, Action::ToggleTheme]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.subjects = true;
        entry.dose_unit = "mg/kg".to_owned();
        entry.rows = [
            ("A", "0", "0", "2"),
            ("A", "1", "3,2", ""),
            ("A", "4", "2,1", ""),
            ("B", "0", "0", "4"),
            ("B", "1", "6,0", ""),
            ("B", "4", "3,9", ""),
        ]
        .iter()
        .map(|(s, t, c, d)| EntryRow {
            subject: (*s).to_owned(),
            time: (*t).to_owned(),
            concentration: (*c).to_owned(),
            dose: (*d).to_owned(),
        })
        .collect();
    }
    scenes.push(("46-entry-two-subjects-dark", app));

    // The pasted table made a worksheet: the same screen as an imported file.
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.name = "Oral 100 mg".to_owned();
        entry.paste(None, PASTED);
    }
    app.perform(vec![Action::EntryCreate]);
    scenes.push(("47-entry-made-a-worksheet", app));

    // The welcome page in the dark theme: the logo on a dark background.
    let mut app = UiApp::new();
    app.state.mode = ThemeMode::Dark;
    scenes.push(("48-welcome-dark", app));

    // A wide table pasted with "Several subjects" on: one row per sample (T-108).
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.subjects = true;
        let note = entry.paste(None, WIDE);
        entry.note = Some(note);
    }
    scenes.push(("49-entry-pasted-wide-table", app));

    // "Several subjects" ticked: five subjects in columns, the times typed once (T-108).
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.set_subjects(true);
        entry.grid.set_subject_count(5);
        entry.grid_focus = Some((0, 0));
        entry.paste(None, "0,5\n1\n2\n4\n8");
        entry.grid_focus = Some((0, 1));
        entry.paste(None, "9,1\t12,4\n8,4\t11,0");
    }
    scenes.push(("51-entry-subjects-in-columns", app));

    // A dose unit the engine does not recognise, and the list of the recognised ones.
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenEntry]);
    if let Some(entry) = app.state.entry.as_mut() {
        entry.dose_unit = "mg per kg".to_owned();
    }
    scenes.push(("50-entry-unit-menu-and-check", app));
    // Plots as objects of the project (T-110): six invented subjects overlaid, both themes.
    for (name, mode) in [
        ("52-plot-six-subjects-light", ThemeMode::Light),
        ("53-plot-six-subjects-dark", ThemeMode::Dark),
    ] {
        let mut app = UiApp::new();
        app.load_csv("six-subjects.csv", SIX_SUBJECTS.as_bytes());
        app.perform(vec![Action::ImportConfirm, Action::NewPlot]);
        app.state.mode = mode;
        scenes.push((name, app));
    }
    // Four subjects ticked, one panel each, semi-log only.
    for (name, mode) in [
        ("54-plot-per-subject-semilog-light", ThemeMode::Light),
        ("55-plot-per-subject-semilog-dark", ThemeMode::Dark),
    ] {
        let mut app = UiApp::new();
        app.load_csv("six-subjects.csv", SIX_SUBJECTS.as_bytes());
        app.perform(vec![Action::ImportConfirm, Action::NewPlot]);
        if let Some(page) = app.state.plot.as_mut() {
            page.all_subjects = false;
            page.subjects = ["S1", "S3", "S4", "S5"].map(str::to_owned).to_vec();
            page.layout = "per_subject".to_owned();
            page.scale = "log".to_owned();
        }
        app.perform(vec![Action::PlotChanged]);
        app.state.mode = mode;
        scenes.push((name, app));
    }
    // The diagnostic plots of a fit, from its stored result.
    for (name, view, mode) in [
        (
            "56-plot-fit-observed-predicted-light",
            "observed_predicted",
            ThemeMode::Light,
        ),
        (
            "57-plot-fit-residuals-dark",
            "weighted_residuals_time",
            ThemeMode::Dark,
        ),
        ("58-plot-fit-profile-light", "profile", ThemeMode::Light),
    ] {
        let mut app = with_oral();
        let fit = app
            .engine_mut()
            .execute("fit.run", json!({ "worksheet": 1, "model": "pk1.oral_1" }));
        if let Some(fit) = fit.ok().and_then(|v| v.get("id").and_then(|i| i.as_u64())) {
            app.perform(vec![Action::NewFitPlot {
                fit,
                view: view.to_owned(),
            }]);
        }
        app.state.mode = mode;
        scenes.push((name, app));
    }

    // Verify a project (T-112): a file whose steps all reproduce their seals; one whose history
    // holds a changed number (the first difference, with both seals); one saved before seals.
    let document = saved_project();
    scenes.push((
        "59-verify-project-verified-light",
        verifying("study.caladrius.json", &document, ThemeMode::Light),
    ));
    let mut edited = document.clone();
    if let Some(entry) = edited["history"]
        .as_array_mut()
        .and_then(|h| h.iter_mut().find(|e| e["command"] == "data.import"))
    {
        let csv = entry["params"]["csv"].as_str().unwrap_or_default();
        entry["params"]["csv"] = json!(csv.replacen("1,3.293,100", "1,3.294,100", 1));
    }
    scenes.push((
        "60-verify-project-first-difference-dark",
        verifying("edited.caladrius.json", &edited, ThemeMode::Dark),
    ));
    let mut old = document.clone();
    if let Some(history) = old["history"].as_array_mut() {
        for entry in history {
            if let Some(map) = entry.as_object_mut() {
                map.remove("seal");
                map.remove("at");
            }
        }
    }
    scenes.push((
        "61-verify-project-saved-before-seals-light",
        verifying("2026-spring.caladrius.json", &old, ThemeMode::Light),
    ));
    scenes.push((
        "63-verify-project-first-difference-narrow-light",
        verifying("edited.caladrius.json", &edited, ThemeMode::Light),
    ));
    let mut changed_result = document.clone();
    changed_result["worksheets"][0]["columns"][2]["data"]["number"][2] = json!(7.5);
    scenes.push((
        "62-verify-project-stored-project-differs-light",
        verifying(
            "changed-result.caladrius.json",
            &changed_result,
            ThemeMode::Light,
        ),
    ));

    // ---- three compartments ----
    let start3 = [
        ("cl", 3.2),
        ("vc", 8.5),
        ("q2", 9.0),
        ("vp2", 10.0),
        ("q3", 6.0),
        ("vp3", 30.0),
    ];
    // The model picker on three compartments: the diagram, the equation and the placeholders.
    scenes.push(("64-fit-three-compartments-setup", pk3_fit(None)));
    let mut app = pk3_fit(Some(start3));
    app.perform(vec![Action::RunFit]);
    scenes.push(("65-fit-three-compartments-results", app));
    let mut app = pk3_fit(Some(start3));
    app.perform(vec![Action::RunFit]);
    app.state.mode = ThemeMode::Dark;
    app.state.log_axis = true;
    scenes.push(("66-fit-three-compartments-results-dark-semilog", app));
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(Compartments::Three)]);
    if let Some(page) = app.state.sim.as_mut() {
        page.set = ParameterSet::Micro;
    }
    app.perform(vec![Action::SimChanged]);
    scenes.push(("67-simulation-three-compartments-micro", app));

    scenes
}

fn main() {
    // An argument renders only the scenes whose name contains it (`snapshot -- plot`).
    let only: Option<String> = std::env::args().nth(1);
    let mut failed = false;
    for (name, app) in scenes() {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        if let Err(e) = render(name, app) {
            eprintln!("{name}: {e}");
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
