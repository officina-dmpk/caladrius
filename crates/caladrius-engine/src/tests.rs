//! Tests of the command registry: every command dispatches, schemas match the serde types,
//! stale marking is visible through the commands, the history is append-only.

use serde_json::{Value, json};

use crate::schema::validate;
use crate::*;

fn ok(engine: &mut Engine, id: &str, params: Value) -> Value {
    match engine.execute(id, params) {
        Ok(v) => v,
        Err(e) => panic!("{id} failed: {e}"),
    }
}

fn err(engine: &mut Engine, id: &str, params: Value) -> CommandError {
    match engine.execute(id, params) {
        Ok(v) => panic!("{id} should have failed, gave {v}"),
        Err(e) => e,
    }
}

fn info(id: &str) -> CommandInfo {
    describe().into_iter().find(|c| c.id == id).unwrap()
}

/// Two subjects, oral profiles; the second has a missing concentration.
const TWO_SUBJECTS: &str = "Subject,Time (h),Conc (mg/L),Dose (mg)\n\
A,0,0,100\nA,0.5,2.195,100\nA,1,3.293,100\nA,2,3.971,100\nA,4,3.611,100\nA,6,2.989,100\nA,8,2.451,100\nA,12,1.643,100\nA,24,0.495,100\n\
B,0,0,100\nB,0.5,1.8,100\nB,1,2.9,100\nB,2,3.5,100\nB,4,3.2,100\nB,6,2.6,100\nB,8,,100\nB,12,1.4,100\nB,24,0.4,100\n";

fn engine_with_two_subjects() -> Engine {
    let mut e = Engine::new();
    ok(
        &mut e,
        "data.import",
        json!({ "name": "study", "csv": TWO_SUBJECTS }),
    );
    e
}

fn assert_valid(command: &str, schema: &Value, instance: &Value) {
    if let Err(why) = validate::check(schema, schema, instance) {
        panic!("{command}: {why}\ninstance: {instance}");
    }
}

// ---- the registry ----------------------------------------------------------------------

#[test]
fn the_commands_are_the_documented_ones() {
    assert_eq!(
        command_ids(),
        [
            "data.preview",
            "data.import",
            "data.describe",
            "data.set_column",
            "data.set_cell",
            "nca.run",
            "analysis.get",
            "fit.initial_estimates",
            "fit.run",
            "model.simulate",
            "analysis.run",
            "export.table",
            "project.describe",
            "project.save",
            "history.list",
            "analysis.remove",
            "data.remove",
            "project.new",
            "project.load",
        ]
    );
    let all = describe();
    for c in &all {
        assert!(!c.title.is_empty() && !c.description.is_empty(), "{}", c.id);
        for schema in [&c.params_schema, &c.result_schema] {
            assert_eq!(schema["$schema"], schema::DIALECT, "{}", c.id);
            assert!(
                schema["title"]
                    .as_str()
                    .is_some_and(|t| t.starts_with(&c.id))
            );
        }
        assert_eq!(c.params_schema["type"], "object", "{}", c.id);
    }
    // The description is plain data: it round-trips through JSON as the MCP server will send it.
    let text = serde_json::to_string(&all).unwrap();
    let back: Vec<CommandInfo> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, all);
}

#[test]
fn every_reference_in_a_schema_is_defined() {
    fn refs(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                if let Some(Value::String(r)) = m.get("$ref") {
                    out.push(r.clone());
                }
                m.values().for_each(|x| refs(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| refs(x, out)),
            _ => {}
        }
    }
    for c in describe() {
        for schema in [&c.params_schema, &c.result_schema] {
            let mut found = Vec::new();
            refs(schema, &mut found);
            for r in found {
                let name = r.strip_prefix("#/$defs/").unwrap();
                assert!(
                    schema.pointer(&format!("/$defs/{name}")).is_some(),
                    "{}: {r} is not defined",
                    c.id
                );
            }
        }
    }
}

#[test]
fn every_command_dispatches_and_its_example_and_result_match_the_schemas() {
    // The examples are a script: each succeeds against the state the previous ones left.
    let mut engine = Engine::new();
    for c in describe() {
        assert_valid(&c.id, &c.params_schema, &c.example);
        let result = engine
            .execute(&c.id, c.example.clone())
            .unwrap_or_else(|e| panic!("example of {} failed: {e}", c.id));
        assert_valid(&c.id, &c.result_schema, &result);
    }
    assert_eq!(engine.history().len(), describe().len());
    assert!(engine.history().entries().iter().all(|e| e.ok));
}

#[test]
fn every_command_refuses_unknown_and_malformed_parameters_with_a_readable_error() {
    let mut engine = Engine::new();
    for c in describe() {
        let e = err(&mut engine, &c.id, json!({ "no_such_field": 1 }));
        assert_eq!(e.code, "invalid_parameters", "{}: {e}", c.id);
        assert!(e.message.contains(&c.id), "{}", e.message);
        assert!(e.message.contains("no_such_field"), "{}", e.message);
        let e = err(&mut engine, &c.id, json!([1, 2]));
        assert_eq!(e.code, "invalid_parameters", "{}", c.id);
    }
    let e = err(&mut engine, "nca.run", Value::Null);
    assert!(e.message.contains("worksheet"), "{e}");
    let e = err(&mut engine, "no.such.command", json!({}));
    assert_eq!(e.code, "unknown_command");
    assert!(e.message.contains("nca.run"));
}

#[test]
fn a_schema_rejects_what_the_serde_types_reject() {
    let schema = info("nca.run").params_schema;
    for bad in [
        json!({ "worksheet": 0 }),
        json!({ "worksheet": 1, "extra": true }),
        json!({ "worksheet": 1, "route": "intramuscular" }),
        json!({ "worksheet": 1, "options": { "auc_method": "cubic" } }),
        json!({ "worksheet": 1, "options": { "lambda_z": { "min_points": 1 } } }),
        json!({}),
    ] {
        assert!(validate::check(&schema, &schema, &bad).is_err(), "{bad}");
    }
    let mut engine = Engine::new();
    for bad in [
        json!({ "worksheet": 1, "route": "intramuscular" }),
        json!({ "worksheet": 1, "options": { "auc_method": "cubic" } }),
        json!({ "worksheet": 1, "extra": true }),
    ] {
        assert_eq!(err(&mut engine, "nca.run", bad).code, "invalid_parameters");
    }
}

