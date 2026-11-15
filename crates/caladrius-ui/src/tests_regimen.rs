//! Repeated doses and the steady state on the simulation page (demo of 2026-11-16): the regimen
//! sent as the engine takes it, the page's answer equal to `model.simulate` called directly, and
//! the values checked against two public cases of the multiple-dosing oracle
//! (`oracle/expected/models/md/`, `specs/models.md` section 12).

use egui_kittest::kittest::Queryable;

use crate::app::{Action, UiApp};
use crate::modelinfo::Input;
use crate::sim::Dosing;
use crate::tests::harness;

/// IV boluses of 100 every 6 at steady state, V = 10, CL = 2.
const BOLUS_SS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/expected/models/md/model_md_pk1_iv_bolus_ss_tau_6.csv"
));

/// Ten 2-hour infusions of 100 every 6, V = 10, CL = 2.
const INFUSION_N10: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/expected/models/md/model_md_pk1_iv_infusion_regular_n10.csv"
));

/// A value of an oracle case: `scalar,<name>,<value>` or `t=<time>,conc,<value>`.
fn oracle(csv: &str, subject: &str, parameter: &str) -> f64 {
    csv.lines()
        .map(|l| l.split(',').collect::<Vec<_>>())
        .find(|f| f.len() == 3 && f[0] == subject && f[1] == parameter)
        .and_then(|f| f[2].parse().ok())
        .unwrap_or_else(|| panic!("{subject} {parameter} not in the case"))
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-300)
}

/// The simulation page with one compartment, V = 10, k = 0.2 (CL = 2), a dose of 100.
fn simulation(input: Input, dosing: Dosing, tau: f64, n_doses: u32) -> UiApp {
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulation]);
    if let Some(p) = app.state.sim.as_mut() {
        p.input = input;
        p.duration = 2.0;
        p.dose = 100.0;
        p.params.insert("v".to_owned(), 10.0);
        p.params.insert("k".to_owned(), 0.2);
        p.tau = tau;
        p.n_doses = n_doses;
        p.set_dosing(dosing);
    }
    app.perform(vec![Action::SimChanged]);
    app
}

#[test]
fn the_steady_state_of_the_page_is_the_oracles() {
    let mut app = simulation(Input::Bolus, Dosing::SteadyState, 6.0, 1);
    let page = app.sim_page().unwrap().clone();
    assert_eq!(page.error, None);
    let result = page.result.clone().unwrap();
    for name in ["cmax_ss", "cmin_ss", "cav_ss", "auc_tau_ss", "accum_cmax"] {
        let want = oracle(BOLUS_SS, "scalar", name);
        let got = result.secondary[name];
        assert!(close(got, want, 1e-9), "{name}: {got} against {want}");
    }
    // One interval, times since the last dose: from 0 to tau.
    assert_eq!(result.times.first(), Some(&0.0));
    assert_eq!(result.times.last(), Some(&6.0));
    assert!(close(result.conc[0], oracle(BOLUS_SS, "t=0", "conc"), 1e-9));
    let direct = app
        .engine_mut()
        .execute("model.simulate", page.params(false))
        .unwrap();
    assert_eq!(
        crate::model::read::<crate::model::Simulation>(&direct).conc,
        result.conc
    );
    // Saved, then opened again: the same steady state, the regimen apart from the parameters.
    app.perform(vec![Action::SimSave]);
    let crate::app::Selection::Analysis(id) = app.state.selection.clone() else {
        panic!("a saved simulation is shown as an analysis");
    };
    app.perform(vec![Action::Select(crate::app::Selection::Welcome)]);
    app.perform(vec![Action::Select(crate::app::Selection::Analysis(id))]);
    let back = app.sim_page().unwrap().clone();
    assert_eq!((back.dosing, back.tau), (Dosing::SteadyState, 6.0));
    assert!(!back.params.contains_key("tau"));
    app.perform(vec![Action::SimChanged]);
    let again = app.sim_page().unwrap().result.clone().unwrap();
    assert_eq!(again.secondary["cmin_ss"], result.secondary["cmin_ss"]);
    // Drawn, with the steady-state values named.
    let mut h = harness(app);
    h.run_steps(3);
    for label in [
        "Cmin at steady state, trough",
        "Cmax at steady state",
        "Accumulation ratio of Cmax",
        "Steady state",
        "Every",
    ] {
        assert!(h.query_all_by_label(label).count() > 0, "{label} not shown");
    }
}

