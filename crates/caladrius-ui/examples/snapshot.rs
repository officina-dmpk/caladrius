//! Renders each screen offscreen to a PNG (`target/snapshots/`), without a window (golden rule 8):
//! `cargo run -p caladrius-ui --example snapshot`. Each scene is set up through the application's
//! own actions and engine commands, then drawn by the real `UiApp::ui`.

use std::path::PathBuf;

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

fn render(name: &str, mut app: UiApp) -> Result<(), String> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(SIZE.0, SIZE.1))
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
