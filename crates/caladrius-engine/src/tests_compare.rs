//! `analysis.compare` (task T-042): the difference, relative difference and ratio of the
//! parameters two analyses share, with the unit checks, computed by the engine. The expected
//! numbers are the values of the two results (read through `export.table`) combined here by hand,
//! and, for the areas, an independent trapezoid sum.

use serde_json::{Value, json};

use crate::schema::validate;
use crate::*;

/// One oral profile, mg/L, mg (the same shape as the benchmark's exercises).
const TIMES: [f64; 9] = [0.0, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 24.0];
const CONC: [f64; 9] = [0.0, 2.195, 3.293, 3.971, 3.611, 2.989, 2.451, 1.643, 0.495];

fn csv(header: &str, times: &[f64], conc: &[f64]) -> String {
    let mut out = format!("Subject,{header},Dose (mg)\n");
    for (t, c) in times.iter().zip(conc) {
        out.push_str(&format!("A,{t},{c},100\n"));
    }
    out
}

fn run(engine: &mut Engine, id: &str, params: Value) -> Value {
    match engine.execute(id, params) {
        Ok(v) => v,
        Err(e) => panic!("{id} failed: {e}"),
    }
}

fn refuse(engine: &mut Engine, params: Value) -> CommandError {
    match engine.execute("analysis.compare", params) {
        Ok(v) => panic!("analysis.compare should have failed, gave {v}"),
        Err(e) => e,
    }
}

fn engine_with(sheets: &[(&str, String)]) -> Engine {
    let mut e = Engine::new();
    for (name, csv) in sheets {
        run(&mut e, "data.import", json!({ "name": name, "csv": csv }));
    }
    e
}

fn mg_per_l() -> String {
    csv("Time (h),Conc (mg/L)", &TIMES, &CONC)
}

fn units(conc: &str) -> Value {
    json!({ "time": "h", "concentration": conc, "dose": "mg" })
}

/// Runs `nca.run` and returns the id of the analysis.
fn nca(e: &mut Engine, worksheet: u64, options: Value) -> u64 {
    let r = run(
        e,
        "nca.run",
        json!({ "worksheet": worksheet, "route": "extravascular", "options": options }),
    );
    r["id"].as_u64().unwrap()
}

fn compare(e: &mut Engine, params: Value) -> Value {
    let result = run(e, "analysis.compare", params);
    let info = describe()
        .into_iter()
        .find(|c| c.id == "analysis.compare")
        .unwrap();
    if let Err(why) = validate::check(&info.result_schema, &info.result_schema, &result) {
        panic!("result: {why}\n{result}");
    }
    result
}

fn row<'a>(result: &'a Value, parameter: &str) -> &'a Value {
    result["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["parameter"] == parameter))
        .unwrap_or_else(|| panic!("no row for {parameter} in {result}"))
}

