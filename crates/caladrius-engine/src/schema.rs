//! JSON Schema (draft 2020-12) of the parameters and results of the commands, written by hand
//! next to the serde types they describe. The MCP server serves exactly these values
//! (`describe()`), so a machine sees the same contract as the UI.
//!
//! The schemas are not generated, so the tests check them against the serde types: the default
//! options and every real output must validate (see `tests/`), and the types reject what the
//! schema forbids.

use std::collections::BTreeSet;

use caladrius_models::ModelId;
use serde_json::{Map, Value, json};

pub(crate) const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

// ---- small builders ----------------------------------------------------------------------

pub(crate) fn string() -> Value {
    json!({ "type": "string" })
}

pub(crate) fn number() -> Value {
    json!({ "type": "number" })
}

pub(crate) fn integer() -> Value {
    json!({ "type": "integer" })
}

pub(crate) fn boolean() -> Value {
    json!({ "type": "boolean" })
}

pub(crate) fn nullable(inner: Value) -> Value {
    json!({ "anyOf": [inner, { "type": "null" }] })
}

/// A subject label: text or a number, or null for none.
pub(crate) fn subject() -> Value {
    json!({ "anyOf": [{ "type": "string" }, { "type": "integer" }, { "type": "number" }, { "type": "null" }] })
}

pub(crate) fn array_of(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

pub(crate) fn one_of_strings(values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values })
}

pub(crate) fn reference(name: &str) -> Value {
    json!({ "$ref": format!("#/$defs/{name}") })
}

pub(crate) fn described(mut schema: Value, text: &str) -> Value {
    if let Some(map) = schema.as_object_mut() {
        map.insert("description".to_owned(), json!(text));
    }
    schema
}

/// An object with these properties; nothing else is accepted.
pub(crate) fn object(properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let props: Map<String, Value> = properties
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

/// An object whose listed properties are checked and which may carry others (used for the large
/// results of the numerical crates).
pub(crate) fn open_object(properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut v = object(properties, required);
    if let Some(map) = v.as_object_mut() {
        map.insert("additionalProperties".to_owned(), json!(true));
    }
    v
}

/// The schema of a command: a root object with `$defs` holding the definitions it uses.
pub(crate) fn root(title: &str, body: Value) -> Value {
    let mut map = match body {
        Value::Object(m) => m,
        other => {
            let mut m = Map::new();
            m.insert("allOf".to_owned(), json!([other]));
            m
        }
    };
    map.insert("$schema".to_owned(), json!(DIALECT));
    map.insert("title".to_owned(), json!(title));
    let mut used = BTreeSet::new();
    let mut defs = Map::new();
    collect(&Value::Object(map.clone()), &mut used, &mut defs);
    if !defs.is_empty() {
        map.insert("$defs".to_owned(), Value::Object(defs));
    }
    Value::Object(map)
}

/// Adds the definitions referenced from `value`, and those they reference.
fn collect(value: &Value, used: &mut BTreeSet<String>, defs: &mut Map<String, Value>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref") {
                if let Some(name) = r.strip_prefix("#/$defs/") {
                    if used.insert(name.to_owned()) {
                        if let Some(def) = definition(name) {
                            collect(&def, used, defs);
                            defs.insert(name.to_owned(), def);
                        }
                    }
                }
            }
            for v in map.values() {
                collect(v, used, defs);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect(v, used, defs);
            }
        }
        _ => {}
    }
}

// ---- shared definitions -------------------------------------------------------------------

