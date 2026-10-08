//! Units as data (golden rule 5, `AGENTS.md` section 7 friction 5). A unit is free text on a
//! column; this module recognises the common ones to check dose, concentration and time together
//! and to name the derived units (AUC, CL, V). It never converts a value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A check that failed or could not be made; shown before running an analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitWarning {
    /// Stable code: `missing_unit`, `unrecognised_unit`, `mass_mismatch`, `dose_per_body_size`.
    pub code: String,
    /// What to check or fix.
    pub message: String,
}

fn warning(code: &str, message: String) -> UnitWarning {
    UnitWarning {
        code: code.to_owned(),
        message,
    }
}

fn normalise(unit: &str) -> String {
    unit.trim()
        .replace(['\u{b5}', '\u{3bc}'], "u")
        .replace(' ', "")
}

/// Grams in one unit of mass, for the units written as a prefix and `g`.
fn mass_in_grams(unit: &str) -> Option<f64> {
    match unit {
        "kg" => Some(1e3),
        "g" => Some(1.0),
        "mg" => Some(1e-3),
        "ug" | "mcg" => Some(1e-6),
        "ng" => Some(1e-9),
        "pg" => Some(1e-12),
        _ => None,
    }
}

/// Litres in one unit of volume.
fn volume_in_litres(unit: &str) -> Option<f64> {
    match unit {
        "L" | "l" => Some(1.0),
        "dL" | "dl" => Some(1e-1),
        "mL" | "ml" => Some(1e-3),
        "uL" | "ul" => Some(1e-6),
        _ => None,
    }
}

/// Seconds in one unit of time.
fn time_in_seconds(unit: &str) -> Option<f64> {
    match unit {
        "s" | "sec" => Some(1.0),
        "min" => Some(60.0),
        "h" | "hr" | "hour" | "hours" => Some(3600.0),
        "d" | "day" | "days" => Some(86400.0),
        _ => None,
    }
}

/// (mass unit, volume unit) of a concentration such as `ng/mL`.
fn split_concentration(unit: &str) -> Option<(&str, &str)> {
    let (mass, volume) = unit.split_once('/')?;
    mass_in_grams(mass)?;
    volume_in_litres(volume)?;
    Some((mass, volume))
}

/// The checks on the units of the time, concentration and dose columns of one worksheet.
/// `None` is a column without a unit (or no such column).
pub fn unit_warnings(
    time: Option<&str>,
    concentration: Option<&str>,
    dose: Option<&str>,
) -> Vec<UnitWarning> {
    let mut out = Vec::new();
    let time = time.map(normalise).filter(|u| !u.is_empty());
    let conc = concentration.map(normalise).filter(|u| !u.is_empty());
    let dose = dose.map(normalise).filter(|u| !u.is_empty());
    for (what, unit) in [("time", &time), ("concentration", &conc), ("dose", &dose)] {
        if unit.is_none() {
            out.push(warning(
                "missing_unit",
                format!(
                    "the {what} has no unit; derived units (AUC, clearance, volume) cannot be named"
                ),
            ));
        }
    }
    if let Some(t) = &time {
        if time_in_seconds(t).is_none() {
            out.push(warning(
                "unrecognised_unit",
                format!("the time unit `{t}` is not recognised (s, min, h, d); consistency cannot be checked"),
            ));
        }
    }
    let conc_parts = conc.as_deref().and_then(split_concentration);
    if let Some(c) = &conc {
        if conc_parts.is_none() {
            out.push(warning(
                "unrecognised_unit",
                format!("the concentration unit `{c}` is not recognised (mass/volume such as ng/mL); consistency cannot be checked"),
            ));
        }
    }
    if let Some(d) = &dose {
        if d.contains("/kg") || d.contains("/m2") || d.contains("/m^2") {
            out.push(warning(
                "dose_per_body_size",
                format!("the dose unit `{d}` is per body size; give the absolute dose (for example mg) to compute clearance and volumes"),
            ));
        } else if mass_in_grams(d).is_none() {
            out.push(warning(
                "unrecognised_unit",
                format!("the dose unit `{d}` is not recognised (a mass such as mg); consistency cannot be checked"),
            ));
        } else if let Some((mass, _)) = conc_parts {
            if mass != d {
                out.push(warning(
                    "mass_mismatch",
                    format!("the dose is in {d} but the concentration is per {mass}; clearance and volumes will carry a conversion factor, convert one of them first"),
                ));
            }
        }
    }
    out
}

/// Names of the derived units, from the units of the time, concentration and dose columns:
/// `auc`, `aumc`, `mrt`, `half_life`, `lambda_z`, and (when the dose unit is known) `cl` and `v`.
/// A missing unit leaves the corresponding names out; nothing is guessed.
pub fn derived_units(
    time: Option<&str>,
    concentration: Option<&str>,
    dose: Option<&str>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let time = time.map(normalise).filter(|u| !u.is_empty());
    let conc = concentration.map(normalise).filter(|u| !u.is_empty());
    let dose = dose.map(normalise).filter(|u| !u.is_empty());
    let mut put = |name: &str, value: String| {
        out.insert(name.to_owned(), value);
    };
    if let Some(t) = &time {
        put("mrt", t.clone());
        put("half_life", t.clone());
        put("lambda_z", format!("1/{t}"));
    }
    if let (Some(t), Some(c)) = (&time, &conc) {
        put("auc", format!("{t}*{c}"));
        put("aumc", format!("{t}^2*{c}"));
    }
    if let (Some(t), Some(c), Some(d)) = (&time, &conc, &dose) {
        match split_concentration(c) {
            Some((mass, volume)) if mass == d => {
                put("cl", format!("{volume}/{t}"));
                put("v", volume.to_owned());
            }
            _ => {
                put("cl", format!("{d}/({t}*{c})"));
                put("v", format!("{d}/({c})"));
            }
        }
    }
    out
}
