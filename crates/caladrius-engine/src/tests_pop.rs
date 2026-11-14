//! `pop.fit`: the population fit (`specs/population.md`) of every subject of a worksheet.
//!
//! The reference is the population fit called directly on the same file, read by its own reader
//! (`dataset_text`, POP-API-01 item 12): the engine reads the subjects through the worksheet, so the
//! two routes share only the numerical code, and their answers must be the same to the bit.

use std::path::PathBuf;

use caladrius_fit::pop::PopRequest;
use serde_json::{Value, json};

use crate::replay::{Report, Verdict};
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

fn theoph_csv() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/data/population/event_theoph.csv");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The request of the oracle's Theoph case `theoph_additive` (`oracle/expected/pop/ofv/`, the
/// second look of `oracle/expected/pop/second_look/nlme.json`): `pk1.oral_1`, a random effect on
/// cl, v and ka, a diagonal omega, the additive error model. The oracle leaves out the samples at
/// time 0; the file has them, and the additive model keeps them (132 observations).
fn theoph_request() -> Value {
    json!({
        "model": "pk1.oral_1",
        "initial": { "v": 32.0, "cl": 2.8, "ka": 1.5 },
        "random_effects": ["cl", "v", "ka"],
        "omega": [[0.09, 0.0, 0.0], [0.0, 0.04, 0.0], [0.0, 0.0, 0.25]],
        "omega_shape": "diagonal",
        "error_model": "additive",
        "error_init": [0.7],
    })
}

/// `a` with the keys of `b` set.
fn with(mut a: Value, b: Value) -> Value {
    if let (Some(map), Value::Object(extra)) = (a.as_object_mut(), b) {
        map.extend(extra);
    }
    a
}

fn without(mut a: Value, keys: &[&str]) -> Value {
    if let Some(map) = a.as_object_mut() {
        for k in keys {
            map.remove(*k);
        }
    }
    a
}

fn engine_with(csv: &str) -> Engine {
    let mut engine = Engine::new();
    ok(
        &mut engine,
        "data.import",
        json!({ "name": "theoph", "csv": csv }),
    );
    engine
}

fn result_schema(id: &str) -> Value {
    describe()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.result_schema)
        .unwrap()
}

fn assert_valid(id: &str, instance: &Value) {
    let schema = result_schema(id);
    if let Err(why) = validate::check(&schema, &schema, instance) {
        panic!("{id}: {why}");
    }
}

