//! Loaders for the two-compartment oracle (task T-032): `oracle/expected/models/pk2/`.
//!
//! Three kinds of files share the long format `subject,parameter,value` of the one-compartment
//! model cases (`crate::step3`), each with an `.options.json`:
//!
//! - `model_pk2_<id>_<case>`: values of one of the six `pk2.*` ids on a time grid (`conc`, `auc`,
//!   `aumc` at `t=<time>`) and its scalars (`scalar`: the three parameter sets, half-lives,
//!   volumes, areas to infinity, MRT, Tmax and Cmax). [`Pk2Kind::Values`].
//! - `model_pk2_deriv_<id>_<case>`: partial derivatives of the concentration, `d_<parameter>` at
//!   `t=<time>`, in the parameter set of the case. [`Pk2Kind::Derivatives`].
//! - `model_pk2_errors`: the inputs the engine must refuse, one not-available row per case, the
//!   reason and the words of the message in the options. [`load_pk2_errors`].
//!
//! The files are in a subdirectory so that the one-compartment loaders and the conformance table
//! (`cargo xtask conformance`, which compares `list_model_cases` with `caladrius-models`) are not
//! affected until the engine and the interface cards for two compartments land.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::oracle::{OracleError, oracle_dir, parse_expected};
use crate::step3::{ModelCase, json, load_model_case_in, names_in, read};

/// Name of the error suite (`model_pk2_errors.csv` and `.options.json`).
pub const PK2_ERRORS: &str = "model_pk2_errors";

/// `oracle/expected/models/pk2/`.
pub fn pk2_dir() -> PathBuf {
    oracle_dir().join("expected").join("models").join("pk2")
}

/// What a two-compartment case holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pk2Kind {
    /// `conc`, `auc`, `aumc` on a grid and the `scalar` rows.
    Values,
    /// `d_<parameter>` on a grid.
    Derivatives,
}

/// One two-compartment case: the common model case plus what only these cases carry.
#[derive(Debug, Clone, PartialEq)]
pub struct Pk2Case {
    /// Model id (`pk2.iv_bolus`, ...), dose, parameters (of `parameterisation`, plus `ka`, `dur`,
    /// `tlag`), time grid and expected values.
    pub case: ModelCase,
    pub kind: Pk2Kind,
    /// `clearance` (cl, vc, q, vp), `micro` (k10, k12, k21, vc) or `macro` (a, b, alpha, beta).
    pub parameterisation: String,
    /// True when the textbook forms in double precision keep their digits for this case (exponents
    /// not close to each other, `ka` not close to an exponent): the naive implementation of the
    /// testkit can then be compared with the expected values.
    pub textbook_double_ok: bool,
}

#[derive(Debug, Deserialize)]
struct Extra {
    kind: String,
    parameterisation: String,
    #[serde(default)]
    textbook_double_ok: bool,
}

/// Names of the value and derivative cases of the two-compartment oracle, sorted (the error suite
/// is [`PK2_ERRORS`]).
pub fn list_pk2_cases() -> Result<Vec<String>, OracleError> {
    let mut names = names_in(&pk2_dir())?;
    names.retain(|n| n != PK2_ERRORS);
    Ok(names)
}

/// Loads one value or derivative case and checks that its files agree.
pub fn load_pk2_case(name: &str) -> Result<Pk2Case, OracleError> {
    let dir = pk2_dir();
    let case = load_model_case_in(&dir, name)?;
    let path = dir.join(format!("{name}.options.json"));
    let extra: Extra = json(&path)?;
    let kind = match extra.kind.as_str() {
        "model" => Pk2Kind::Values,
        "model_derivatives" => Pk2Kind::Derivatives,
        other => {
            return Err(OracleError::Inconsistent {
                path: path.display().to_string(),
                message: format!("unknown kind {other:?}"),
            });
        }
    };
    Ok(Pk2Case {
        case,
        kind,
        parameterisation: extra.parameterisation,
        textbook_double_ok: extra.textbook_double_ok,
    })
}