#[test]
fn option_schemas_accept_what_the_serde_types_write() {
    use caladrius_fit::{Bounds, Derivatives, FitOptions};
    use caladrius_nca::{
        BlqAction, BlqPolicy, LambdaZManual, LambdaZSelection, MissingPolicy, NcaOptions,
        NegativePolicy, QualityThresholds, StartPolicy,
    };
    let nca = info("nca.run").params_schema;
    let mut variants = vec![NcaOptions::default()];
    let tmax_blq = NcaOptions {
        missing: MissingPolicy::Replace(0.5),
        negative: NegativePolicy::SetZero,
        start: StartPolicy::Zero,
        blq: BlqPolicy::Tmax {
            before: BlqAction::Set(0.25),
            after: BlqAction::Drop,
        },
        lambda_z_selection: LambdaZSelection {
            manual: Some(LambdaZManual::Times(vec![4.0, 6.0, 8.0])),
            ..Default::default()
        },
        quality: QualityThresholds {
            min_points: None,
            ..Default::default()
        },
        ..Default::default()
    };
    let ranged = NcaOptions {
        lambda_z_selection: LambdaZSelection {
            manual: Some(LambdaZManual::Range {
                start: 4.0,
                end: 12.0,
            }),
            ..Default::default()
        },
        ..tmax_blq.clone()
    };
    variants.push(tmax_blq);
    variants.push(ranged);
    for options in variants {
        let params = json!({ "worksheet": 1, "options": options });
        assert_valid("nca.run", &nca, &params);
    }
    let fit = info("fit.run").params_schema;
    let analytic = FitOptions {
        derivatives: Derivatives::Analytic,
        fixed: [("dur".to_owned(), 2.0)].into(),
        bounds: [(
            "v".to_owned(),
            Bounds {
                lower: Some(1.0),
                upper: None,
            },
        )]
        .into(),
        ..Default::default()
    };
    for options in [FitOptions::default(), analytic] {
        let params = json!({ "worksheet": 1, "model": "pk1.iv_bolus", "options": options });
        assert_valid("fit.run", &fit, &params);
    }
    // Every model id of the catalogue is accepted.
    for m in caladrius_models::ModelId::ALL {
        let params = json!({ "worksheet": 1, "model": m.id() });
        assert_valid("fit.run", &fit, &params);
    }
    // Routes as the NCA writes them.
    for route in [
        caladrius_nca::Route::Extravascular,
        caladrius_nca::Route::IvBolus,
        caladrius_nca::Route::IvInfusion { duration: 1.5 },
    ] {
        assert_valid("nca.run", &nca, &json!({ "worksheet": 1, "route": route }));
    }
}

// ---- workflows ---------------------------------------------------------------------------

#[test]
fn import_describes_what_it_guessed() {
    let mut e = Engine::new();
    let r = ok(
        &mut e,
        "data.import",
        json!({ "name": "study", "csv": TWO_SUBJECTS }),
    );
    let ws = &r["worksheet"];
    assert_eq!(ws["rows"], 18);
    assert_eq!(ws["subjects"], json!(["A", "B"]));
    let roles: Vec<&str> = ws["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["subject", "time", "concentration", "dose"]);
    assert_eq!(ws["columns"][2]["missing"], 1);
    assert_eq!(ws["derived_units"]["auc"], "h*mg/L");
    assert_eq!(ws["unit_warnings"], json!([]));
    // Explicit overrides win over the guess, and a bad one is refused without side effects.
    let r = ok(
        &mut e,
        "data.import",
        json!({ "name": "again", "csv": "t,y\n0,0\n1,5\n",
                "columns": [{ "name": "t", "role": "time", "unit": "min" },
                            { "name": "y", "role": "concentration", "unit": "ng/mL" }] }),
    );
    assert_eq!(r["worksheet"]["columns"][0]["unit"], "min");
    let before = e.project().clone();
    let bad = err(
        &mut e,
        "data.import",
        json!({ "name": "x", "csv": "t,y\n0,0\n", "columns": [{ "name": "nope", "role": "time" }] }),
    );
    assert_eq!(bad.code, "unknown_column");
    assert_eq!(e.project(), &before);
    // The Rust entry point for bytes goes through the same command.
    e.import_csv("from bytes", b"time,conc\n0,1\n1,2\n")
        .unwrap();
    let non_utf8 = e.import_csv("bad", &[0xff, 0xfe]).unwrap_err();
    assert_eq!(non_utf8.code, "csv_not_utf8");
    assert_eq!(e.project().worksheets().len(), 3);
}

#[test]
fn units_are_checked_together_at_import() {
    let mut e = Engine::new();
    let r = ok(
        &mut e,
        "data.import",
        json!({ "name": "u", "csv": "Time (h),Conc (ng/mL),Dose (mg)\n0,0,5\n1,3,5\n" }),
    );
    let codes: Vec<&str> = r["worksheet"]["unit_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["mass_mismatch"]);
}

#[test]
fn nca_runs_per_subject_and_reports_each_failure() {
    let mut e = engine_with_two_subjects();
    let r = ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    assert_eq!(r["status"], json!({ "state": "fresh" }));
    assert_eq!(r["label"], "NCA of study, all subjects");
    let subjects = r["result"]["subjects"].as_array().unwrap();
    assert_eq!(subjects.len(), 2);
    for s in subjects {
        let params = s["outcome"]["ok"]["parameters"].as_array().unwrap();
        let cmax = params.iter().find(|p| p["name"] == "cmax").unwrap();
        assert!(cmax["value"]["value"].as_f64().unwrap() > 3.0);
        assert_eq!(s["dose"], 100.0);
    }
    // Without a route there is nothing to guess: each subject says what to fix.
    let r = ok(&mut e, "nca.run", json!({ "worksheet": 1 }));
    for s in r["result"]["subjects"].as_array().unwrap() {
        assert!(s["outcome"]["error"].as_str().unwrap().contains("route"));
    }
    // A named subject that does not exist is an error and creates nothing.
    let before = e.project().clone();
    let bad = err(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular", "subject": "Z" }),
    );
    assert_eq!(bad.code, "unknown_subject");
    assert_eq!(e.project(), &before);
    let one = ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular", "subject": "B" }),
    );
    assert_eq!(one["result"]["subjects"].as_array().unwrap().len(), 1);
}

#[test]
fn a_route_column_is_read_and_a_missing_dose_only_blanks_dependent_parameters() {
    let mut e = Engine::new();
    ok(
        &mut e,
        "data.import",
        json!({ "name": "iv", "csv": "time,conc,route\n0.25,10,iv_bolus\n1,6,iv_bolus\n2,3.6,iv_bolus\n4,1.3,iv_bolus\n6,0.5,iv_bolus\n" }),
    );
    let r = ok(&mut e, "nca.run", json!({ "worksheet": 1 }));
    let s = &r["result"]["subjects"][0];
    assert_eq!(s["route"], "iv_bolus");
    assert_eq!(s["dose"], Value::Null);
    let params = s["outcome"]["ok"]["parameters"].as_array().unwrap();
    let get = |n: &str| params.iter().find(|p| p["name"] == n).unwrap()["value"].clone();
    assert!(get("auclast")["value"].as_f64().is_some());
    assert_eq!(get("cl.obs"), json!({ "not_calculated": "dose_missing" }));
}

#[test]
fn stale_marking_is_visible_through_the_commands() {
    let mut e = engine_with_two_subjects();
    ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    let fit = ok(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "subject": "A", "model": "pk1.oral_1" }),
    );
    assert_eq!(fit["result"]["outcome"]["ok"]["status"], "converged");
    assert_eq!(fit["label"], "Fit pk1.oral_1 to study, subject A");
    let described = ok(&mut e, "project.describe", Value::Null);
    assert_eq!(
        described["analyses"][0]["status"],
        json!({ "state": "fresh" })
    );

    // Editing a cell reports which analyses became stale.
    let edit = ok(
        &mut e,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Conc", "row": 3, "value": 4.0 }),
    );
    assert_eq!(edit["stale_analyses"], json!([2, 3]));
    let got = ok(&mut e, "analysis.get", json!({ "analysis": 2 }));
    assert_eq!(got["status"]["state"], "stale");
    assert_eq!(got["status"]["reasons"][0]["code"], "input_changed");
    // The old result is still there, and exporting says it is stale.
    assert!(got["result"]["subjects"].is_array());
    let table = ok(
        &mut e,
        "export.table",
        json!({ "table": "nca.parameters", "analysis": 2 }),
    );
    assert_eq!(table["status"], "stale");

    // Running again makes it fresh; a unit change makes it stale again.
    let again = ok(&mut e, "analysis.run", json!({ "analysis": 2 }));
    assert_eq!(again["status"], json!({ "state": "fresh" }));
    let unit = ok(
        &mut e,
        "data.set_column",
        json!({ "worksheet": 1, "column": "Conc", "unit": "ug/L" }),
    );
    assert_eq!(unit["stale_analyses"], json!([2, 3]));
    // New options on the same analysis replace the stored ones and run it.
    let updated = ok(
        &mut e,
        "nca.run",
        json!({ "analysis": 2, "worksheet": 1, "route": "extravascular",
                "options": { "auc_method": "linear" } }),
    );
    assert_eq!(updated["spec"]["options"]["auc_method"], "linear");
    assert_eq!(updated["status"], json!({ "state": "fresh" }));
    assert_eq!(e.project().analyses().len(), 2);
    // The kind of an analysis cannot change.
    let wrong = err(
        &mut e,
        "fit.run",
        json!({ "analysis": 2, "worksheet": 1, "subject": "A", "model": "pk1.oral_1" }),
    );
    assert_eq!(wrong.code, "analysis_kind_changed");
}

