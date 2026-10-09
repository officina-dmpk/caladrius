//! Tests of the settings: the page, the number display, persistence as data, the system locale as
//! a displayed suggestion, and the defaults new analyses start from.

use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use crate::app::{Action, Request, Selection, UiApp};
use crate::settings::{DecimalMark, FitDefaults, Settings, ThemeChoice, nca_defaults};
use crate::tests::{harness, with_analysis, with_oral};
use crate::theme::ThemeMode;

fn set(app: &mut UiApp, key: &str, value: Value) {
    app.perform(vec![Action::SetSetting {
        key: key.to_owned(),
        value,
    }]);
}

/// The settings texts the app asked the program to store.
fn stored(app: &mut UiApp) -> Vec<String> {
    app.take_requests()
        .into_iter()
        .filter_map(|r| match r {
            Request::SaveSettings(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn the_page_lists_every_setting_with_what_it_affects_and_its_default() {
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenSettings]);
    assert_eq!(app.state.selection, Selection::Settings);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Settings");
    for field in &crate::settings::FIELDS {
        h.get_by_label(field.title);
        h.get_by_label(field.affects);
    }
    // Each default shows its value.
    h.get_by_label("Default: 4");
    h.get_by_label("Default: Point (3.25)");
    h.get_by_label("Default: Linear up, log down");
    h.get_by_label("Default: 50");
    h.get_by_label("Default: 0.0000000001");
    // Nothing is changed, so nothing offers a reset.
    assert!(h.query_by_label("Reset").is_none());
    // The system locale row says when the system reported nothing.
    h.get_by_label("Not reported by the system: nothing is suggested.");
}

#[test]
fn the_search_box_narrows_the_page_and_a_changed_setting_offers_its_default() {
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenSettings]);
    app.state.settings_query = "decimal".to_owned();
    set(&mut app, "decimal_mark", json!("comma"));
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Decimal mark");
    assert!(h.query_by_label("Maximum iterations").is_none());
    // The changed setting offers a way back, and the page says what the default is.
    h.get_by_label("Default: Point (3.25)");
    h.get_by_label("Reset").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.decimal_mark, DecimalMark::Point);
    assert!(h.query_by_label("Reset").is_none());
    h.state_mut().state.settings_query = "zzz".to_owned();
    h.run_steps(3);
    h.get_by_label("No setting matches.");
}

#[test]
fn the_number_display_follows_the_digits_and_the_decimal_mark_without_touching_data() {
    let mut app = with_analysis();
    let project = app.engine().project().clone();
    assert_eq!(crate::fmt::number(12.3456789), "12.35");
    set(&mut app, "significant_digits", json!(6));
    assert_eq!(crate::fmt::number(12.3456789), "12.3457");
    assert_eq!(crate::fmt::number(1234.56789), "1234.57");
    set(&mut app, "decimal_mark", json!("comma"));
    assert_eq!(crate::fmt::number(12.3456789), "12,3457");
    assert_eq!(crate::fmt::exact(2.5), "2,5");
    // What was typed with either mark is read the same.
    assert_eq!(crate::fmt::parse_number("3,25"), Some(3.25));
    assert_eq!(crate::fmt::parse_number("3.25"), Some(3.25));
    assert_eq!(crate::fmt::parse_number("-1,5e-3"), Some(-0.0015));
    assert_eq!(crate::fmt::parse_number("abc"), None);
    // The results on screen use it (the Cmax of the oral profile is 3.971).
    app.perform(vec![Action::Select(Selection::Analysis(2))]);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("3,971");
    assert!(h.query_by_label("3.971").is_none());
    // Nothing stored changed, and a point-decimal file is still read as it always was.
    assert_eq!(h.state().engine().project(), &project);
    h.state_mut()
        .load_csv("point.csv", b"time,conc,dose\n0.5,1.5,10\n1,2.5,10\n");
    let pending = h.state().pending_import().unwrap();
    assert!(pending.can_import());
    // Back to the defaults.
    set(h.state_mut(), "significant_digits", json!(4));
    set(h.state_mut(), "decimal_mark", json!("point"));
    assert_eq!(crate::fmt::number(12.3456789), "12.35");
}