/// A number of the error suite: JSON has no NaN or infinity, so those are the strings `"NaN"`,
/// `"Infinity"` and `"-Infinity"`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Number {
    Finite(f64),
    Text(String),
}

fn number(n: Number, path: &str) -> Result<f64, OracleError> {
    match n {
        Number::Finite(x) => Ok(x),
        Number::Text(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            other => Err(OracleError::Inconsistent {
                path: path.to_string(),
                message: format!(
                    "{other:?} is not a number (only NaN, Infinity, -Infinity are text)"
                ),
            }),
        },
    }
}

#[derive(Debug, Deserialize)]
struct RawError {
    id: String,
    group: String,
    spec: String,
    model: String,
    dose: Number,
    parameters: BTreeMap<String, Number>,
    times: Vec<Number>,
    message_contains: Vec<Vec<String>>,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct RawErrors {
    case: String,
    cases: Vec<RawError>,
    n_values: usize,
}

/// One input the engine must refuse.
#[derive(Debug, Clone)]
pub struct Pk2ErrorCase {
    pub id: String,
    /// `domain`, `one_compartment`, `macro`, `sets`, `numeric` or `times`.
    pub group: String,
    /// The rule of `specs/models.md` that refuses it.
    pub spec: String,
    pub model: String,
    pub dose: f64,
    /// Parameters by name; a NaN or infinite value is a real `f64::NAN` or `f64::INFINITY`.
    pub parameters: BTreeMap<String, f64>,
    pub times: Vec<f64>,
    /// For each inner list, the lower-cased message must contain at least one of its words.
    pub message_contains: Vec<Vec<String>>,
    pub reason: String,
}

/// Loads the error suite and checks that its table lists exactly its cases, all not available.
pub fn load_pk2_errors() -> Result<Vec<Pk2ErrorCase>, OracleError> {
    load_error_suite(&pk2_dir(), PK2_ERRORS)
}

/// Loads the error suite `name` of `dir` (`<name>.options.json` and `<name>.csv`) and checks that its
/// table lists exactly its cases, all not available. The multiple-dosing suite of `crate::md` uses the
/// same layout.
pub(crate) fn load_error_suite(
    dir: &Path,
    name: &str,
) -> Result<Vec<Pk2ErrorCase>, OracleError> {
    let options_path = dir.join(format!("{name}.options.json"));
    let here = options_path.display().to_string();
    let raw: RawErrors = json(&options_path)?;
    let inconsistent = |message: String| OracleError::Inconsistent {
        path: here.clone(),
        message,
    };
    if raw.case != name {
        return Err(inconsistent(format!("case field is {:?}", raw.case)));
    }
    if raw.n_values != raw.cases.len() {
        return Err(inconsistent(format!(
            "options say {} values, they list {} cases",
            raw.n_values,
            raw.cases.len()
        )));
    }
    let csv_path = dir.join(format!("{name}.csv"));
    let table = parse_expected(&csv_path.display().to_string(), &read(&csv_path)?)?;
    if table.len() != raw.cases.len() {
        return Err(inconsistent(format!(
            "the table has {} rows for {} cases",
            table.len(),
            raw.cases.len()
        )));
    }
    let mut out = Vec::new();
    for c in raw.cases {
        if table.get(&c.id, "result") != Some(None) {
            return Err(inconsistent(format!(
                "case {:?} is not a not-available row of the table",
                c.id
            )));
        }
        let mut parameters = BTreeMap::new();
        for (k, v) in c.parameters {
            parameters.insert(k, number(v, &here)?);
        }
        let times = c
            .times
            .into_iter()
            .map(|t| number(t, &here))
            .collect::<Result<Vec<_>, _>>()?;
        out.push(Pk2ErrorCase {
            id: c.id,
            group: c.group,
            spec: c.spec,
            model: c.model,
            dose: number(c.dose, &here)?,
            parameters,
            times,
            message_contains: c.message_contains,
            reason: c.reason,
        });
    }
    Ok(out)
}