#[test]
fn data_edits_are_checked_and_atomic() {
    let mut e = engine_with_two_subjects();
    let before = e.project().clone();
    for (id, params, code) in [
        (
            "data.set_column",
            json!({ "worksheet": 1, "column": "Subject" }),
            "invalid_parameters",
        ),
        (
            "data.set_column",
            json!({ "worksheet": 1, "column": "Nope", "role": "time" }),
            "unknown_column",
        ),
        (
            "data.set_column",
            json!({ "worksheet": 1, "column": "Subject", "role": "time", "unit": "x" }),
            "role_needs_numbers",
        ),
        (
            "data.set_cell",
            json!({ "worksheet": 1, "column": "Conc", "row": 99, "value": 1 }),
            "row_out_of_range",
        ),
        (
            "data.set_cell",
            json!({ "worksheet": 1, "column": "Conc", "row": 0, "value": "text" }),
            "invalid_number",
        ),
        (
            "data.set_cell",
            json!({ "worksheet": 7, "column": "Conc", "row": 0, "value": 1 }),
            "unknown_worksheet",
        ),
    ] {
        assert_eq!(err(&mut e, id, params).code, code, "{id}");
        assert_eq!(e.project(), &before, "{id} changed the project");
    }
    // A role moves to the new column and the old holder becomes `other`.
    let r = ok(
        &mut e,
        "data.set_column",
        json!({ "worksheet": 1, "column": "Dose", "role": "concentration" }),
    );
    let roles: Vec<&str> = r["worksheet"]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["subject", "time", "other", "concentration"]);
    // A missing value is set with null.
    ok(
        &mut e,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Time", "row": 1, "value": null }),
    );
    let d = ok(
        &mut e,
        "data.describe",
        json!({ "worksheet": 1, "preview_rows": 3 }),
    );
    assert_eq!(d["preview"]["rows"][1][1], Value::Null);
    assert_eq!(d["preview"]["rows"].as_array().unwrap().len(), 3);
}

#[test]
fn fit_and_initial_estimates() {
    let mut e = engine_with_two_subjects();
    let init = ok(
        &mut e,
        "fit.initial_estimates",
        json!({ "worksheet": 1, "subject": "A", "model": "pk1.oral_1" }),
    );
    assert_eq!(init["n_observations"], 9);
    assert_eq!(init["dose"], 100.0);
    for p in ["v", "k", "ka"] {
        assert!(init["initial"][p].as_f64().unwrap() > 0.0, "{p}");
    }
    // A zero concentration cannot be weighted by 1/y: the error says so.
    let zero = err(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "subject": "B", "model": "pk1.oral_1", "weighting": "inv_y" }),
    );
    assert_eq!(zero.code, "fit_error");
    assert!(zero.message.contains("inv_y"), "{zero}");
    // Subject B has a missing concentration: it is left out and counted.
    let fit = ok(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "subject": "B", "model": "pk1.oral_1",
                "initial": init["initial"] }),
    );
    assert_eq!(fit["result"]["n_missing"], 1);
    assert_eq!(fit["result"]["n_observations"], 8);
    let est = &fit["result"]["outcome"]["ok"]["values"];
    assert!(est["estimate.v"].as_f64().unwrap() > 5.0);
    // A subject must be named when there are several.
    assert_eq!(
        err(
            &mut e,
            "fit.run",
            json!({ "worksheet": 1, "model": "pk1.oral_1" })
        )
        .code,
        "subject_required"
    );
    // A fit that cannot start is an error and stores nothing.
    let n = e.project().analyses().len();
    let bad = err(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "subject": "A", "model": "pk1.oral_1", "dose": 100,
                "initial": { "v": 20, "k": 0.1, "ka": 1.0 },
                "options": { "increment": 5.0 } }),
    );
    assert_eq!(bad.code, "fit_error");
    assert_eq!(e.project().analyses().len(), n);
    // A dose is required, from the sheet or the call.
    let nodose = ok(
        &mut e,
        "data.import",
        json!({ "name": "nodose", "csv": "time,conc\n0,0\n1,3\n2,4\n4,3\n8,1\n12,0.5\n" }),
    );
    assert_eq!(
        err(
            &mut e,
            "fit.run",
            json!({ "worksheet": nodose["worksheet"]["id"], "model": "pk1.oral_1" })
        )
        .code,
        "dose_required"
    );
}

