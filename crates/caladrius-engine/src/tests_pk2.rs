//! Two-compartment models through the commands (task T-034a): the schemas list the `pk2.*` ids,
//! `model.simulate`, `fit.evaluate` and `fit.run` work on a pk2 profile, and
//! `fit.initial_estimates` refuses pk2 with the way out. Parameters and expected values are the
//! public ones of `oracle/expected/models/pk2/` (clearance set cl 2, vc 10, q 4, vp 8, dose 100).

use serde_json::{Value, json};

use crate::schema::validate;
use crate::*;

fn run(engine: &mut Engine, id: &str, params: Value) -> Value {
    match engine.execute(id, params) {
        Ok(v) => v,
        Err(e) => panic!("{id} failed: {e}"),
    }
}

fn refuse(engine: &mut Engine, id: &str, params: Value) -> CommandError {
    match engine.execute(id, params) {
        Ok(v) => panic!("{id} should have failed, gave {v}"),
        Err(e) => e,
    }
}

const TIMES: [f64; 14] = [
    0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 36.0, 48.0,
];

fn numbers(v: &Value) -> Vec<f64> {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

/// A simulated pk2 profile with a small deterministic alternating error (1 %), as a worksheet.
fn engine_with_profile(model: &str, params: Value, dose: f64) -> Engine {
    let mut e = Engine::new();
    let sim = run(
        &mut e,
        "model.simulate",
        json!({ "model": model, "dose": dose, "params": params, "times": TIMES }),
    );
    let mut csv = String::from("Subject,Time (h),Conc (mg/L),Dose (mg)\n");
    for (i, (t, c)) in TIMES.iter().zip(numbers(&sim["conc"])).enumerate() {
        let noise = if i % 2 == 0 { 1.01 } else { 0.99 };
        csv.push_str(&format!("A,{t},{},{dose}\n", c * noise));
    }
    run(&mut e, "data.import", json!({ "name": "pk2", "csv": csv }));
    e
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs()
}

#[test]
fn every_pk2_id_is_in_the_schemas_with_the_parameter_sets_in_one_line() {
    for id in [
        "fit.run",
        "fit.evaluate",
        "fit.initial_estimates",
        "model.simulate",
    ] {
        let info = describe().into_iter().find(|c| c.id == id).unwrap();
        let schema = &info.params_schema;
        let text = schema.to_string();
        for m in caladrius_models::ModelId::TWO_COMPARTMENT {
            assert!(text.contains(m.id()), "{id} lacks {}", m.id());
        }
        assert!(text.contains("k10,k12,k21,vc"), "{id}: parameter sets");
        let mut params = info.example.clone();
        params["model"] = json!("pk2.oral_1_lag");
        if let Err(why) = validate::check(schema, schema, &params) {
            panic!("{id}: {why}");
        }
        params["model"] = json!("pk2.bogus");
        assert!(validate::check(schema, schema, &params).is_err(), "{id}");
    }
}

#[test]
fn a_pk2_model_is_simulated_in_the_three_parameter_sets() {
    let mut e = Engine::new();
    // Oracle `model_pk2_iv_bolus_base` (and its micro and macro forms): C(1), C(5) and AUC(0, 1).
    let sets = [
        json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8 }),
        json!({ "k10": 0.2, "k12": 0.4, "k21": 0.5, "vc": 10 }),
        json!({ "a": 5.555555555555555, "b": 4.444444444444445, "alpha": 1, "beta": 0.1 }),
    ];
    for params in sets {
        let r = run(
            &mut e,
            "model.simulate",
            json!({ "model": "pk2.iv_bolus", "dose": 100, "params": params, "times": [0, 1, 5] }),
        );
        let conc = numbers(&r["conc"]);
        let auc = numbers(&r["auc"]);
        assert_eq!(conc[0], 10.0);
        assert!(close(conc[1], 6.065_274_308_890_055, 1e-12), "{conc:?}");
        assert!(close(conc[2], 2.7331248598288456, 1e-12), "{conc:?}");
        assert!(close(auc[1], 7.741_228_969_671_561, 1e-12), "{auc:?}");
        for name in ["alpha", "beta", "vss"] {
            assert!(r["secondary"][name].as_f64().is_some(), "{name}");
        }
    }
    // Oracle `model_pk2_oral_1_base`: ka 2.
    let r = run(
        &mut e,
        "model.simulate",
        json!({ "model": "pk2.oral_1", "dose": 100,
                "params": { "cl": 2, "vc": 10, "q": 4, "vp": 8, "ka": 2 }, "times": [1, 2, 5] }),
    );
    let conc = numbers(&r["conc"]);
    assert!(close(conc[0], 6.183_833_964_419_028, 1e-12), "{conc:?}");
    assert!(close(conc[2], 2.9117195746082483, 1e-12), "{conc:?}");
    // A stored pk2 simulation exports its table like any other.
    let s = run(
        &mut e,
        "model.simulate",
        json!({ "model": "pk2.iv_infusion", "dose": 100,
                "params": { "cl": 2, "vc": 10, "q": 4, "vp": 8, "dur": 2 },
                "grid": { "start": 0, "end": 24, "points": 25 }, "store": true }),
    );
    let table = run(
        &mut e,
        "export.table",
        json!({ "table": "simulation", "analysis": s["analysis"] }),
    );
    assert_eq!(table["rows"].as_array().map(Vec::len), Some(25));
}

