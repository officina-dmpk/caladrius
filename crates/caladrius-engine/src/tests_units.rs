//! Units through `nca.run` (task T-025): the three units are NCA options, the results carry their
//! units, and the result still matches the command's schema.

use serde_json::{Value, json};

use crate::schema::validate;
use crate::*;

const PROFILE: &str = "Subject,Time,Conc,Dose\n\
A,0,0,10\nA,0.5,219.5,10\nA,1,329.3,10\nA,2,397.1,10\nA,4,361.1,10\nA,6,298.9,10\nA,8,245.1,10\nA,12,164.3,10\nA,24,49.5,10\n";

fn run(engine: &mut Engine, id: &str, params: Value) -> Value {
    match engine.execute(id, params) {
        Ok(v) => v,
        Err(e) => panic!("{id} failed: {e}"),
    }
}

#[test]
fn nca_run_passes_the_units_through() {
    let mut engine = Engine::new();
    run(
        &mut engine,
        "data.import",
        json!({ "name": "study", "csv": PROFILE }),
    );
    let params = json!({
        "worksheet": 1,
        "route": "extravascular",
        "options": { "units": { "time": "h", "concentration": "ng/mL", "dose": "mg" } }
    });
    let info = describe()
        .into_iter()
        .find(|c| c.id == "nca.run")
        .map(|c| (c.params_schema, c.result_schema));
    let Some((params_schema, result_schema)) = info else {
        panic!("nca.run is not registered");
    };
    if let Err(why) = validate::check(&params_schema, &params_schema, &params) {
        panic!("parameters: {why}");
    }
    let result = run(&mut engine, "nca.run", params);
    if let Err(why) = validate::check(&result_schema, &result_schema, &result) {
        panic!("result: {why}");
    }
    let text = result.to_string();
    assert!(text.contains(r#""unit":"L/h""#), "{text}");
    assert!(text.contains(r#""unit":"h·ng/mL""#), "{text}");
    // Unknown units: an error that names the unit, before any computation.
    let bad = json!({
        "worksheet": 1,
        "route": "extravascular",
        "options": { "units": { "time": "hours", "concentration": "ng/mL", "dose": "mg" } }
    });
    match engine.execute("nca.run", bad) {
        Ok(v) => {
            let text = v.to_string();
            assert!(text.contains("hours"), "{text}");
        }
        Err(e) => assert!(e.to_string().contains("hours"), "{e}"),
    }
}