#[test]
fn simulation_matches_the_model_crate() {
    let mut e = Engine::new();
    let params = json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": 5, "k": 0.2 },
                         "times": [0, 1, 2, 10] });
    let r = ok(&mut e, "model.simulate", params);
    let direct = caladrius_models::run(&caladrius_models::ModelInput {
        model: caladrius_models::ModelId::IvBolus,
        dose: 10.0,
        params: [("v".to_owned(), 5.0), ("k".to_owned(), 0.2)].into(),
        times: vec![0.0, 1.0, 2.0, 10.0],
    })
    .unwrap();
    let conc: Vec<f64> = serde_json::from_value(r["conc"].clone()).unwrap();
    assert_eq!(conc, direct.conc());
    assert_eq!(
        r["secondary"]["half_life"].as_f64(),
        direct.get("half_life")
    );
    assert_eq!(r["analysis"], Value::Null);
    // Nothing is stored by default, and the project does not change.
    assert!(e.project().analyses().is_empty());
    // A grid is a regular list ending exactly at `end`.
    let g = ok(
        &mut e,
        "model.simulate",
        json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": 5, "cl": 1 },
                "grid": { "start": 0, "end": 10, "points": 5 }, "store": true, "name": "curve" }),
    );
    assert_eq!(g["times"], json!([0.0, 2.5, 5.0, 7.5, 10.0]));
    assert_eq!(e.project().analyses().len(), 1);
    let table = ok(
        &mut e,
        "export.table",
        json!({ "table": "simulation", "analysis": g["analysis"] }),
    );
    assert_eq!(table["columns"], json!(["time", "conc", "auc"]));
    assert_eq!(table["rows"].as_array().unwrap().len(), 5);
    // Errors say what to fix.
    for (params, code, text) in [
        (
            json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": 5, "k": 0.2 } }),
            "invalid_parameters",
            "times",
        ),
        (
            json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": 5, "k": 0.2 },
                    "times": [1], "grid": { "start": 0, "end": 1, "points": 3 } }),
            "invalid_parameters",
            "not both",
        ),
        (
            json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": -5, "k": 0.2 },
                    "times": [1] }),
            "model_error",
            "v",
        ),
        (
            json!({ "model": "pk1.iv_bolus", "dose": 10, "params": { "v": 5, "k": 0.2 },
                    "grid": { "start": 3, "end": 1, "points": 3 } }),
            "invalid_parameters",
            "start",
        ),
    ] {
        let e = err(&mut e, "model.simulate", params);
        assert_eq!(e.code, code, "{e}");
        assert!(e.message.contains(text), "{e}");
    }
}

#[test]
fn tables_are_exported_as_csv_and_json() {
    let mut e = engine_with_two_subjects();
    ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    let nca = ok(
        &mut e,
        "export.table",
        json!({ "table": "nca.parameters", "analysis": 2 }),
    );
    assert_eq!(
        nca["file_name"],
        "nca-of-study-all-subjects-nca-parameters.csv"
    );
    assert_eq!(
        nca["columns"],
        json!(["subject", "parameter", "value", "not_calculated_reason"])
    );
    let csv = nca["csv"].as_str().unwrap();
    assert!(csv.starts_with("subject,parameter,value,not_calculated_reason\nA,"));
    assert!(csv.lines().any(|l| l.starts_with("B,cmax,")));
    // Not-calculated parameters carry a reason, not a number: C0 does not exist for oral dosing.
    assert!(csv.lines().any(|l| l.starts_with("A,c0,,")), "{csv}");
    assert_eq!(nca["status"], "fresh");

    ok(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "subject": "A", "model": "pk1.oral_1" }),
    );
    for (table, first_column) in [
        ("fit.parameters", "parameter"),
        ("fit.values", "name"),
        ("fit.observations", "time"),
        ("fit.curve", "time"),
        ("fit.trace", "iteration"),
    ] {
        let t = ok(
            &mut e,
            "export.table",
            json!({ "table": table, "analysis": 3 }),
        );
        assert_eq!(t["columns"][0], first_column, "{table}");
        assert!(!t["rows"].as_array().unwrap().is_empty(), "{table}");
        assert_eq!(
            t["csv"].as_str().unwrap().lines().count(),
            t["rows"].as_array().unwrap().len() + 1,
            "{table}"
        );
    }
    let params = ok(
        &mut e,
        "export.table",
        json!({ "table": "fit.parameters", "analysis": 3 }),
    );
    assert_eq!(params["rows"].as_array().unwrap().len(), 3);

    let sheet = ok(
        &mut e,
        "export.table",
        json!({ "table": "worksheet", "worksheet": 1 }),
    );
    assert_eq!(sheet["columns"], json!(["Subject", "Time", "Conc", "Dose"]));
    assert_eq!(sheet["rows"].as_array().unwrap().len(), 18);
    // A missing value is an empty CSV field.
    assert!(sheet["csv"].as_str().unwrap().contains("B,8,,100\n"));

    for (params, code) in [
        (
            json!({ "table": "fit.curve", "analysis": 2 }),
            "wrong_table_for_analysis",
        ),
        (json!({ "table": "nca.parameters" }), "invalid_parameters"),
        (json!({ "table": "worksheet" }), "invalid_parameters"),
        (
            json!({ "table": "nope", "analysis": 2 }),
            "invalid_parameters",
        ),
        (
            json!({ "table": "nca.parameters", "analysis": 99 }),
            "unknown_analysis",
        ),
    ] {
        assert_eq!(err(&mut e, "export.table", params).code, code);
    }
}

#[test]
fn an_analysis_that_never_ran_has_no_tables() {
    let mut e = engine_with_two_subjects();
    // Build an analysis without a result through the project, then ask for a table.
    let mut project = e.project().clone();
    let id = project
        .add_analysis(
            project::AnalysisSpec::Nca(project::NcaSpec {
                worksheet: project::WorksheetId(1),
                subject: None,
                route: None,
                dose: None,
                options: Default::default(),
            }),
            None,
        )
        .unwrap();
    e = Engine::with_project(project);
    let got = ok(&mut e, "analysis.get", json!({ "analysis": id }));
    assert_eq!(got["status"], json!({ "state": "no_result" }));
    assert_eq!(got["result"], Value::Null);
    let t = err(
        &mut e,
        "export.table",
        json!({ "table": "nca.parameters", "analysis": id }),
    );
    assert_eq!(t.code, "no_result");
}

