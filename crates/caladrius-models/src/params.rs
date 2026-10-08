//! Parameter sets: names, completeness and domains (MOD-GEN-03, GEN-04, VOC-01).

use std::collections::BTreeMap;

use crate::ModelError;
use crate::model::ModelId;

/// Every parameter name a model may take.
const KNOWN: [&str; 6] = ["v", "cl", "k", "ka", "dur", "tlag"];

/// A checked parameter set; absent inputs are 0 (never read by a model without them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Params {
    pub v: f64,
    pub k: f64,
    pub cl: f64,
    pub ka: f64,
    pub dur: f64,
    pub tlag: f64,
}

fn out_of_domain(name: &str, value: f64, domain: &str) -> ModelError {
    ModelError::ParameterOutOfDomain {
        name: name.to_string(),
        value,
        domain: domain.to_string(),
    }
}

/// A finite number > 0 (or >= 0 when `zero_allowed`).
fn check(name: &str, value: f64, zero_allowed: bool) -> Result<f64, ModelError> {
    let ok = value.is_finite() && (value > 0.0 || (zero_allowed && value == 0.0));
    if ok {
        Ok(value)
    } else if zero_allowed {
        Err(out_of_domain(name, value, "a finite number >= 0"))
    } else {
        Err(out_of_domain(name, value, "a finite number > 0"))
    }
}

/// Checks the names against the model, then every domain.
pub(crate) fn resolve(model: ModelId, given: &BTreeMap<String, f64>) -> Result<Params, ModelError> {
    let id = model.id().to_string();
    let inputs = model.input_parameters();
    for name in given.keys() {
        if !KNOWN.contains(&name.as_str()) {
            return Err(ModelError::UnknownParameter {
                name: name.clone(),
                model: id,
            });
        }
        let allowed = matches!(name.as_str(), "v" | "cl" | "k") || inputs.contains(&name.as_str());
        if !allowed {
            return Err(ModelError::ParameterNotInModel {
                name: name.clone(),
                model: id,
            });
        }
    }
    let required = |name: &str| {
        given
            .get(name)
            .copied()
            .ok_or_else(|| ModelError::MissingParameter {
                name: name.to_string(),
                model: id.clone(),
            })
    };
    let v = check("v", required("v")?, false)?;
    let (cl, k) = match (given.get("cl"), given.get("k")) {
        (Some(_), Some(_)) => return Err(ModelError::ClearanceAndRateConstant),
        (None, None) => {
            return Err(ModelError::MissingParameter {
                name: "cl or k".to_string(),
                model: id,
            });
        }
        (Some(&cl), None) => {
            let cl = check("cl", cl, false)?;
            (cl, cl / v)
        }
        (None, Some(&k)) => {
            let k = check("k", k, false)?;
            (v * k, k)
        }
    };
    let input = |name: &str, zero_allowed: bool| -> Result<f64, ModelError> {
        if inputs.contains(&name) {
            check(name, required(name)?, zero_allowed)
        } else {
            Ok(0.0)
        }
    };
    let p = Params {
        v,
        k,
        cl,
        ka: input("ka", false)?,
        dur: input("dur", false)?,
        tlag: input("tlag", true)?,
    };
    // Derived values must be usable too (e.g. cl / v underflowing to 0).
    check("k", p.k, false)?;
    check("cl", p.cl, false)?;
    Ok(p)
}
