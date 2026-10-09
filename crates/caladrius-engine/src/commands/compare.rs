//! `analysis.compare`: what two analyses say about the parameters they share. The model of an
//! agent must never subtract or divide two results itself (benchmark of 2026-10-09: every
//! unverified number came from that), so the engine does the three operations, with the unit
//! checks, and says why when it cannot.
//!
//! A value is compared only with a value in the same unit; no unit is ever converted. Nothing is
//! rounded here: the numbers are those of the results, and `difference = b - a`,
//! `relative_percent = (b - a) / a * 100`, `ratio = b / a`.

use caladrius_project::{
    AnalysisId, AnalysisResult, AnalysisStatus, ColumnRole, Project, derived_units,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CommandDef, parse, respond};
use crate::Engine;
use crate::error::CommandError;
use crate::schema::root;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompareParams {
    a: AnalysisId,
    b: AnalysisId,
    #[serde(default)]
    parameters: Vec<String>,
}

/// One parameter of one analysis.
struct Entry {
    name: String,
    value: Option<f64>,
    /// `None`: the unit is not known.
    unit: Option<String>,
    /// Why there is no value.
    missing: Option<String>,
}

/// An analysis as the comparison reads it.
struct Side {
    info: Value,
    entries: Vec<Entry>,
}

/// The name a parameter is compared under: the spelling of the NCA parameters, so that the fit
/// results (`half_life`, `cl`, `auc_inf`) and the NCA results (`half.life`, `cl.obs`,
/// `aucinf.obs`) meet. Other names are kept as they are.
fn canonical(name: &str) -> &str {
    match name {
        "half_life" => "half.life",
        "cl" => "cl.obs",
        "auc_inf" => "aucinf.obs",
        other => other,
    }
}

/// The unit of a fitted parameter, from the units of the worksheet columns (the fit result
/// carries none). `None` when the worksheet lacks the unit the parameter needs.
fn fit_unit(name: &str, units: &std::collections::BTreeMap<String, String>) -> Option<String> {
    let key = match name {
        // `mrt` is a time, whatever else it is.
        "half_life" | "tlag" | "dur" => "mrt",
        "k" | "ka" | "k10" | "k12" | "k21" => "lambda_z",
        "v" | "vc" | "vp" => "v",
        "cl" | "q" => "cl",
        "auc_inf" => "auc",
        _ => return None,
    };
    units.get(key).cloned()
}

fn status_text(status: AnalysisStatus) -> &'static str {
    match status {
        AnalysisStatus::Fresh => "fresh",
        AnalysisStatus::Stale { .. } => "stale",
        AnalysisStatus::NoResult => "no_result",
    }
}

fn side(project: &Project, id: AnalysisId) -> Result<Side, CommandError> {
    let analysis = project.analysis(id)?;
    let status = project.status(id)?;
    let result = analysis.result().ok_or_else(|| {
        CommandError::new(
            "no_result",
            format!("analysis {id} has not been run; run it before comparing it"),
        )
    })?;
    let mut info = json!({
        "analysis": id,
        "label": project.label_of(id)?,
        "kind": analysis.spec().kind(),
        "subject": Value::Null,
        "status": status_text(status),
    });
    let entries = match result {
        AnalysisResult::Nca { subjects } => {
            let [only] = subjects.as_slice() else {
                let names: Vec<&str> = subjects.iter().map(|s| s.subject.as_str()).collect();
                return Err(CommandError::new(
                    "ambiguous_subject",
                    format!(
                        "analysis {id} has {} subjects ({}); compare one subject: run nca.run with `subject`",
                        names.len(),
                        names.join(", ")
                    ),
                ));
            };
            info["subject"] = json!(only.subject);
            let ok = only.outcome.ok().ok_or_else(|| {
                CommandError::new(
                    "no_result",
                    format!(
                        "analysis {id} failed for subject {}; see its error",
                        only.subject
                    ),
                )
            })?;
            ok.parameters()
                .iter()
                .map(|p| Entry {
                    name: p.name.clone(),
                    value: p.value.value(),
                    unit: p.unit.clone(),
                    missing: p.value.reason().map(|r| r.to_string()),
                })
                .collect()
        }
        AnalysisResult::Fit(run) => {
            info["subject"] = json!(run.subject);
            let fit = run.outcome.ok().ok_or_else(|| {
                CommandError::new(
                    "no_result",
                    format!("the fit of analysis {id} did not produce a result; see its error"),
                )
            })?;
            let units = analysis
                .spec()
                .worksheet()
                .and_then(|w| project.worksheet(w).ok())
                .map(|ws| {
                    derived_units(
                        ws.unit_of(ColumnRole::Time),
                        ws.unit_of(ColumnRole::Concentration),
                        ws.unit_of(ColumnRole::Dose),
                    )
                })
                .unwrap_or_default();
            let mut names: Vec<&str> = fit.parameters().iter().map(String::as_str).collect();
            names.extend(["cl", "half_life", "auc_inf"]);
            let mut entries: Vec<Entry> = Vec::new();
            for name in names {
                let key = canonical(name);
                if entries.iter().any(|e| e.name == key) {
                    continue;
                }
                let Some(value) = fit.get(&format!("estimate.{name}")) else {
                    continue;
                };
                entries.push(Entry {
                    name: key.to_owned(),
                    value: Some(value),
                    unit: fit_unit(name, &units),
                    missing: None,
                });
            }
            entries
        }
        AnalysisResult::Simulation(_) => {
            return Err(CommandError::new(
                "wrong_kind",
                format!("analysis {id} is a simulation; compare NCA or fit results"),
            ));
        }
    };
    Ok(Side { info, entries })
}