#[test]
fn a_value_a_setting_does_not_accept_is_refused_with_a_sentence_and_nothing_changes() {
    let mut app = UiApp::new();
    let _ = stored(&mut app);
    set(&mut app, "significant_digits", json!(1));
    let said = app.notice_text().unwrap();
    assert!(
        said.starts_with("Significant digits: give a whole number from 2 to 10"),
        "{said}"
    );
    set(&mut app, "fit.weighting", json!("wild"));
    assert!(app.notice_text().unwrap().contains("not one of uniform"));
    assert_eq!(app.settings, Settings::default());
    assert!(stored(&mut app).is_empty());
}

#[test]
fn settings_are_stored_as_data_and_come_back() {
    let mut app = UiApp::new();
    assert!(
        stored(&mut app).is_empty(),
        "nothing to store before a change"
    );
    set(&mut app, "nca.lambda_z_min_points", json!(4));
    set(&mut app, "theme", json!("dark"));
    let texts = stored(&mut app);
    assert_eq!(texts.len(), 2);
    let last = texts.last().unwrap();
    assert_eq!(Settings::from_json(last).unwrap(), app.settings);
    assert_eq!(app.settings_json(), *last);
    // The same value again stores nothing more.
    set(&mut app, "nca.lambda_z_min_points", json!(4));
    assert!(stored(&mut app).is_empty());
    // A new session reads them back; the theme is applied at once.
    let mut next = UiApp::new();
    assert!(next.set_settings_json(last));
    assert_eq!(next.settings, app.settings);
    assert_eq!(next.state.mode, ThemeMode::Dark);
    assert!(
        stored(&mut next).is_empty(),
        "what was just read is already stored"
    );
    // Text that cannot be read leaves the defaults and says so.
    let mut broken = UiApp::new();
    assert!(!broken.set_settings_json("{ not json"));
    assert_eq!(broken.settings, Settings::default());
    assert!(
        broken
            .notice_text()
            .unwrap()
            .ends_with("the default settings are used.")
    );
    assert!(!broken.set_settings_json(r#"{ "significant_digits": 99 }"#));
}

#[test]
fn the_system_locale_is_shown_as_a_suggestion_and_never_applied_unseen() {
    let mut app = UiApp::new();
    app.set_system_locale(Some("fr-FR"));
    // Reported, but nothing changed: the number display is still the default.
    assert_eq!(app.settings.decimal_mark, DecimalMark::Point);
    assert_eq!(crate::fmt::number(2.5), "2.5");
    app.perform(vec![Action::OpenSettings]);
    // The search narrows the page to the locale row, so it fits on the screen.
    app.state.settings_query = "locale".to_owned();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("fr-FR");
    h.get_by_label("Reported by the system; it suggests a comma as the decimal mark.");
    // It is offered, and taken only when the person asks.
    h.get_by_label("Use the suggestion").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.decimal_mark, DecimalMark::Comma);
    assert!(h.state().settings.locale.mark_from_system);
    // The page says where the mark in use comes from.
    h.get_by_label("The decimal mark in use is this suggestion; change it above if it is wrong.");
    assert_eq!(crate::fmt::number(2.5), "2,5");
    // Choosing a mark replaces the suggestion.
    set(h.state_mut(), "decimal_mark", json!("point"));
    h.run_steps(3);
    assert!(!h.state().settings.locale.mark_from_system);
    assert!(
        h.query_by_label_contains("The decimal mark in use is this")
            .is_none()
    );
    h.get_by_label("fr-FR");

    // On a first start the program lets the locale suggest, and the page still shows it.
    let mut first = UiApp::new();
    first.set_system_locale(Some("de_DE.UTF-8"));
    first.apply_locale_suggestion();
    assert_eq!(first.settings.decimal_mark, DecimalMark::Comma);
    assert!(first.settings.locale.mark_from_system);
    assert!(stored(&mut first).len() == 1);
    first.perform(vec![Action::OpenSettings]);
    first.state.settings_query = "locale".to_owned();
    let mut h = harness(first);
    h.run_steps(3);
    h.get_by_label("de_DE.UTF-8");
    h.get_by_label("The decimal mark in use is this suggestion; change it above if it is wrong.");
    // A locale that names no language suggests nothing.
    let mut none = UiApp::new();
    none.set_system_locale(Some("C"));
    none.apply_locale_suggestion();
    assert_eq!(none.settings.decimal_mark, DecimalMark::Point);
    none.perform(vec![Action::UseSystemSuggestion]);
    assert!(none.notice_text().unwrap().contains("no locale"));
    // Settings read from a file keep the locale the system reports now, not a stored one.
    let mut next = UiApp::new();
    next.set_system_locale(Some("en-US"));
    let mut old = Settings::default();
    old.locale.system = Some("fr-FR".to_owned());
    old.locale.mark_from_system = true;
    assert!(next.set_settings_json(&old.to_json()));
    assert_eq!(next.settings.locale.system.as_deref(), Some("en-US"));
}

#[test]
fn the_defaults_of_the_settings_are_the_engines_and_new_analyses_use_the_ones_that_differ() {
    // With the defaults, a new NCA is run with the engine's options, which are the ones listed.
    let mut app = with_oral();
    app.perform(vec![Action::NewAnalysis]);
    let options = app.nca_page().unwrap().options.clone();
    assert_eq!(options["auc_method"], nca_defaults::AUC_METHOD);
    assert_eq!(options["lambda_z"]["min_points"], nca_defaults::MIN_POINTS);
    assert_eq!(options["lambda_z"]["allow_tmax"], nca_defaults::ALLOW_TMAX);
    // A changed default reaches the next NCA.
    let mut app = with_oral();
    set(&mut app, "nca.lambda_z_min_points", json!(4));
    set(&mut app, "nca.auc_method", json!("linear"));
    app.perform(vec![Action::NewAnalysis]);
    let options = app.nca_page().unwrap().options.clone();
    assert_eq!(options["auc_method"], "linear");
    assert_eq!(options["lambda_z"]["min_points"], 4);
    // A changed default of the fit reaches the next fit, and the page shows it.
    let mut app = with_oral();
    set(&mut app, "fit.weighting", json!("inv_y2"));
    set(&mut app, "fit.max_iterations", json!(80));
    set(&mut app, "fit.convergence", json!(0.001));
    app.perform(vec![
        Action::Select(Selection::Worksheet(1)),
        Action::NewFit,
    ]);
    let page = app.fit_page().unwrap();
    assert_eq!(page.weighting, "inv_y2");
    assert_eq!(page.options["max_iterations"], 80);
    assert_eq!(page.options["convergence"], 0.001);
    assert_eq!(FitDefaults::default().max_iterations, 50);
    // Settings do not rewrite an analysis that already exists.
    let mut app = with_analysis();
    let before = app.nca_page().unwrap().options.clone();
    set(&mut app, "nca.lambda_z_min_points", json!(5));
    assert_eq!(app.nca_page().unwrap().options, before);
}

#[test]
fn the_theme_is_a_setting_the_button_and_the_palette_agree_with() {
    let mut app = UiApp::new();
    let _ = stored(&mut app);
    assert_eq!(app.state.mode, ThemeMode::Light);
    set(&mut app, "theme", json!("dark"));
    assert_eq!(app.state.mode, ThemeMode::Dark);
    // The button switches it and the choice is stored like any other.
    app.perform(vec![Action::ToggleTheme]);
    assert_eq!(
        (app.state.mode, app.settings.theme),
        (ThemeMode::Light, ThemeChoice::Light)
    );
    assert!(!stored(&mut app).is_empty());
    // Following the system: the window's own theme decides, and a change of it is followed.
    set(&mut app, "theme", json!("system"));
    app.follow_settings(Some(ThemeMode::Dark));
    assert_eq!(app.state.mode, ThemeMode::Dark);
    app.follow_settings(Some(ThemeMode::Light));
    assert_eq!(app.state.mode, ThemeMode::Light);
    // A fixed choice ignores the system.
    set(&mut app, "theme", json!("light"));
    app.follow_settings(Some(ThemeMode::Dark));
    assert_eq!(app.state.mode, ThemeMode::Light);
}

#[test]
fn settings_open_from_the_palette_and_every_setting_is_a_row_of_the_page() {
    let mut app = UiApp::new();
    app.perform(vec![Action::OpenPalette]);
    app.state.palette.query = "settings".to_owned();
    let first = app
        .palette_results()
        .first()
        .map(|e| e.key.clone())
        .unwrap();
    assert_eq!(first, "ui:settings");
    app.perform(vec![Action::PaletteRun(first)]);
    assert_eq!(app.state.selection, Selection::Settings);
    // The page state is data.
    app.state.settings_query = "nca".to_owned();
    let text = serde_json::to_string(&app.state).unwrap();
    let back: crate::app::UiState = serde_json::from_str(&text).unwrap();
    assert_eq!(back, app.state);
}