// ---- save and load -----------------------------------------------------------------------

#[test]
fn a_project_survives_save_and_load_with_its_staleness() {
    let mut e = engine_with_two_subjects();
    ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    ok(
        &mut e,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": 3.0 }),
    );
    let bytes = e.save_bytes().unwrap();

    let mut other = Engine::new();
    other.load_bytes(&bytes).unwrap();
    assert_eq!(other.project(), e.project());
    let got = ok(&mut other, "analysis.get", json!({ "analysis": 2 }));
    assert_eq!(got["status"]["state"], "stale");
    // The same through the document form of the commands.
    let saved = ok(&mut e, "project.save", Value::Null);
    assert_eq!(saved["file_name"], "untitled-project.caladrius.json");
    let mut third = Engine::new();
    ok(
        &mut third,
        "project.load",
        json!({ "project": saved["project"] }),
    );
    assert_eq!(third.project(), e.project());
}

#[test]
fn the_saved_project_carries_its_history() {
    let mut e = engine_with_two_subjects();
    ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    let _ = e.execute("nca.run", json!({ "worksheet": 9 }));
    let bytes = e.save_bytes().unwrap();
    // The document is the project plus its history, the save included, deterministic.
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let saved = doc["history"].as_array().unwrap();
    assert_eq!(saved.len(), 4);
    assert_eq!(saved[3]["command"], "project.save");
    assert_eq!(saved[2]["ok"], false);

    // A fresh session adopts it and carries on from it.
    let mut fresh = Engine::new();
    let r = fresh.load_bytes(&bytes).unwrap();
    assert_eq!(r["history_restored"], true);
    assert_eq!(fresh.history().len(), 5);
    assert_eq!(fresh.history().entries()[..4], e.history().entries()[..4]);
    assert_eq!(fresh.history().entries()[4].command, "project.load");
    assert_eq!(fresh.project(), e.project());
    ok(&mut fresh, "project.describe", Value::Null);
    assert_eq!(fresh.history().entries().last().map(|h| h.seq), Some(6));

    // A session that already has a history keeps it.
    let mut busy = Engine::new();
    ok(&mut busy, "project.describe", Value::Null);
    let r = busy.load_bytes(&bytes).unwrap();
    assert_eq!(r["history_restored"], false);
    assert_eq!(busy.history().len(), 2);
    assert_eq!(busy.project(), e.project());

    // A damaged history is left out; the project loads and the result says so.
    let mut damaged = doc.clone();
    damaged["history"][2]["seq"] = json!(9);
    let mut target = Engine::new();
    let r = ok(&mut target, "project.load", json!({ "project": damaged }));
    assert_eq!(r["history_restored"], false);
    assert!(
        r["history_note"].as_str().unwrap().contains("position 9"),
        "{r}"
    );
    assert_eq!(target.project(), e.project());
    assert_eq!(target.history().len(), 1);
    let mut unreadable = doc.clone();
    unreadable["history"] = json!("nonsense");
    let mut target = Engine::new();
    let r = ok(
        &mut target,
        "project.load",
        json!({ "project": unreadable }),
    );
    assert_eq!(r["history_restored"], false);
    assert!(
        r["history_note"].as_str().unwrap().contains("not readable"),
        "{r}"
    );
    // A file without history (older, or written by hand) loads with an empty one.
    let mut plain = doc;
    plain.as_object_mut().unwrap().remove("history");
    let mut other = Engine::new();
    let r = ok(&mut other, "project.load", json!({ "project": plain }));
    assert_eq!(r["history_restored"], true);
    assert_eq!(other.history().len(), 1);
}

#[test]
fn large_parameters_are_kept_as_a_digest() {
    let mut e = Engine::new();
    let small = json!({ "name": "small", "csv": "time,conc\n0,1\n1,2\n" });
    ok(&mut e, "data.import", small.clone());
    assert_eq!(e.history().entries()[0].params, small);

    let mut csv = String::from("time,conc\n");
    let mut i = 0;
    while csv.len() <= MAX_RECORDED_PARAMS_BYTES {
        csv.push_str(&format!("{i},{}\n", 1 + i % 7));
        i += 1;
    }
    ok(&mut e, "data.import", json!({ "name": "big", "csv": csv }));
    let kept = e.history().entries()[1].params.clone();
    assert_eq!(kept["omitted"], true);
    assert!(kept["bytes"].as_u64().unwrap() > MAX_RECORDED_PARAMS_BYTES as u64);
    let digest = kept["digest"].as_str().unwrap();
    assert!(digest.starts_with("fnv1a64:") && digest.len() == "fnv1a64:".len() + 16);
    // The same input gives the same digest, another input another one.
    ok(&mut e, "data.import", json!({ "name": "big", "csv": csv }));
    assert_eq!(e.history().entries()[2].params, kept);
    let other = format!("{csv}0,0\n");
    ok(
        &mut e,
        "data.import",
        json!({ "name": "big", "csv": other }),
    );
    assert_ne!(e.history().entries()[3].params["digest"], kept["digest"]);
    // The history still lists it, and the entry is small.
    let listed = ok(&mut e, "history.list", json!({ "from": 2, "limit": 1 }));
    assert!(listed.to_string().len() < 1000);
}

#[test]
fn worksheets_and_analyses_can_be_removed() {
    let mut e = engine_with_two_subjects();
    ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    ok(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "subject": "A", "model": "pk1.oral_1" }),
    );
    // A worksheet that analyses read is refused, naming them, and nothing changes.
    let before = e.project().clone();
    let refused = err(&mut e, "data.remove", json!({ "worksheet": 1 }));
    assert_eq!(refused.code, "worksheet_in_use");
    assert!(refused.message.contains("NCA of study"), "{refused}");
    assert_eq!(e.project(), &before);
    // An analysis goes alone.
    let r = ok(&mut e, "analysis.remove", json!({ "analysis": 3 }));
    assert_eq!(r["removed"], json!({ "analyses": [3] }));
    assert_eq!(r["analyses"].as_array().unwrap().len(), 1);
    assert_eq!(
        err(&mut e, "analysis.get", json!({ "analysis": 3 })).code,
        "unknown_analysis"
    );
    assert_eq!(
        err(&mut e, "analysis.remove", json!({ "analysis": 3 })).code,
        "unknown_analysis"
    );
    // With `remove_analyses` the worksheet goes with the analyses that read it.
    let r = ok(
        &mut e,
        "data.remove",
        json!({ "worksheet": 1, "remove_analyses": true }),
    );
    assert_eq!(r["removed"], json!({ "worksheet": 1, "analyses": [2] }));
    assert!(e.project().worksheets().is_empty() && e.project().analyses().is_empty());
    assert_eq!(
        err(&mut e, "data.remove", json!({ "worksheet": 1 })).code,
        "unknown_worksheet"
    );
    // An unused worksheet goes without the flag.
    let second = ok(
        &mut e,
        "data.import",
        json!({ "name": "w", "csv": "time,conc\n0,1\n" }),
    );
    let id = second["worksheet"]["id"].clone();
    let r = ok(&mut e, "data.remove", json!({ "worksheet": id }));
    assert_eq!(r["removed"]["analyses"], json!([]));
}