#[test]
fn repeated_infusions_on_the_page_are_the_oracles_and_every_peak_is_drawn() {
    let mut app = simulation(Input::Infusion, Dosing::Repeated, 6.0, 10);
    let page = app.sim_page().unwrap().clone();
    assert_eq!(page.error, None);
    // The curve covers the ten doses.
    assert_eq!(page.end, 60.0);
    let result = page.result.clone().unwrap();
    assert!(close(
        result.secondary["auc_inf"],
        oracle(INFUSION_N10, "scalar", "auc_inf"),
        1e-9
    ));
    // The engine at the oracle's own times, with the page's regimen.
    let mut at = page.params(false);
    at.as_object_mut().unwrap().remove("grid");
    let times = [2.0, 8.0, 32.0, 56.0];
    at["times"] = serde_json::json!(times);
    let direct = app.engine_mut().execute("model.simulate", at).unwrap();
    let sim: crate::model::Simulation = crate::model::read(&direct);
    for (t, c) in times.iter().zip(&sim.conc) {
        let want = oracle(INFUSION_N10, &format!("t={t}"), "conc");
        assert!(close(*c, want, 1e-9), "t = {t}: {c} against {want}");
    }
    // The highest point of the drawn curve is within 0.5% of the last peak (end of the 10th
    // infusion, t = 56): the grid is dense enough.
    let peak = result.conc.iter().copied().fold(f64::MIN, f64::max);
    let last_peak = oracle(INFUSION_N10, "t=56", "conc");
    assert!(close(peak, last_peak, 5e-3), "{peak} against {last_peak}");
    let mut h = harness(app);
    h.run_steps(3);
    for label in [
        "Number of doses",
        "AUC to infinity, all doses",
        "Repeated doses",
    ] {
        assert!(h.query_all_by_label(label).count() > 0, "{label} not shown");
    }
}

#[test]
fn a_stored_number_of_doses_above_the_field_is_kept_when_drawn_and_saved() {
    let over = crate::sim::MAX_DOSES + 500;
    let mut app = simulation(Input::Bolus, Dosing::Repeated, 6.0, over);
    assert_eq!(app.sim_page().unwrap().error, None);
    app.perform(vec![Action::SimSave]);
    let crate::app::Selection::Analysis(id) = app.state.selection.clone() else {
        panic!("a saved simulation is shown as an analysis");
    };
    app.perform(vec![Action::Select(crate::app::Selection::Welcome)]);
    app.perform(vec![Action::Select(crate::app::Selection::Analysis(id))]);
    assert_eq!(app.sim_page().unwrap().n_doses, over);
    // Drawn: the field does not bring the stored number down to what it offers, and says so.
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(h.state().sim_page().unwrap().n_doses, over);
    assert!(
        h.query_all_by_label_contains("as stored: more than the")
            .next()
            .is_some()
    );
    // Saved again from the page: the stored number is unchanged.
    h.state_mut().perform(vec![Action::SimSave]);
    let app = h.state_mut();
    let crate::app::Selection::Analysis(again) = app.state.selection.clone() else {
        panic!("a saved simulation is shown as an analysis");
    };
    let view = app
        .engine_mut()
        .execute("analysis.get", serde_json::json!({ "analysis": again }))
        .unwrap();
    assert_eq!(view["spec"]["input"]["params"]["n_doses"], f64::from(over));
}

#[test]
fn going_back_to_one_dose_sends_no_regimen_and_three_compartments_take_one() {
    let mut app = simulation(Input::Bolus, Dosing::Repeated, 6.0, 3);
    if let Some(p) = app.state.sim.as_mut() {
        p.set_dosing(Dosing::Single);
    }
    app.perform(vec![Action::SimChanged]);
    let page = app.sim_page().unwrap();
    assert!(page.params(false)["params"].get("tau").is_none());
    let secondary = &page.result.as_ref().unwrap().secondary;
    assert!(secondary.contains_key("auc_inf") && !secondary.contains_key("cmin_ss"));
    // Three compartments at steady state: the engine answers, the page shows it.
    let mut app = UiApp::new();
    app.perform(vec![Action::NewSimulationWith(
        crate::modelinfo::Compartments::Three,
    )]);
    if let Some(p) = app.state.sim.as_mut() {
        p.tau = 12.0;
        p.set_dosing(Dosing::SteadyState);
    }
    app.perform(vec![Action::SimChanged]);
    let page = app.sim_page().unwrap();
    assert_eq!(page.error, None);
    assert!(
        page.result
            .as_ref()
            .unwrap()
            .secondary
            .contains_key("cmin_ss")
    );
    harness(app).run_steps(3);
}