/// The value of an NCA parameter (`export.table`).
fn nca_value(e: &mut Engine, analysis: u64, name: &str) -> f64 {
    let t = run(
        e,
        "export.table",
        json!({ "table": "nca.parameters", "analysis": analysis }),
    );
    t["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[1] == name)
        .and_then(|r| r[2].as_f64())
        .unwrap_or_else(|| panic!("{name} is not a value in {t}"))
}

fn fit_value(e: &mut Engine, analysis: u64, name: &str) -> f64 {
    let t = run(
        e,
        "export.table",
        json!({ "table": "fit.values", "analysis": analysis }),
    );
    t["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == name)
        .and_then(|r| r[1].as_f64())
        .unwrap_or_else(|| panic!("{name} is not a value in {t}"))
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs()
}

// ---- two NCA analyses, two AUC methods -------------------------------------------------

#[test]
fn two_auc_methods_of_one_worksheet_give_the_difference_the_percentage_and_the_ratio() {
    let mut e = engine_with(&[("study", mg_per_l())]);
    let linear = nca(
        &mut e,
        1,
        json!({ "auc_method": "linear", "units": units("mg/L") }),
    );
    let mixed = nca(&mut e, 1, json!({ "units": units("mg/L") }));
    let r = compare(
        &mut e,
        json!({ "a": linear, "b": mixed, "parameters": ["auclast"] }),
    );
    assert_eq!(r["a"]["analysis"], linear);
    assert_eq!(r["a"]["kind"], "nca");
    assert_eq!(r["a"]["subject"], "A");
    assert_eq!(r["b"]["status"], "fresh");
    let rows = r["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let auc = &rows[0];
    let (a, b) = (
        nca_value(&mut e, linear, "auclast"),
        nca_value(&mut e, mixed, "auclast"),
    );
    // The linear area, by an independent sum of trapezoids.
    let trapezoids: f64 = TIMES
        .windows(2)
        .zip(CONC.windows(2))
        .map(|(t, c)| (t[1] - t[0]) * (c[0] + c[1]) / 2.0)
        .sum();
    assert!(close(a, trapezoids, 1e-12), "{a} {trapezoids}");
    assert!(
        b < a,
        "log-down trapezoids give a smaller area on a decline"
    );
    assert_eq!(auc["a"], a);
    assert_eq!(auc["b"], b);
    assert_eq!(auc["unit_a"], "h·mg/L");
    assert_eq!(auc["unit_b"], "h·mg/L");
    assert_eq!(auc["difference"], b - a);
    assert_eq!(auc["difference_unit"], "h·mg/L");
    assert_eq!(auc["relative_percent"], (b - a) / a * 100.0);
    assert_eq!(auc["ratio"], b / a);
    assert_eq!(auc["not_comparable"], Value::Null);
}

#[test]
fn without_a_list_every_parameter_present_in_both_is_compared_in_the_order_of_a() {
    let mut e = engine_with(&[("study", mg_per_l())]);
    let linear = nca(&mut e, 1, json!({ "auc_method": "linear" }));
    let mixed = nca(&mut e, 1, json!({}));
    let r = compare(&mut e, json!({ "a": linear, "b": mixed }));
    let names: Vec<&str> = r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["parameter"].as_str())
        .collect();
    assert_eq!(names.first(), Some(&"c0"));
    assert!(names.contains(&"cmax") && names.contains(&"auclast") && names.contains(&"half.life"));
    // Same data, same peak: the areas differ, Cmax does not.
    let cmax = row(&r, "cmax");
    assert_eq!(cmax["difference"], 0.0);
    assert_eq!(cmax["ratio"], 1.0);
    assert_eq!(cmax["relative_percent"], 0.0);
    assert!(row(&r, "auclast")["difference"].as_f64().unwrap() < 0.0);
    // No units were given: the values are in the units of the data, and that is not an obstacle.
    assert_eq!(cmax["unit_a"], Value::Null);
    // A parameter that is not calculated in either is a row that says so.
    let c0 = row(&r, "c0");
    assert_eq!(c0["difference"], Value::Null);
    assert!(
        c0["not_comparable"]
            .as_str()
            .unwrap()
            .contains("analysis a")
    );
    assert!(
        c0["not_comparable"]
            .as_str()
            .unwrap()
            .contains("analysis b")
    );
    // A list in another order and with a duplicate: asked order, once each.
    let r = compare(
        &mut e,
        json!({ "a": linear, "b": mixed, "parameters": ["tmax", "cmax", "tmax"] }),
    );
    let names: Vec<&str> = r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["parameter"].as_str())
        .collect();
    assert_eq!(names, ["tmax", "cmax"]);
}

// ---- an NCA and a fit -------------------------------------------------------------------

#[test]
fn an_nca_and_a_fit_meet_on_half_life_clearance_and_the_area_to_infinity() {
    let mut e = engine_with(&[("study", mg_per_l())]);
    let n = nca(&mut e, 1, json!({ "units": units("mg/L") }));
    let f = run(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "model": "pk1.oral_1" }),
    )["id"]
        .as_u64()
        .unwrap();
    let r = compare(&mut e, json!({ "a": n, "b": f }));
    assert_eq!(r["a"]["kind"], "nca");
    assert_eq!(r["b"]["kind"], "fit");
    let half = row(&r, "half.life");
    let (a, b) = (
        nca_value(&mut e, n, "half.life"),
        fit_value(&mut e, f, "estimate.half_life"),
    );
    assert_eq!(half["a"], a);
    assert_eq!(half["b"], b);
    assert_eq!(half["unit_a"], "h");
    assert_eq!(half["unit_b"], "h");
    assert_eq!(half["difference"], b - a);
    assert_eq!(half["ratio"], b / a);
    // The clearance and the area to infinity are in the same units on both sides.
    let cl = row(&r, "cl.obs");
    assert_eq!(cl["unit_a"], "L/h");
    assert_eq!(cl["unit_b"], "L/h");
    assert_eq!(cl["b"], fit_value(&mut e, f, "estimate.cl"));
    assert_eq!(cl["not_comparable"], Value::Null);
    let auc = row(&r, "aucinf.obs");
    assert_eq!(auc["unit_b"], "h*mg/L");
    assert_eq!(auc["not_comparable"], Value::Null);
    // The fit's own parameters (v, k, ka) have no counterpart in the NCA: not listed.
    let names: Vec<&str> = r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["parameter"].as_str())
        .collect();
    assert!(!names.contains(&"ka") && !names.contains(&"v"), "{names:?}");
    // Asked for by name, a parameter only one side has is a row that says which.
    let r = compare(
        &mut e,
        json!({ "a": n, "b": f, "parameters": ["cmax", "ka", "half_life"] }),
    );
    let cmax = row(&r, "cmax");
    assert_eq!(cmax["difference"], Value::Null);
    assert_eq!(cmax["not_comparable"], "analysis b has no such parameter");
    assert_eq!(cmax["a"], nca_value(&mut e, n, "cmax"));
    assert_eq!(
        row(&r, "ka")["not_comparable"],
        "analysis a has no such parameter"
    );
    // The fit's spelling of a name is accepted and reported under the NCA one.
    assert_eq!(row(&r, "half.life")["difference"], b - a);
}