/// The same JSON, every number the same double to the last bit.
fn same_bits(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert_eq!(x.to_bits(), y.to_bits(), "{path}: {x:e} and {y:e}");
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{path}: lengths");
            for (i, (u, v)) in x.iter().zip(y).enumerate() {
                same_bits(u, v, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            let keys = |m: &serde_json::Map<String, Value>| m.keys().cloned().collect::<Vec<_>>();
            assert_eq!(keys(x), keys(y), "{path}: keys");
            for (k, u) in x {
                same_bits(u, &y[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

/// The population fit called directly on the file, read by its own table reader.
fn direct(csv: &str, request: Value) -> Value {
    let mut request: PopRequest =
        serde_json::from_value(with(request, json!({ "dataset_file": "event_theoph.csv" })))
            .unwrap();
    request.dataset_text = Some(csv.to_owned());
    let answer = caladrius_fit::pop::run(&request).unwrap_or_else(|e| panic!("{e}"));
    serde_json::to_value(&answer).unwrap()
}

fn replay_of(document: &Value) -> Report {
    let mut verifier = Engine::new();
    let answer = ok(
        &mut verifier,
        "project.replay",
        json!({ "text": document.to_string() }),
    );
    serde_json::from_value(answer).unwrap()
}

#[test]
fn the_population_fit_of_theoph_is_the_population_fit_called_directly_to_the_bit() {
    let csv = theoph_csv();
    let mut engine = engine_with(&csv);
    let started = std::time::Instant::now();
    let answer = ok(
        &mut engine,
        "pop.fit",
        with(theoph_request(), json!({ "worksheet": 1 })),
    );
    // Printed for the record (`--nocapture`), never in an answer.
    eprintln!(
        "pop.fit, Theoph, FOCEI, 12 subjects: {:.3} s",
        started.elapsed().as_secs_f64()
    );
    assert_valid("pop.fit", &answer);
    assert_eq!(answer["status"], "converged", "{}", answer["status"]);
    assert_eq!(answer["method"], "foce_i");
    assert_eq!(answer["n_subjects"], 12);
    assert_eq!(answer["n_observations"], 132);
    assert!(answer.get("refused_subjects").is_none() && answer.get("notes").is_none());

    // The same subjects through the population fit's own reader: the same answer, bit for bit.
    let reference = direct(&csv, theoph_request());
    same_bits(&answer["ofv"], &reference["ofv"], "ofv");
    same_bits(&answer["estimate"], &reference["estimate"], "estimate");
    same_bits(
        &without(answer.clone(), &["analysis"]),
        &reference,
        "answer",
    );

    // The diagnostics of every observation: one row per observation used, and the ε-shrinkage is
    // 1 − SD(IWRES) over them (POP-DIA-03, divisor N − 1).
    let subjects = answer["subjects"].as_array().unwrap();
    let mut iwres = Vec::new();
    for s in subjects {
        let rows = s["observations"].as_array().unwrap();
        assert_eq!(rows.len() as u64, s["n_observations"].as_u64().unwrap());
        for r in rows {
            let (y, ipred) = (r["y"].as_f64().unwrap(), r["ipred"].as_f64().unwrap());
            assert!(r["pred"].as_f64().is_some_and(f64::is_finite));
            let w = r["iwres"].as_f64().unwrap();
            assert_eq!(w.signum(), (y - ipred).signum(), "{r}");
            iwres.push(w);
        }
    }
    let n = iwres.len() as f64;
    let mean = iwres.iter().sum::<f64>() / n;
    let sd = (iwres.iter().map(|w| (w - mean) * (w - mean)).sum::<f64>() / (n - 1.0)).sqrt();
    let eps = answer["eps_shrinkage"].as_f64().unwrap();
    assert!((eps - (1.0 - sd)).abs() < 1e-12, "{eps} and {}", 1.0 - sd);

    // Stored as an analysis object: `analysis.get` shows the same result, the history has the
    // entry with its seal.
    let id = answer["analysis"].clone();
    let view = ok(&mut engine, "analysis.get", json!({ "analysis": id }));
    assert_valid("analysis.get", &view);
    assert_eq!(view["kind"], "pop_fit");
    assert_eq!(view["status"]["state"], "fresh");
    assert_eq!(view["label"], "Population fit pk1.oral_1 to theoph");
    same_bits(
        &without(view["result"].clone(), &["kind"]),
        &without(answer.clone(), &["analysis"]),
        "result",
    );
    let entry = engine.history().entries().iter().rev().nth(1).unwrap();
    assert_eq!(entry.command, "pop.fit");
    assert!(entry.ok && entry.changed_project && entry.seal.is_some());

    // Saved and loaded, the result reads back the same; replayed, every seal is reproduced.
    let bytes = engine.save_bytes().unwrap();
    let mut loaded = Engine::new();
    loaded.load_bytes(&bytes).unwrap();
    let again = ok(&mut loaded, "analysis.get", json!({ "analysis": id }));
    same_bits(&again["result"], &view["result"], "loaded");
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    let report = replay_of(&document);
    assert_eq!(report.verdict, Verdict::Verified, "{}", report.summary);
}

#[test]
fn a_request_the_population_fit_refuses_is_an_error_with_a_sentence_and_stores_nothing() {
    let mut engine = engine_with(&theoph_csv());
    let project = engine.project().clone();
    let revision = engine.revision();
    // No error model: there is no default.
    let e = err(
        &mut engine,
        "pop.fit",
        without(
            with(theoph_request(), json!({ "worksheet": 1 })),
            &["error_model"],
        ),
    );
    assert_eq!(e.code, "invalid_option", "{e}");
    assert!(
        e.message.contains("error_model")
            && e.message.contains("no error model is given")
            && e.message.contains("additive"),
        "{e}"
    );
    // The proportional model on the file as imported: `pk1.oral_1` predicts 0 at the samples of
    // time 0, where a proportional error has no standard deviation.
    let e = err(
        &mut engine,
        "pop.fit",
        with(
            theoph_request(),
            json!({ "worksheet": 1, "error_model": "proportional", "error_init": [0.12] }),
        ),
    );
    assert_eq!(e.code, "non_positive_sigma", "{e}");
    assert!(e.message.contains("at time 0 of subject 1"), "{e}");
    // A model the population fit does not know is refused by the parameters.
    let e = err(
        &mut engine,
        "pop.fit",
        with(
            theoph_request(),
            json!({ "worksheet": 1, "model": "identity_linear" }),
        ),
    );
    assert_eq!(e.code, "invalid_parameters", "{e}");
    assert_eq!(engine.project(), &project);
    assert_eq!(engine.revision(), revision);
    let entries = engine.history().entries();
    assert_eq!(entries.len(), 4);
    assert!(
        entries
            .iter()
            .skip(1)
            .all(|h| h.command == "pop.fit" && !h.ok && !h.changed_project && h.seal.is_some())
    );
}

/// The Theoph file with a second dose row for subject 12, which the checks of the data refuse.
fn theoph_with_two_doses_for_subject_12() -> String {
    let mut csv = theoph_csv();
    if !csv.ends_with('\n') {
        csv.push('\n');
    }
    // After its last sample (24.15 h), so that the rows stay in time order.
    csv.push_str("12,25,,320,1,60.5\n");
    csv
}

/// At the given parameters (`evaluate_only`), so that the test is quick.
fn evaluation() -> Value {
    with(
        theoph_request(),
        json!({ "worksheet": 1, "options": { "evaluate_only": true } }),
    )
}

#[test]
fn a_subject_the_data_check_refuses_is_left_out_and_listed_and_the_others_run() {
    let csv = theoph_with_two_doses_for_subject_12();
    let mut engine = engine_with(&csv);
    let answer = ok(&mut engine, "pop.fit", evaluation());
    assert_valid("pop.fit", &answer);
    assert_eq!(answer["status"], "evaluated");
    assert_eq!(answer["n_subjects"], 11);
    let refused = answer["refused_subjects"].as_array().unwrap();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0]["subject"], "12");
    assert!(
        refused[0]["message"].as_str().unwrap().contains("dose"),
        "{refused:?}"
    );
    let ids: Vec<&str> = answer["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"]
    );
    // The same as the population fit of the file without subject 12, to the bit.
    let eleven: String = theoph_csv()
        .lines()
        .filter(|l| !l.starts_with("12,"))
        .map(|l| format!("{l}\n"))
        .collect();
    let reference = direct(
        &eleven,
        with(
            theoph_request(),
            json!({ "options": { "evaluate_only": true } }),
        ),
    );
    same_bits(
        &without(answer.clone(), &["analysis", "refused_subjects"]),
        &reference,
        "answer",
    );
    // Excluded by the person, the refused subject is not reported.
    let excluded = ok(
        &mut engine,
        "pop.fit",
        with(
            evaluation(),
            json!({ "options": { "evaluate_only": true, "exclude_subjects": ["12"] } }),
        ),
    );
    assert!(excluded.get("refused_subjects").is_none(), "{excluded}");
    assert_eq!(excluded["n_subjects"], 11);
    // A label that is no subject is refused by the population fit.
    let e = err(
        &mut engine,
        "pop.fit",
        with(
            evaluation(),
            json!({ "options": { "evaluate_only": true, "exclude_subjects": ["99"] } }),
        ),
    );
    assert_eq!(e.code, "invalid_option", "{e}");
    assert!(e.message.contains("`99`"), "{e}");
}

#[test]
fn a_population_fit_is_an_analysis_like_the_others() {
    let mut engine = engine_with(&theoph_csv());
    let first = ok(&mut engine, "pop.fit", evaluation());
    let id = first["analysis"].clone();
    // Run again in place, with `analysis`, and by `analysis.run`: the same answer.
    let named = ok(
        &mut engine,
        "pop.fit",
        with(evaluation(), json!({ "analysis": id, "name": "Theoph" })),
    );
    assert_eq!(named["analysis"], id);
    assert_eq!(named, first);
    let rerun = ok(&mut engine, "analysis.run", json!({ "analysis": id }));
    assert_eq!(rerun["name"], "Theoph");
    assert_eq!(
        without(rerun["result"].clone(), &["kind"]),
        without(first.clone(), &["analysis"])
    );
    // `fit.run` does not update a population fit.
    let e = err(
        &mut engine,
        "fit.run",
        json!({ "worksheet": 1, "subject": "1", "model": "pk1.oral_1", "analysis": id }),
    );
    assert_eq!(e.code, "analysis_kind_changed", "{e}");
    // A change of the data makes it stale, as any analysis.
    ok(
        &mut engine,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "DV", "row": 3, "value": 2.9 }),
    );
    let view = ok(&mut engine, "analysis.get", json!({ "analysis": id }));
    assert_eq!(view["status"]["state"], "stale", "{}", view["status"]);
    // The tables, the plots and the comparison of single fits say what they read.
    for (command, params) in [
        (
            "export.table",
            json!({ "analysis": id, "table": "fit.parameters" }),
        ),
        ("plot.xy", json!({ "fit": id })),
        ("analysis.compare", json!({ "a": id, "b": id })),
    ] {
        let e = err(&mut engine, command, params);
        assert_ne!(e.code, "invalid_parameters", "{command}: {e}");
        assert!(!e.message.is_empty());
    }
}