/// Units compare equal when they are the same up to the spelling of the product dot and of micro.
fn same_unit(a: &str, b: &str) -> bool {
    let plain = |u: &str| {
        u.replace('\u{b7}', "*")
            .replace(' ', "")
            .replace(['\u{b5}', '\u{3bc}'], "u")
    };
    plain(a) == plain(b)
}

fn find<'a>(side: &'a Side, name: &str) -> Option<&'a Entry> {
    side.entries.iter().find(|e| e.name == name)
}

/// Why `entry` has no value, in a sentence naming the analysis.
fn why_missing(which: &str, entry: Option<&Entry>) -> String {
    match entry {
        None => format!("analysis {which} has no such parameter"),
        Some(e) => match &e.missing {
            Some(reason) => format!("analysis {which}: not calculated, {reason}"),
            None => format!("analysis {which}: not calculated"),
        },
    }
}

/// One row of the comparison.
fn row(name: &str, a: Option<&Entry>, b: Option<&Entry>) -> Value {
    let value = |e: Option<&Entry>| e.and_then(|e| e.value);
    let unit = |e: Option<&Entry>| e.and_then(|e| e.unit.clone());
    let mut out = json!({
        "parameter": name,
        "a": value(a),
        "b": value(b),
        "unit_a": unit(a),
        "unit_b": unit(b),
        "difference": Value::Null,
        "difference_unit": Value::Null,
        "relative_percent": Value::Null,
        "ratio": Value::Null,
        "not_comparable": Value::Null,
    });
    let reason = |text: String, out: &mut Value| out["not_comparable"] = json!(text);
    let (Some(x), Some(y)) = (value(a), value(b)) else {
        let text = match (value(a), value(b)) {
            (None, None) => format!("{}; {}", why_missing("a", a), why_missing("b", b)),
            (None, _) => why_missing("a", a),
            _ => why_missing("b", b),
        };
        reason(text, &mut out);
        return out;
    };
    let (ua, ub) = (unit(a), unit(b));
    match (&ua, &ub) {
        (Some(p), Some(q)) if !same_unit(p, q) => {
            reason(
                format!("unit mismatch: a is in `{p}`, b is in `{q}`; no unit is converted"),
                &mut out,
            );
            return out;
        }
        (Some(_), None) | (None, Some(_)) => {
            reason(
                format!(
                    "the unit of {} is not known, so the two values may not be in the same unit",
                    if ua.is_none() { "a" } else { "b" }
                ),
                &mut out,
            );
            return out;
        }
        _ => {}
    }
    let difference = y - x;
    if !difference.is_finite() {
        reason("the difference overflowed".to_owned(), &mut out);
        return out;
    }
    out["difference"] = json!(difference);
    out["difference_unit"] = json!(ua);
    if x == 0.0 {
        reason(
            "a is zero: the relative difference and the ratio are undefined".to_owned(),
            &mut out,
        );
        return out;
    }
    let (relative, ratio) = (difference / x * 100.0, y / x);
    if relative.is_finite() && ratio.is_finite() {
        out["relative_percent"] = json!(relative);
        out["ratio"] = json!(ratio);
    } else {
        reason(
            "the relative difference or the ratio overflowed".to_owned(),
            &mut out,
        );
    }
    out
}

fn analysis_compare(engine: &mut Engine, params: Value) -> Result<Value, CommandError> {
    let p: CompareParams = parse("analysis.compare", params)?;
    let (a, b) = (side(&engine.project, p.a)?, side(&engine.project, p.b)?);
    let names: Vec<String> = if p.parameters.is_empty() {
        // Every parameter present in both, in the order of a.
        a.entries
            .iter()
            .filter(|e| find(&b, &e.name).is_some())
            .map(|e| e.name.clone())
            .collect()
    } else {
        let mut names: Vec<String> = Vec::new();
        for requested in &p.parameters {
            let name = canonical(requested).to_owned();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    };
    let rows: Vec<Value> = names
        .iter()
        .map(|n| row(n, find(&a, n), find(&b, n)))
        .collect();
    respond(&json!({ "a": a.info, "b": b.info, "rows": rows }))
}

pub(crate) const ANALYSIS_COMPARE: CommandDef = CommandDef {
    id: "analysis.compare",
    title: "Compare analyses",
    description: "Differences and ratios of shared parameters.",
    mutates: false,
    // The schemas are as small as the contract allows: the MCP server sends them to the model
    // with every conversation (T-042: the tool adds under 600 bytes to `tools/list`).
    params: || {
        root(
            "analysis.compare",
            json!({
                "type": "object",
                "properties": {
                    "a": { "type": "integer" },
                    "b": { "type": "integer" },
                    "parameters": { "type": "array" },
                },
                "required": ["a", "b"],
            }),
        )
    },
    result: || root("analysis.compare", json!({ "type": "object" })),
    example: || json!({ "a": 2, "b": 3 }),
    run: analysis_compare,
};