#[test]
fn two_fits_compare_their_parameters_and_flag_units_the_worksheet_does_not_give() {
    // The second subject is the first one with 20 % more of everything but the time.
    let louder: Vec<f64> = CONC.iter().map(|c| c * 1.2 + 0.01).collect();
    let mut e = engine_with(&[
        ("study", mg_per_l()),
        ("louder", csv("Time (h),Conc (mg/L)", &TIMES, &louder)),
    ]);
    let fit = |e: &mut Engine, w: u64| {
        run(
            e,
            "fit.run",
            json!({ "worksheet": w, "model": "pk1.oral_1" }),
        )["id"]
            .as_u64()
            .unwrap()
    };
    let (f1, f2) = (fit(&mut e, 1), fit(&mut e, 2));
    let r = compare(
        &mut e,
        json!({ "a": f1, "b": f2, "parameters": ["k", "ka", "v"] }),
    );
    let k = row(&r, "k");
    assert_eq!(k["unit_a"], "1/h");
    let (a, b) = (
        fit_value(&mut e, f1, "estimate.k"),
        fit_value(&mut e, f2, "estimate.k"),
    );
    assert_eq!(k["difference"], b - a);
    assert_eq!(k["ratio"], b / a);
    assert_eq!(row(&r, "v")["unit_a"], "L");
    // A worksheet without units: the values are compared, in the units of the data.
    let mut bare = engine_with(&[
        ("bare", csv("Time,Conc", &TIMES, &CONC)),
        ("bare2", csv("Time,Conc", &TIMES, &louder)),
    ]);
    let (g1, g2) = (fit(&mut bare, 1), fit(&mut bare, 2));
    let r = compare(&mut bare, json!({ "a": g1, "b": g2, "parameters": ["k"] }));
    assert_eq!(row(&r, "k")["unit_a"], Value::Null);
    assert!(row(&r, "k")["difference"].is_number());
}

// ---- when the numbers must not be computed ---------------------------------------------

#[test]
fn a_unit_mismatch_gives_the_values_and_a_reason_but_no_number() {
    let ug = csv("Time (h),Conc (µg/L)", &TIMES, &CONC);
    let mut e = engine_with(&[("mg", mg_per_l()), ("ug", ug)]);
    let a = nca(&mut e, 1, json!({ "units": units("mg/L") }));
    let b = nca(&mut e, 2, json!({ "units": units("µg/L") }));
    let r = compare(
        &mut e,
        json!({ "a": a, "b": b, "parameters": ["auclast", "tmax", "cmax"] }),
    );
    let auc = row(&r, "auclast");
    assert_eq!(auc["unit_a"], "h·mg/L");
    assert_eq!(auc["unit_b"], "h·µg/L");
    assert_eq!(auc["a"], nca_value(&mut e, a, "auclast"));
    for field in ["difference", "difference_unit", "relative_percent", "ratio"] {
        assert_eq!(auc[field], Value::Null, "{field}");
    }
    let why = auc["not_comparable"].as_str().unwrap();
    assert!(
        why.contains("unit mismatch") && why.contains("h·mg/L") && why.contains("h·µg/L"),
        "{why}"
    );
    assert!(why.contains("no unit is converted"), "{why}");
    // The time is in hours on both sides: that one is compared.
    assert_eq!(row(&r, "tmax")["difference"], 0.0);
    assert!(row(&r, "cmax")["not_comparable"].is_string());
}