/// The definition called `name`, if there is one.
pub(crate) fn definition(name: &str) -> Option<Value> {
    Some(match name {
        "Id" => described(
            json!({ "type": "integer", "minimum": 1 }),
            "Identifier of a worksheet or an analysis, as returned by the command that created it.",
        ),
        "ColumnRole" => {
            one_of_strings(&["time", "concentration", "subject", "dose", "route", "other"])
        }
        "Route" => json!({
            "description": "Route of administration.",
            "oneOf": [
                { "type": "string", "enum": ["extravascular", "iv_bolus"] },
                object(
                    vec![("iv_infusion", object(vec![("duration", json!({ "type": "number", "exclusiveMinimum": 0 }))], &["duration"]))],
                    &["iv_infusion"],
                ),
            ],
        }),
        "ModelId" => {
            let ids: Vec<&str> = ModelId::ALL.iter().map(ModelId::id).collect();
            described(
                one_of_strings(&ids),
                "One-compartment model: route and input (see `describe` of the model catalogue).",
            )
        }
        "Weighting" => one_of_strings(&["uniform", "inv_y", "inv_y2", "inv_yhat", "inv_yhat2"]),
        "BlqAction" => json!({
            "oneOf": [
                { "type": "string", "enum": ["keep", "drop"] },
                object(vec![("set", number())], &["set"]),
            ]
        }),
        "BlqPolicy" => json!({
            "oneOf": [
                object(
                    vec![(
                        "position",
                        object(
                            vec![
                                ("first", reference("BlqAction")),
                                ("middle", reference("BlqAction")),
                                ("last", reference("BlqAction")),
                            ],
                            &["first", "middle", "last"],
                        ),
                    )],
                    &["position"],
                ),
                object(
                    vec![(
                        "tmax",
                        object(
                            vec![
                                ("before", reference("BlqAction")),
                                ("after", reference("BlqAction")),
                            ],
                            &["before", "after"],
                        ),
                    )],
                    &["tmax"],
                ),
            ]
        }),
        "NcaOptions" => described(
            object(
                vec![
                    (
                        "auc_method",
                        one_of_strings(&["linear", "lin_up_log_down", "lin_log"]),
                    ),
                    (
                        "lambda_z",
                        object(
                            vec![
                                ("min_points", json!({ "type": "integer", "minimum": 3 })),
                                ("allow_tmax", boolean()),
                                (
                                    "adj_r_squared_factor",
                                    json!({ "type": "number", "minimum": 0 }),
                                ),
                            ],
                            &[],
                        ),
                    ),
                    (
                        "missing",
                        json!({ "oneOf": [
                            { "type": "string", "enum": ["drop"] },
                            object(vec![("replace", json!({ "type": "number", "minimum": 0 }))], &["replace"]),
                        ] }),
                    ),
                    ("negative", one_of_strings(&["error", "allow", "set_zero"])),
                    ("blq", reference("BlqPolicy")),
                    ("start", one_of_strings(&["none", "zero", "c0"])),
                    ("tmax_tie", one_of_strings(&["first", "last"])),
                    (
                        "lambda_z_selection",
                        object(
                            vec![
                                ("tie_rule", one_of_strings(&["tolerance", "bonus"])),
                                ("positive_filter_first", boolean()),
                                ("exclude", array_of(number())),
                                (
                                    "manual",
                                    nullable(json!({ "oneOf": [
                                        object(vec![("times", array_of(number()))], &["times"]),
                                        object(
                                            vec![(
                                                "range",
                                                object(vec![("start", number()), ("end", number())], &["start", "end"]),
                                            )],
                                            &["range"],
                                        ),
                                    ] })),
                                ),
                                ("exclude_replaced", boolean()),
                            ],
                            &[],
                        ),
                    ),
                    (
                        "quality",
                        object(
                            vec![
                                ("min_adj_r_squared", nullable(number())),
                                ("min_span_ratio", nullable(number())),
                                ("max_extrapolated_percent", nullable(number())),
                                (
                                    "min_points",
                                    nullable(json!({ "type": "integer", "minimum": 0 })),
                                ),
                            ],
                            &[],
                        ),
                    ),
                    (
                        "units",
                        nullable(described(
                            object(
                                vec![
                                    ("time", one_of_strings(&["s", "min", "h", "d"])),
                                    ("concentration", string()),
                                    ("dose", string()),
                                ],
                                &["time", "concentration", "dose"],
                            ),
                            "Units of time, concentration (mass/volume: ng, µg (ug, mcg), mg, g per mL or L) and dose (a mass). Results then carry a unit each; CL and volumes are in L. Absent: unitless results, flagged units_missing.",
                        )),
                    ),
                ],
                &[],
            ),
            "Conventions of the NCA; every field is optional and defaults to the documented value.",
        ),
        "FitOptions" => {
            let bound = object(
                vec![("lower", nullable(number())), ("upper", nullable(number()))],
                &[],
            );
            described(
                object(
                    vec![
                        (
                            "derivatives",
                            one_of_strings(&["forward_difference", "analytic"]),
                        ),
                        (
                            "increment",
                            json!({ "type": "number", "exclusiveMinimum": 0, "maximum": 0.1 }),
                        ),
                        (
                            "criterion",
                            one_of_strings(&["relative_decrease", "relative_offset"]),
                        ),
                        (
                            "convergence",
                            json!({ "type": "number", "minimum": 0, "maximum": 0.1 }),
                        ),
                        (
                            "max_iterations",
                            json!({ "type": "integer", "minimum": 1, "maximum": 100_000 }),
                        ),
                        (
                            "confidence_level",
                            json!({ "type": "number", "exclusiveMinimum": 0, "exclusiveMaximum": 1 }),
                        ),
                        (
                            "n_curve",
                            json!({ "type": "integer", "minimum": 0, "maximum": 1_000_000 }),
                        ),
                        (
                            "fixed",
                            json!({ "type": "object", "additionalProperties": number() }),
                        ),
                        (
                            "bounds",
                            json!({ "type": "object", "additionalProperties": bound }),
                        ),
                        (
                            "flags",
                            object(
                                vec![
                                    ("max_cv_percent", nullable(number())),
                                    ("max_abs_correlation", nullable(number())),
                                    ("max_condition_number", nullable(number())),
                                    (
                                        "min_degrees_of_freedom",
                                        nullable(json!({ "type": "integer", "minimum": 0 })),
                                    ),
                                ],
                                &[],
                            ),
                        ),
                    ],
                    &[],
                ),
                "Options of the fit; every field is optional and defaults to the documented value.",
            )
        }
        "AnalysisStatus" => json!({
            "oneOf": [
                object(vec![("state", json!({ "const": "no_result" }))], &["state"]),
                object(vec![("state", json!({ "const": "fresh" }))], &["state"]),
                object(
                    vec![
                        ("state", json!({ "const": "stale" })),
                        ("reasons", array_of(open_object(vec![("code", one_of_strings(&["input_changed", "options_changed", "worksheet_missing"]))], &["code"]))),
                    ],
                    &["state", "reasons"],
                ),
            ]
        }),
        "NcaResult" => open_object(
            vec![
                (
                    "parameters",
                    array_of(object(
                        vec![
                            ("name", string()),
                            (
                                "value",
                                json!({ "type": "object", "minProperties": 1, "maxProperties": 1,
                                        "properties": { "value": number(), "not_calculated": string() } }),
                            ),
                            ("unit", string()),
                        ],
                        &["name", "value"],
                    )),
                ),
                ("units_missing", boolean()),
                ("profile", array_of(json!({ "type": "object" }))),
                ("removed", array_of(json!({ "type": "object" }))),
                ("lambda_z_candidates", array_of(json!({ "type": "object" }))),
                ("flags", array_of(json!({ "type": "object" }))),
            ],
            &["parameters", "profile", "removed", "lambda_z_candidates"],
        ),
        "FitResult" => open_object(
            vec![
                (
                    "status",
                    one_of_strings(&[
                        "converged",
                        "max_iterations",
                        "no_decrease",
                        "singular",
                        "non_finite",
                        "at_bound",
                    ]),
                ),
                ("parameters", array_of(string())),
                (
                    "values",
                    json!({ "type": "object", "additionalProperties": number() }),
                ),
                (
                    "observations",
                    array_of(open_object(
                        vec![
                            ("time", number()),
                            ("observed", number()),
                            ("predicted", number()),
                            ("residual", number()),
                        ],
                        &["time", "observed", "predicted", "residual"],
                    )),
                ),
                (
                    "curve",
                    array_of(object(
                        vec![("time", number()), ("conc", number())],
                        &["time", "conc"],
                    )),
                ),
                ("trace", array_of(json!({ "type": "object" }))),
                ("partials", array_of(array_of(number()))),
                ("flags", array_of(json!({ "type": "object" }))),
            ],
            &[
                "status",
                "parameters",
                "values",
                "observations",
                "curve",
                "trace",
                "partials",
            ],
        ),
        "ModelOutput" => object(
            vec![
                ("conc", array_of(number())),
                ("auc", array_of(number())),
                (
                    "secondary",
                    json!({ "type": "object", "additionalProperties": number() }),
                ),
            ],
            &["conc", "auc", "secondary"],
        ),
        "AnalysisResult" => {
            let outcome = |inner: Value| {
                json!({ "oneOf": [
                    object(vec![("ok", inner)], &["ok"]),
                    object(vec![("error", string())], &["error"]),
                ] })
            };
            json!({ "oneOf": [
                open_object(
                    vec![
                        ("kind", json!({ "const": "nca" })),
                        ("subjects", array_of(object(
                            vec![
                                ("subject", string()),
                                ("dose", nullable(number())),
                                ("route", nullable(reference("Route"))),
                                ("outcome", outcome(reference("NcaResult"))),
                                ("flag_messages", array_of(string())),
                                (
                                    "not_calculated_messages",
                                    json!({ "type": "object", "additionalProperties": string() }),
                                ),
                            ],
                            &["subject", "dose", "route", "outcome"],
                        ))),
                    ],
                    &["kind", "subjects"],
                ),
                open_object(
                    vec![
                        ("kind", json!({ "const": "fit" })),
                        ("subject", string()),
                        ("dose", nullable(number())),
                        ("n_observations", integer()),
                        ("n_missing", integer()),
                        ("flag_messages", array_of(string())),
                        ("status_message", string()),
                        ("outcome", outcome(reference("FitResult"))),
                    ],
                    &["kind", "subject", "n_observations", "n_missing", "outcome"],
                ),
                open_object(
                    vec![
                        ("kind", json!({ "const": "simulation" })),
                        ("times", array_of(number())),
                        ("outcome", outcome(reference("ModelOutput"))),
                    ],
                    &["kind", "times", "outcome"],
                ),
            ] })
        }
        "AnalysisSpec" => json!({ "type": "object", "required": ["kind"],
            "properties": { "kind": one_of_strings(&["nca", "fit", "simulation"]) } }),
        "AnalysisView" => object(
            vec![
                ("id", reference("Id")),
                ("label", string()),
                ("name", nullable(string())),
                ("kind", one_of_strings(&["nca", "fit", "simulation"])),
                ("spec", reference("AnalysisSpec")),
                ("status", reference("AnalysisStatus")),
                ("result", nullable(reference("AnalysisResult"))),
            ],
            &["id", "label", "name", "kind", "spec", "status", "result"],
        ),
        "UnitWarning" => object(
            vec![("code", string()), ("message", string())],
            &["code", "message"],
        ),
        _ => return None,
    })
}