#[test]
fn loading_a_bad_file_changes_nothing() {
    let mut e = engine_with_two_subjects();
    let before = e.project().clone();
    for params in [
        json!({ "text": "{" }),
        json!({ "text": "[]" }),
        json!({ "project": { "format": "x" } }),
        json!({}),
        json!({ "project": {}, "text": "{}" }),
    ] {
        let failure = err(&mut e, "project.load", params);
        assert!(
            ["load_failed", "invalid_parameters"].contains(&failure.code.as_str()),
            "{failure}"
        );
        assert_eq!(e.project(), &before);
    }
    assert_eq!(e.load_bytes(&[0xff]).unwrap_err().code, "load_failed");
    assert_eq!(e.project(), &before);
}

#[test]
fn a_new_project_is_empty_and_the_history_stays() {
    let mut e = engine_with_two_subjects();
    let n = e.history().len();
    let r = ok(&mut e, "project.new", json!({ "name": "Fresh" }));
    assert_eq!(r["name"], "Fresh");
    assert!(e.project().worksheets().is_empty());
    assert_eq!(e.history().len(), n + 1);
}

// ---- history -----------------------------------------------------------------------------

#[test]
fn the_history_is_append_only_and_records_failures() {
    let mut e = Engine::new();
    let mut snapshots: Vec<Vec<HistoryEntry>> = Vec::new();
    let script: [(&str, Value); 6] = [
        (
            "data.import",
            json!({ "name": "w", "csv": "time,conc\n0,0\n1,5\n" }),
        ),
        ("nca.run", json!({ "worksheet": 9 })),
        ("no.such", json!({})),
        ("data.describe", json!({ "worksheet": 1 })),
        (
            "data.set_cell",
            json!({ "worksheet": 1, "column": "conc", "row": 1, "value": 6 }),
        ),
        ("history.list", json!({})),
    ];
    for (i, (id, params)) in script.iter().enumerate() {
        let _ = e.execute(id, params.clone());
        let now = e.history().entries().to_vec();
        assert_eq!(now.len(), i + 1, "{id}");
        // Everything recorded before is unchanged.
        if let Some(prev) = snapshots.last() {
            assert_eq!(&now[..prev.len()], prev.as_slice(), "{id}");
        }
        snapshots.push(now);
    }
    let h = e.history().entries();
    let seqs: Vec<u64> = h.iter().map(|x| x.seq).collect();
    assert_eq!(seqs, [1, 2, 3, 4, 5, 6]);
    let flags: Vec<(bool, bool)> = h.iter().map(|x| (x.ok, x.changed_project)).collect();
    assert_eq!(
        flags,
        [
            (true, true),
            (false, false),
            (false, false),
            (true, false),
            (true, true),
            (true, false)
        ]
    );
    assert_eq!(h[1].error.as_ref().unwrap().code, "unknown_worksheet");
    assert_eq!(h[2].error.as_ref().unwrap().code, "unknown_command");
    assert_eq!(h[4].params["value"], 6);
    // Reading the history is itself recorded, after the fact, and shows what came before.
    let listed = ok(&mut e, "history.list", json!({ "from": 2, "limit": 3 }));
    assert_eq!(listed["total"], 6);
    let commands: Vec<&str> = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands, ["nca.run", "no.such", "data.describe"]);
    assert_eq!(e.history().len(), 7);
}

#[test]
fn text_parameters_go_through_the_same_path() {
    let mut e = Engine::new();
    let r = e
        .execute_text(
            "data.import",
            r#"{"name":"w","csv":"time,conc\n0,0\n1,5\n"}"#,
        )
        .unwrap();
    assert_eq!(r["worksheet"]["rows"], 2);
    let bad = e.execute_text("data.describe", "{not json").unwrap_err();
    assert_eq!(bad.code, "invalid_parameters");
    assert!(bad.message.contains("not valid JSON"));
    assert_eq!(e.history().len(), 2);
    assert!(!e.history().entries()[1].ok);
    // Empty text means no parameters.
    assert!(e.execute_text("project.describe", "").is_ok());
}

#[test]
fn csv_cells_are_quoted_when_needed() {
    let mut e = Engine::new();
    ok(
        &mut e,
        "data.import",
        json!({ "name": "q", "csv": "id,time,conc\n\"a,b\",0,1\n\"say \"\"hi\"\"\",1,2\n" }),
    );
    let t = ok(
        &mut e,
        "export.table",
        json!({ "table": "worksheet", "worksheet": 1 }),
    );
    assert_eq!(
        t["csv"],
        "id,time,conc\n\"a,b\",0,1\n\"say \"\"hi\"\"\",1,2\n"
    );
}

#[test]
fn a_subject_can_be_given_as_a_number() {
    let mut e = Engine::new();
    ok(
        &mut e,
        "data.import",
        json!({ "name": "n", "csv": "id,time,conc,dose\n1,0,0,10\n1,1,5,10\n1,2,3,10\n1,4,1,10\n2,0,0,10\n2,1,4,10\n2,2,2,10\n2,4,1,10\n" }),
    );
    let r = ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular", "subject": 2 }),
    );
    assert_eq!(r["result"]["subjects"][0]["subject"], "2");
    let r = ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular", "subject": "1" }),
    );
    assert_eq!(r["result"]["subjects"][0]["subject"], "1");
    let bad = err(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular", "subject": [1] }),
    );
    assert_eq!(bad.code, "invalid_parameters");
}

#[test]
fn an_unknown_column_is_reported_as_such_whatever_the_value() {
    let mut e = engine_with_two_subjects();
    for value in [json!(1), json!("x"), json!(null)] {
        let failure = err(
            &mut e,
            "data.set_cell",
            json!({ "worksheet": 1, "column": "nope", "row": 0, "value": value }),
        );
        assert_eq!(failure.code, "unknown_column");
        assert!(
            failure.message.contains("Subject, Time, Conc, Dose"),
            "{failure}"
        );
    }
}