#[test]
fn a_fit_clearance_in_other_units_than_the_nca_one_is_not_compared() {
    // ng/mL and mg: the NCA reports litres per hour, the fit's units follow the columns.
    let ng = csv("Time (h),Conc (ng/mL)", &TIMES, &CONC);
    let mut e = engine_with(&[("ng", ng)]);
    let n = nca(&mut e, 1, json!({ "units": units("ng/mL") }));
    let f = run(
        &mut e,
        "fit.run",
        json!({ "worksheet": 1, "model": "pk1.oral_1" }),
    )["id"]
        .as_u64()
        .unwrap();
    let r = compare(
        &mut e,
        json!({ "a": n, "b": f, "parameters": ["cl", "half_life"] }),
    );
    let cl = row(&r, "cl.obs");
    assert_eq!(cl["unit_a"], "L/h");
    assert!(
        cl["not_comparable"]
            .as_str()
            .unwrap()
            .contains("unit mismatch")
    );
    assert_eq!(cl["difference"], Value::Null);
    assert!(row(&r, "half.life")["difference"].is_number());
    // The NCA of the same worksheet without units: the unit of a is not known.
    let bare = nca(&mut e, 1, json!({}));
    let r = compare(
        &mut e,
        json!({ "a": bare, "b": f, "parameters": ["half_life"] }),
    );
    let half = row(&r, "half.life");
    assert_eq!(half["difference"], Value::Null);
    assert!(
        half["not_comparable"]
            .as_str()
            .unwrap()
            .contains("the unit of a is not known")
    );
}

#[test]
fn a_value_that_was_not_calculated_says_why() {
    // A profile that is still rising has no terminal phase: no half-life.
    let rising = csv(
        "Time (h),Conc (mg/L)",
        &[0.0, 1.0, 2.0, 4.0],
        &[0.0, 1.0, 2.0, 3.0],
    );
    let mut e = engine_with(&[("study", mg_per_l()), ("rising", rising)]);
    let a = nca(&mut e, 1, json!({ "units": units("mg/L") }));
    let b = nca(&mut e, 2, json!({ "units": units("mg/L") }));
    let r = compare(
        &mut e,
        json!({ "a": a, "b": b, "parameters": ["half.life", "cmax"] }),
    );
    let half = row(&r, "half.life");
    assert_eq!(half["a"], nca_value(&mut e, a, "half.life"));
    assert_eq!(half["b"], Value::Null);
    assert_eq!(half["difference"], Value::Null);
    assert_eq!(half["ratio"], Value::Null);
    let why = half["not_comparable"].as_str().unwrap();
    assert!(why.starts_with("analysis b: not calculated, "), "{why}");
    assert!(why.len() > "analysis b: not calculated, ".len(), "{why}");
    // The other direction names a.
    let r = compare(
        &mut e,
        json!({ "a": b, "b": a, "parameters": ["half.life"] }),
    );
    assert!(
        row(&r, "half.life")["not_comparable"]
            .as_str()
            .unwrap()
            .starts_with("analysis a: not calculated")
    );
    // A parameter neither analysis has.
    let r = compare(&mut e, json!({ "a": a, "b": b, "parameters": ["no.such"] }));
    let why = row(&r, "no.such")["not_comparable"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        why.contains("analysis a has no such parameter")
            && why.contains("analysis b has no such parameter"),
        "{why}"
    );
}