// ---- a minimal validator, for the tests ---------------------------------------------------

#[cfg(test)]
pub(crate) mod validate {
    //! Just enough of JSON Schema to check the schemas of this crate against real values.

    use serde_json::Value;

    /// Checks `instance` against `schema` (whose `$defs` resolve `$ref`).
    pub(crate) fn check(root: &Value, schema: &Value, instance: &Value) -> Result<(), String> {
        walk(root, schema, instance, "$")
    }

    fn type_matches(name: &str, v: &Value) -> bool {
        match name {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            "number" => v.is_number(),
            "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|x| x.fract() == 0.0),
            _ => false,
        }
    }

    fn walk(root: &Value, schema: &Value, v: &Value, path: &str) -> Result<(), String> {
        let Some(s) = schema.as_object() else {
            return match schema {
                Value::Bool(true) => Ok(()),
                _ => Err(format!("{path}: schema is not an object")),
            };
        };
        if let Some(Value::String(r)) = s.get("$ref") {
            let name = r
                .strip_prefix("#/$defs/")
                .ok_or_else(|| format!("{path}: unsupported $ref {r}"))?;
            let target = root
                .pointer(&format!("/$defs/{name}"))
                .ok_or_else(|| format!("{path}: unresolved $ref {r}"))?;
            return walk(root, target, v, path);
        }
        if let Some(t) = s.get("type") {
            let ok = match t {
                Value::String(n) => type_matches(n, v),
                Value::Array(ns) => ns
                    .iter()
                    .any(|n| n.as_str().is_some_and(|n| type_matches(n, v))),
                _ => false,
            };
            if !ok {
                return Err(format!("{path}: expected type {t}, got {v}"));
            }
        }
        if let Some(Value::Array(options)) = s.get("enum") {
            if !options.contains(v) {
                return Err(format!("{path}: {v} is not one of {options:?}"));
            }
        }
        if let Some(c) = s.get("const") {
            if c != v {
                return Err(format!("{path}: expected {c}, got {v}"));
            }
        }
        if let Some(x) = v.as_f64() {
            let bound = |key: &str| s.get(key).and_then(Value::as_f64);
            if bound("minimum").is_some_and(|m| x < m) {
                return Err(format!("{path}: {x} below minimum"));
            }
            if bound("maximum").is_some_and(|m| x > m) {
                return Err(format!("{path}: {x} above maximum"));
            }
            if bound("exclusiveMinimum").is_some_and(|m| x <= m) {
                return Err(format!("{path}: {x} not above exclusive minimum"));
            }
            if bound("exclusiveMaximum").is_some_and(|m| x >= m) {
                return Err(format!("{path}: {x} not below exclusive maximum"));
            }
        }
        if let Some(text) = v.as_str() {
            let len = |key: &str| s.get(key).and_then(Value::as_u64);
            let n = text.chars().count() as u64;
            if len("minLength").is_some_and(|m| n < m) || len("maxLength").is_some_and(|m| n > m) {
                return Err(format!("{path}: length {n} out of range"));
            }
        }
        if let Some(items) = v.as_array() {
            let count = |key: &str| s.get(key).and_then(Value::as_u64);
            let n = items.len() as u64;
            if count("minItems").is_some_and(|m| n < m) || count("maxItems").is_some_and(|m| n > m)
            {
                return Err(format!("{path}: {n} items out of range"));
            }
            if let Some(item_schema) = s.get("items") {
                for (i, item) in items.iter().enumerate() {
                    walk(root, item_schema, item, &format!("{path}[{i}]"))?;
                }
            }
        }
        if let Some(map) = v.as_object() {
            let n = map.len() as u64;
            let count = |key: &str| s.get(key).and_then(Value::as_u64);
            if count("minProperties").is_some_and(|m| n < m)
                || count("maxProperties").is_some_and(|m| n > m)
            {
                return Err(format!("{path}: {n} properties out of range"));
            }
            if let Some(Value::Array(required)) = s.get("required") {
                for r in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(r) {
                        return Err(format!("{path}: missing required property `{r}`"));
                    }
                }
            }
            let props = s.get("properties").and_then(Value::as_object);
            for (k, item) in map {
                match props.and_then(|p| p.get(k)) {
                    Some(prop_schema) => walk(root, prop_schema, item, &format!("{path}.{k}"))?,
                    None => match s.get("additionalProperties") {
                        Some(Value::Bool(false)) => {
                            return Err(format!("{path}: unexpected property `{k}`"));
                        }
                        Some(extra @ Value::Object(_)) => {
                            walk(root, extra, item, &format!("{path}.{k}"))?;
                        }
                        _ => {}
                    },
                }
            }
        }
        if let Some(Value::Array(subs)) = s.get("allOf") {
            for sub in subs {
                walk(root, sub, v, path)?;
            }
        }
        if let Some(Value::Array(subs)) = s.get("anyOf") {
            if !subs.iter().any(|sub| walk(root, sub, v, path).is_ok()) {
                return Err(format!("{path}: {v} matches none of the alternatives"));
            }
        }
        if let Some(Value::Array(subs)) = s.get("oneOf") {
            let n = subs
                .iter()
                .filter(|sub| walk(root, sub, v, path).is_ok())
                .count();
            if n != 1 {
                return Err(format!(
                    "{path}: {v} matches {n} alternatives, expected exactly one"
                ));
            }
        }
        Ok(())
    }
}