#[test]
fn a_project_with_a_counter_at_the_limit_gives_errors_through_the_commands() {
    let mut e = engine_with_two_subjects();
    ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    let saved = ok(&mut e, "project.save", Value::Null)["project"].clone();
    let with = |pointer: &str, value: u64| {
        let mut doc = saved.clone();
        *doc.pointer_mut(pointer).unwrap() = json!(value);
        doc
    };
    // At the limit the file is refused; nothing is loaded.
    for pointer in [
        "/next_id",
        "/worksheets/0/revision",
        "/analyses/0/spec_version",
    ] {
        let mut fresh = Engine::new();
        let failure = err(
            &mut fresh,
            "project.load",
            json!({ "project": with(pointer, u64::MAX) }),
        );
        assert_eq!(failure.code, "load_bad_counter", "{pointer}");
        assert!(fresh.project().worksheets().is_empty());
    }
    // One below it, the commands that would pass the limit fail with a message, and change nothing.
    let mut near = Engine::new();
    ok(
        &mut near,
        "project.load",
        json!({ "project": with("/next_id", u64::MAX - 1) }),
    );
    let before = near.project().clone();
    let failure = err(
        &mut near,
        "data.import",
        json!({ "name": "x", "csv": "time,conc\n0,1\n" }),
    );
    assert_eq!(failure.code, "counter_overflow");
    assert_eq!(near.project(), &before);

    let mut near = Engine::new();
    ok(
        &mut near,
        "project.load",
        json!({ "project": with("/worksheets/0/revision", u64::MAX - 1) }),
    );
    let before = near.project().clone();
    let failure = err(
        &mut near,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": 9.0 }),
    );
    assert_eq!(failure.code, "counter_overflow");
    assert_eq!(near.project(), &before);
}

// ---- the schemas against the serde types, the other way round -------------------------------

/// Values a schema allows at one place, one per alternative: each enum value, each `oneOf` branch,
/// each property of an object (set on a base value), booleans both ways, a number, a short text.
fn variants(root: &Value, schema: &Value, base: &Value) -> Vec<Value> {
    if let Some(Value::String(r)) = schema.get("$ref") {
        let name = r.strip_prefix("#/$defs/").unwrap();
        let target = root.pointer(&format!("/$defs/{name}")).unwrap();
        return variants(root, target, base);
    }
    if let Some(Value::Array(values)) = schema.get("enum") {
        return values.clone();
    }
    if let Some(c) = schema.get("const") {
        return vec![c.clone()];
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(branches)) = schema.get(key) {
            return branches
                .iter()
                .flat_map(|b| variants(root, b, &Value::Null))
                .collect();
        }
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("null") => vec![Value::Null],
        Some("boolean") => vec![json!(true), json!(false)],
        Some("number" | "integer") => vec![json!(1)],
        Some("string") => vec![json!("x")],
        Some("array") => vec![json!([])],
        Some("object") => {
            let Some(Value::Object(props)) = schema.get("properties") else {
                return vec![base.clone()];
            };
            // The base, completed with the required properties it lacks.
            let mut start = match base {
                Value::Object(m) => m.clone(),
                _ => serde_json::Map::new(),
            };
            if let Some(Value::Array(required)) = schema.get("required") {
                for name in required.iter().filter_map(Value::as_str) {
                    if !start.contains_key(name) {
                        if let Some(first) = props
                            .get(name)
                            .and_then(|sub| variants(root, sub, &Value::Null).into_iter().next())
                        {
                            start.insert(name.to_owned(), first);
                        }
                    }
                }
            }
            let base = &Value::Object(start);
            let mut out = Vec::new();
            for (name, sub) in props {
                let current = base.get(name).cloned().unwrap_or(Value::Null);
                // A base that is not an object (a unit variant written as text) starts empty.
                for v in variants(root, sub, &current) {
                    let mut object = match base {
                        Value::Object(m) => m.clone(),
                        _ => serde_json::Map::new(),
                    };
                    object.insert(name.clone(), v);
                    out.push(Value::Object(object));
                }
            }
            out
        }
        _ => vec![base.clone()],
    }
}

#[test]
fn everything_the_option_schemas_allow_is_accepted_by_the_serde_types() {
    let nca = info("nca.run").params_schema;
    let nca_options = &nca["$defs"]["NcaOptions"];
    let base = serde_json::to_value(caladrius_nca::NcaOptions::default()).unwrap();
    let all = variants(&nca, nca_options, &base);
    assert!(all.len() > 25, "{}", all.len());
    for v in all {
        if let Err(e) = serde_json::from_value::<caladrius_nca::NcaOptions>(v.clone()) {
            panic!("the schema allows {v} but serde refuses it: {e}");
        }
    }
    let fit = info("fit.run").params_schema;
    let base = serde_json::to_value(caladrius_fit::FitOptions::default()).unwrap();
    let all = variants(&fit, &fit["$defs"]["FitOptions"], &base);
    assert!(all.len() > 15, "{}", all.len());
    for v in all {
        if let Err(e) = serde_json::from_value::<caladrius_fit::FitOptions>(v.clone()) {
            panic!("the schema allows {v} but serde refuses it: {e}");
        }
    }
    // The route and the model and weighting ids.
    let route = variants(&nca, &nca["$defs"]["Route"], &Value::Null);
    assert_eq!(route.len(), 3);
    for v in route {
        serde_json::from_value::<caladrius_nca::Route>(v.clone())
            .unwrap_or_else(|e| panic!("{v}: {e}"));
    }
    for (def, schema) in [("ModelId", &fit), ("Weighting", &fit)] {
        for v in variants(schema, &schema["$defs"][def], &Value::Null) {
            match def {
                "ModelId" => drop(serde_json::from_value::<caladrius_models::ModelId>(v).unwrap()),
                _ => drop(serde_json::from_value::<caladrius_fit::Weighting>(v).unwrap()),
            }
        }
    }
}

#[test]
fn every_property_and_value_of_every_parameter_schema_is_known_to_the_command() {
    // Run the examples in order; before each command, try the example with each schema property
    // set to each of its alternatives. The command may refuse the value for its own reasons, but
    // never because it does not know the property or the variant.
    let mut engine = Engine::new();
    let mut tried = 0;
    for c in describe() {
        let Some(Value::Object(props)) = c.params_schema.get("properties") else {
            continue;
        };
        for (name, sub) in props {
            let current = c.example.get(name).cloned().unwrap_or(Value::Null);
            for v in variants(&c.params_schema, sub, &current) {
                let mut params = c.example.clone();
                params[name] = v.clone();
                let mut trial = engine.clone();
                if let Err(e) = trial.execute(&c.id, params.clone()) {
                    assert!(
                        !(e.code == "invalid_parameters"
                            && (e.message.contains("unknown field")
                                || e.message.contains("unknown variant"))),
                        "{}: the schema allows {name} = {v} but the command does not know it: {e}",
                        c.id
                    );
                }
                tried += 1;
            }
        }
        engine
            .execute(&c.id, c.example.clone())
            .unwrap_or_else(|e| panic!("example of {} failed: {e}", c.id));
    }
    assert!(tried > 150, "{tried}");
}