#[test]
fn a_pk2_simulation_error_names_the_parameter_and_the_way_out() {
    let mut e = Engine::new();
    let base = |params: Value| json!({ "model": "pk2.iv_bolus", "dose": 100, "params": params, "times": [1] });
    // Q = 0 is a one-compartment model.
    let one = refuse(
        &mut e,
        "model.simulate",
        base(json!({ "cl": 2, "vc": 10, "q": 0, "vp": 8 })),
    );
    assert_eq!(one.code, "model_error");
    assert!(
        one.message.contains('q') && one.message.contains("pk1"),
        "{one}"
    );
    // A mixture of two sets is refused naming the parameter.
    let mixed = refuse(
        &mut e,
        "model.simulate",
        base(json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8, "k10": 0.2 })),
    );
    assert_eq!(mixed.code, "model_error");
    assert!(mixed.message.contains("k10"), "{mixed}");
    // One-compartment parameter names are not pk2 parameters.
    let v = refuse(&mut e, "model.simulate", base(json!({ "v": 10, "k": 0.2 })));
    assert_eq!(v.code, "model_error");
}

#[test]
fn fit_initial_estimates_refuses_pk2_and_says_to_supply_them() {
    let mut e = engine_with_profile(
        "pk2.iv_bolus",
        json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8 }),
        100.0,
    );
    let r = refuse(
        &mut e,
        "fit.initial_estimates",
        json!({ "worksheet": 1, "model": "pk2.iv_bolus" }),
    );
    assert_eq!(r.code, "fit_error");
    assert!(
        r.message.contains("two-compartment") && r.message.contains("enter initial estimates"),
        "{r}"
    );
    // The same refusal reaches fit.run and fit.evaluate without initial values, and nothing is
    // stored.
    for id in ["fit.run", "fit.evaluate"] {
        let r = refuse(
            &mut e,
            id,
            json!({ "worksheet": 1, "model": "pk2.iv_bolus" }),
        );
        assert_eq!(r.code, "fit_error", "{id}");
        assert!(r.message.contains("initial"), "{id}: {r}");
    }
    assert!(e.project().analyses().is_empty());
}

#[test]
fn fit_evaluate_and_fit_run_work_on_a_pk2_profile() {
    let truth = json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8 });
    let mut e = engine_with_profile("pk2.iv_bolus", truth.clone(), 100.0);
    let start = json!({ "cl": 1.6, "vc": 11, "q": 3.2, "vp": 9 });
    let ev = run(
        &mut e,
        "fit.evaluate",
        json!({ "worksheet": 1, "model": "pk2.iv_bolus", "initial": start }),
    );
    let at_start = ev["wrss"].as_f64().unwrap_or(f64::NAN);
    assert!(at_start > 0.0 && at_start.is_finite(), "{ev}");
    assert_eq!(ev["n_observations"], 14);
    let fit = run(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "model": "pk2.iv_bolus", "initial": start }),
    );
    let ok = &fit["result"]["outcome"]["ok"];
    assert_eq!(ok["status"], "converged", "{ok}");
    for name in ["cl", "vc", "q", "vp"] {
        let got = ok["values"][format!("estimate.{name}")]
            .as_f64()
            .unwrap_or(f64::NAN);
        let want = truth[name].as_f64().unwrap_or(f64::NAN);
        assert!(close(got, want, 0.1), "{name}: {got} against {want}");
    }
    let table = run(
        &mut e,
        "export.table",
        json!({ "table": "fit.parameters", "analysis": fit["id"] }),
    );
    assert_eq!(table["rows"].as_array().map(Vec::len), Some(4));
    // The macro set fits too (the dose is the one of the worksheet).
    let macro_start = json!({ "a": 5.0, "b": 5.0, "alpha": 1.1, "beta": 0.11 });
    let fit = run(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "model": "pk2.iv_bolus", "initial": macro_start }),
    );
    assert_eq!(fit["result"]["outcome"]["ok"]["status"], "converged");
}

#[test]
fn a_pk2_oral_fit_runs() {
    let truth = json!({ "cl": 2, "vc": 10, "q": 4, "vp": 8, "ka": 2 });
    let mut e = engine_with_profile("pk2.oral_1", truth, 100.0);
    let fit = run(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "model": "pk2.oral_1",
                "initial": { "cl": 1.8, "vc": 11, "q": 3.5, "vp": 9, "ka": 1.6 } }),
    );
    let ok = &fit["result"]["outcome"]["ok"];
    assert_eq!(ok["status"], "converged", "{ok}");
    assert!(ok["values"]["estimate.ka"].as_f64().is_some());
}