#[test]
fn a_zero_in_a_keeps_the_difference_and_refuses_the_percentage_and_the_ratio() {
    // The first profile has no lag (tlag 0); the second starts rising only after 1 h.
    let lagged = csv(
        "Time (h),Conc (mg/L)",
        &[0.0, 0.5, 1.0, 2.0, 4.0, 8.0],
        &[0.0, 0.0, 0.0, 3.0, 2.0, 1.0],
    );
    let mut e = engine_with(&[("study", mg_per_l()), ("lag", lagged)]);
    let a = nca(&mut e, 1, json!({ "units": units("mg/L") }));
    let b = nca(&mut e, 2, json!({ "units": units("mg/L") }));
    let r = compare(&mut e, json!({ "a": a, "b": b, "parameters": ["tlag"] }));
    let tlag = row(&r, "tlag");
    assert_eq!(tlag["a"], 0.0);
    let after = nca_value(&mut e, b, "tlag");
    assert!(after > 0.0, "{after}");
    assert_eq!(tlag["difference"], after);
    assert_eq!(tlag["difference_unit"], "h");
    assert_eq!(tlag["relative_percent"], Value::Null);
    assert_eq!(tlag["ratio"], Value::Null);
    assert!(
        tlag["not_comparable"]
            .as_str()
            .unwrap()
            .starts_with("a is zero")
    );
    // A zero in b is an ordinary value: -100 % and a ratio of 0.
    let r = compare(&mut e, json!({ "a": b, "b": a, "parameters": ["tlag"] }));
    let tlag = row(&r, "tlag");
    assert_eq!(tlag["relative_percent"], -100.0);
    assert_eq!(tlag["ratio"], 0.0);
}

// ---- refusals and the schema ------------------------------------------------------------

#[test]
fn the_analyses_must_exist_have_a_result_and_hold_one_subject() {
    let two = "Subject,Time (h),Conc (mg/L),Dose (mg)\nA,0,0,100\nA,1,5,100\nA,2,3,100\nA,4,1,100\nB,0,0,100\nB,1,6,100\nB,2,3,100\nB,4,1,100\n";
    let mut e = engine_with(&[("study", mg_per_l()), ("two", two.to_owned())]);
    let a = nca(&mut e, 1, json!({}));
    let err = refuse(&mut e, json!({ "a": a, "b": 99 }));
    assert_eq!(err.code, "unknown_analysis", "{err}");
    // Two subjects in one analysis: which one?
    let many = nca(&mut e, 2, json!({}));
    let err = refuse(&mut e, json!({ "a": a, "b": many }));
    assert_eq!(err.code, "ambiguous_subject", "{err}");
    assert!(
        err.message.contains("A, B") && err.message.contains("subject"),
        "{err}"
    );
    // One subject of it is fine.
    let one = run(
        &mut e,
        "nca.run",
        json!({ "worksheet": 2, "route": "extravascular", "subject": "B" }),
    )["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        compare(&mut e, json!({ "a": a, "b": one, "parameters": ["cmax"] }))["b"]["subject"],
        "B"
    );
    // A simulation is not a result to compare.
    let sim = run(
        &mut e,
        "model.simulate",
        json!({ "model": "pk1.iv_bolus", "dose": 100, "params": { "v": 10, "k": 0.1 }, "times": [0, 1, 2], "store": true }),
    )["analysis"]
        .as_u64()
        .unwrap();
    assert_eq!(
        refuse(&mut e, json!({ "a": a, "b": sim })).code,
        "wrong_kind"
    );
    // A stale result is compared, and says so.
    run(
        &mut e,
        "data.set_cell",
        json!({ "worksheet": 1, "column": "Conc", "row": 3, "value": 4.0 }),
    );
    let r = compare(&mut e, json!({ "a": a, "b": one, "parameters": ["cmax"] }));
    assert_eq!(r["a"]["status"], "stale");
    assert_eq!(r["b"]["status"], "fresh");
    // Unknown fields and a missing id.
    assert_eq!(refuse(&mut e, json!({ "a": a })).code, "invalid_parameters");
    assert_eq!(
        refuse(&mut e, json!({ "a": a, "b": a, "x": 1 })).code,
        "invalid_parameters"
    );
}

#[test]
fn comparing_an_analysis_with_itself_gives_no_difference() {
    let mut e = engine_with(&[("study", mg_per_l())]);
    let a = nca(&mut e, 1, json!({ "units": units("mg/L") }));
    let r = compare(&mut e, json!({ "a": a, "b": a, "parameters": ["auclast"] }));
    let auc = row(&r, "auclast");
    assert_eq!(auc["difference"], 0.0);
    assert_eq!(auc["relative_percent"], 0.0);
    assert_eq!(auc["ratio"], 1.0);
}

#[test]
fn the_command_is_a_read_that_stores_nothing() {
    let mut e = engine_with(&[("study", mg_per_l())]);
    let a = nca(&mut e, 1, json!({}));
    let before = e.project().analyses().len();
    compare(&mut e, json!({ "a": a, "b": a }));
    assert_eq!(e.project().analyses().len(), before);
    let info = describe()
        .into_iter()
        .find(|c| c.id == "analysis.compare")
        .unwrap();
    assert!(!info.mutates);
}