#[test]
fn a_whole_number_subject_has_one_label_whatever_its_spelling() {
    let mut e = Engine::new();
    ok(
        &mut e,
        "data.import",
        json!({ "name": "n", "csv": "id,time,conc,dose\n1,0,0,10\n1,1,5,10\n1,2,3,10\n1,4,1,10\n" }),
    );
    for subject in [json!(1), json!(1.0), json!("1")] {
        let r = ok(
            &mut e,
            "nca.run",
            json!({ "worksheet": 1, "route": "extravascular", "subject": subject }),
        );
        assert_eq!(r["result"]["subjects"][0]["subject"], "1");
    }
}

#[test]
fn a_preview_shows_the_readings_and_stores_nothing() {
    let mut e = Engine::new();
    let r = ok(
        &mut e,
        "data.preview",
        json!({ "csv": "time;conc\n0,25;3,4\n0,5;5,1\n" }),
    );
    assert_eq!(r["needs_choice"], false);
    let best = &r["readings"][0];
    assert_eq!(best["delimiter"], ";");
    assert_eq!(best["decimal_comma"], true);
    assert_eq!(best["preview"][0], json!([0.25, 3.4]));
    // The reported failure: a file whose comma-separated reading repeats the time 0.
    let r = ok(
        &mut e,
        "data.preview",
        json!({ "csv": "time,conc\n0,25\n0,5\n1,2\n" }),
    );
    assert_eq!(r["needs_choice"], true);
    assert_eq!(r["readings"][0]["checks"][0]["code"], "duplicate_times");
    // Nothing was imported, and the history says so.
    assert!(e.project().worksheets().is_empty());
    assert!(e.history().entries().iter().all(|h| !h.changed_project));
    // A file that cannot be read says why.
    let failure = err(&mut e, "data.preview", json!({ "csv": "a,b\n1,2\n" }));
    assert!(
        failure.message.contains("time") || failure.message.contains("no reading"),
        "{failure}"
    );
}

#[test]
fn flags_come_with_their_sentence() {
    let mut e = Engine::new();
    // Three points in the terminal phase over a short span: the span flag is raised.
    ok(
        &mut e,
        "data.import",
        json!({ "name": "w", "csv": "time,conc,dose\n0,0,10\n1,8,10\n2,6,10\n3,4.5,10\n4,3.4,10\n" }),
    );
    let r = ok(
        &mut e,
        "nca.run",
        json!({ "worksheet": 1, "route": "extravascular" }),
    );
    let s = &r["result"]["subjects"][0];
    let flags = s["outcome"]["ok"]["flags"].as_array().unwrap();
    let messages = s["flag_messages"].as_array().unwrap();
    assert!(!flags.is_empty());
    assert_eq!(flags.len(), messages.len());
    assert!(
        messages
            .iter()
            .all(|m| m.as_str().is_some_and(|t| t.len() > 20))
    );
    let f = ok(
        &mut e,
        "data.import",
        json!({ "name": "f", "csv": "time,conc,dose\n0,0,100\n0.25,1.279,100\n0.5,2.195,100\n1,3.293,100\n2,3.971,100\n4,3.611,100\n6,2.989,100\n8,2.451,100\n12,1.643,100\n24,0.495,100\n" }),
    );
    let id = f["worksheet"]["id"].clone();
    let fit = ok(
        &mut e,
        "fit.run",
        json!({ "worksheet": id, "model": "pk1.oral_1" }),
    );
    assert!(fit["result"]["flag_messages"].is_array());
}

#[test]
fn a_cell_typed_with_either_decimal_mark_is_understood_and_the_answer_says_which() {
    let mut e = engine_with_two_subjects();
    for (typed, number) in [
        ("3,25", json!(3.25)),
        ("3.25", json!(3.25)),
        (" 7 ", json!(7.0)),
        ("", Value::Null),
        ("NA", Value::Null),
    ] {
        let r = ok(
            &mut e,
            "data.set_cell",
            json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": typed }),
        );
        assert_eq!(r["understood"], number, "{typed:?}");
    }
    let before = e.project().clone();
    for typed in ["1,234.5", "1.2.3", "abc", "1e999", "12 5"] {
        let failure = err(
            &mut e,
            "data.set_cell",
            json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": typed }),
        );
        assert_eq!(failure.code, "invalid_number", "{typed}");
        assert!(failure.message.contains("Conc"), "{failure}");
    }
    assert_eq!(e.project(), &before, "a refused entry changes nothing");
}

#[test]
fn a_number_that_could_have_a_thousands_separator_is_refused_not_guessed() {
    let mut e = engine_with_two_subjects();
    for typed in [
        "1,500", "1.500", "12,345", "12.345", "-1,500", "+999.999", "123,456",
    ] {
        let failure = err(
            &mut e,
            "data.set_cell",
            json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": typed }),
        );
        assert_eq!(failure.code, "invalid_number", "{typed}");
        assert!(failure.message.contains("thousands separator"), "{failure}");
        assert!(failure.message.contains("Conc"), "{failure}");
    }
    // Both ways of writing it are offered in the message.
    let failure = err(
        &mut e,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": "1,500" }),
    );
    assert!(
        failure.message.contains("1500") && failure.message.contains("1.5000"),
        "{failure}"
    );
    // What is plainly a decimal is read as one, with either mark.
    for (typed, number) in [
        ("1,5", 1.5),
        ("1.5", 1.5),
        ("0.500", 0.5),
        ("0,250", 0.25),
        ("1234,567", 1234.567),
        ("1234.567", 1234.567),
        ("1.50", 1.5),
        ("12,3456", 12.3456),
        ("1500", 1500.0),
        ("1.5000", 1.5),
    ] {
        let r = ok(
            &mut e,
            "data.set_cell",
            json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": typed }),
        );
        assert_eq!(r["understood"], json!(number), "{typed}");
    }
    // More than one mark is refused as before.
    let failure = err(
        &mut e,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Conc", "row": 2, "value": "12,345,678" }),
    );
    assert!(
        failure.message.contains("more than one decimal mark"),
        "{failure}"
    );
}
