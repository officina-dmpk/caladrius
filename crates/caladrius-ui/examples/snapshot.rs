//! Renders each screen offscreen to a PNG (`target/snapshots/`), without a window (golden rule 8):
//! `cargo run -p caladrius-ui --example snapshot`. Each scene is set up through the application's
//! own actions and engine commands, then drawn by the real `UiApp::ui`.

use std::path::PathBuf;

use caladrius_ui::modelinfo::{Compartments, Input, ParameterSet};
use caladrius_ui::{Action, Selection, ThemeMode, UiApp};
use egui_kittest::Harness;
use serde_json::json;

const SIZE: (f32, f32) = (1500.0, 1280.0);

/// One oral profile (100 mg, mg/L): a zero at time 0, a fit-friendly curve.
const ORAL: &str = "Time (h),Conc (mg/L),Dose (mg)\n0,0,100\n0.25,1.279,100\n0.5,2.195,100\n1,3.293,100\n2,3.971,100\n4,3.611,100\n6,2.989,100\n8,2.451,100\n12,1.643,100\n24,0.495,100\n";

/// Two subjects, semicolons and decimal commas (a French spreadsheet export).
const FRENCH: &str = "Sujet;Time (h);Conc (mg/L);Dose (mg)\n1;0;0;100\n1;0,25;1,279;100\n1;0,5;2,195;100\n1;1;3,293;100\n1;2;3,971;100\n1;4;3,611;100\n1;8;2,451;100\n1;24;0,495;100\n2;0;0;100\n2;0,5;1,8;100\n2;1;2,9;100\n2;2;3,5;100\n2;4;3,2;100\n2;8;2,2;100\n2;24;0,4;100\n";

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
    if name.starts_with("35-") || name.starts_with("36-") {
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

    scenes
}

fn main() {
    let mut failed = false;
    for (name, app) in scenes() {
        if let Err(e) = render(name, app) {
            eprintln!("{name}: {e}");
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
