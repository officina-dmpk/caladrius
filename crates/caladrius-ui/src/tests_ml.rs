//! The maximum-likelihood fit on the fit page (demo of 2026-11-16): the objective and the error
//! model sent as the engine takes them, uniform weights, the results read from the engine's answer
//! (-2LL, AIC and BIC of the likelihood, the least-squares forms labelled as such, the error
//! parameter, the IWRES) and the page drawn without a panic.

use egui_kittest::kittest::Queryable;

use crate::app::{Action, Selection, UiApp};
use crate::tests::{ORAL, harness, with_oral};

/// The oral profile of the other tests without its sample at time 0, where the model predicts 0
/// and a proportional error has no standard deviation (the engine refuses it, tested below).
fn oral_after_dose() -> UiApp {
    let csv: String = ORAL
        .lines()
        .filter(|line| !line.starts_with("0,"))
        .map(|line| format!("{line}\n"))
        .collect();
    let mut app = UiApp::new();
    app.load_csv("oral.csv", csv.as_bytes());
    app.perform(vec![Action::ImportConfirm]);
    app
}

fn with_ml_fit(error_model: &str) -> UiApp {
    let mut app = oral_after_dose();
    app.perform(vec![Action::NewFit]);
    if let Some(page) = app.state.fit.as_mut() {
        page.weighting = "inv_y2".to_owned();
        page.set_maximum_likelihood(true);
        page.set_option(&["error_model"], serde_json::json!(error_model));
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    app
}

#[test]
fn a_maximum_likelihood_fit_runs_from_the_page_and_shows_the_likelihood() {
    let mut app = with_ml_fit("proportional");
    let Selection::Analysis(id) = app.state.selection.clone() else {
        panic!(
            "no analysis: {:?}",
            app.fit_page().and_then(|p| p.error.clone())
        );
    };
    let page = app.fit_page().unwrap().clone();
    let ok = page.ok().expect("the fit has a result");
    assert_eq!(ok.status, "converged");
    assert!(ok.maximum_likelihood());
    assert_eq!(ok.error_model.as_deref(), Some("proportional"));
    // The error parameter is estimated with the model's.
    assert_eq!(ok.parameters, ["v", "k", "ka", "error_b"]);
    for key in [
        "m2ll",
        "aic_ml",
        "bic_ml",
        "estimate.error_b",
        "se.error_b",
        "iwres_sd",
        "wrss",
        "aic",
    ] {
        assert!(ok.value(key).is_some(), "{key}");
    }
    assert!(ok.trace.iter().all(|row| row.m2ll.is_some()));
    assert!(
        ok.observations
            .iter()
            .all(|o| o.sigma.is_some() && o.iwres.is_some())
    );
    // The stored request: maximum likelihood, uniform weights (the person's 1/y² is kept aside).
    let spec = &page.view.as_ref().unwrap().spec;
    assert_eq!(spec["weighting"], "uniform");
    assert_eq!(spec["options"]["objective"], "ml");
    assert_eq!(page.weighting, "inv_y2");
    // The same answer as fit.run called directly with these parameters.
    let direct = app
        .engine_mut()
        .execute("fit.run", page.run_params())
        .unwrap();
    assert_eq!(
        direct
            .pointer("/result/outcome/ok/values/m2ll")
            .and_then(|v| v.as_f64()),
        ok.value("m2ll")
    );
    // Reopened from the project, the page is still a maximum-likelihood fit.
    app.perform(vec![Action::Select(Selection::Worksheet(1))]);
    app.perform(vec![Action::Select(Selection::Analysis(id))]);
    let back = app.fit_page().unwrap();
    assert!(back.maximum_likelihood());
    assert_eq!(back.error_model(), "proportional");
    assert_eq!(back.option_f64("/max_iterations", 0.0), 500.0);
    // The page drawn: the likelihood labels and the least-squares forms named as such.
    let mut h = harness(app);
    h.run_steps(3);
    for label in [
        "-2 log-likelihood",
        "AIC",
        "BIC",
        "AIC, least-squares form",
        "b (proportional error)",
        "Error model",
        "IWRES SD",
    ] {
        assert!(h.query_all_by_label(label).count() > 0, "{label} not shown");
    }
}

#[test]
fn every_error_model_fits_and_going_back_to_least_squares_fits_again() {
    for model in ["additive", "proportional", "combined", "combined_sd"] {
        let app = with_ml_fit(model);
        let page = app.fit_page().unwrap();
        let ok = page.ok().unwrap_or_else(|| panic!("{model}: no result"));
        assert!(ok.value("m2ll").is_some(), "{model}");
        let mut h = harness(app);
        h.run_steps(3);
    }
    let mut app = with_ml_fit("combined");
    if let Some(page) = app.state.fit.as_mut() {
        page.set_maximum_likelihood(false);
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    let page = app.fit_page().unwrap();
    let ok = page.ok().unwrap();
    assert!(!ok.maximum_likelihood());
    assert!(ok.value("m2ll").is_none());
    assert_eq!(page.view.as_ref().unwrap().spec["weighting"], "inv_y2");
}

#[test]
fn a_proportional_error_on_a_sample_predicted_at_zero_is_refused_with_what_to_do() {
    let mut app = with_oral();
    app.perform(vec![Action::NewFit]);
    if let Some(page) = app.state.fit.as_mut() {
        page.set_maximum_likelihood(true);
    }
    app.perform(vec![
        Action::FitChanged { regenerate: false },
        Action::RunFit,
    ]);
    let message = app.fit_page().unwrap().error.clone().unwrap_or_default();
    assert!(message.contains("at time 0"), "{message}");
    assert!(message.contains("additive"), "{message}");
    assert!(app.engine().project().analyses().is_empty());
    harness(app).run_steps(3);
}
