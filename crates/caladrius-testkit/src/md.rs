//! Loaders for the multiple-dosing and steady-state oracle (task T-047, open item OM-16):
//! `oracle/expected/models/md/`, from `oracle/scripts/models_md.R`.
//!
//! The files use the long format `subject,parameter,value` of the model cases (`crate::step3`), one
//! case per file pair, for the twelve ids `pk1.*` and `pk2.*`:
//!
//! - a **schedule** (`regimen.kind == "schedule"`): doses `(time, dose, dur)` in any order, overlaps
//!   allowed (`specs/models.md` MOD-MD-01); `conc` and `auc` at each time since the origin of the
//!   dose times, and the scalar `auc_inf` (the sum of the doses over CL);
//! - a **regular regimen** of `n_doses` equal doses at interval `tau` from time 0 (MOD-MD-07), the
//!   same quantities;
//! - a **steady state** (`regimen.kind == "steady_state"`) of interval `tau` (MOD-MD-03 to 06): `conc`,
//!   `auc` and `accum_c` at the times `s` since the last dose, 0 <= s <= tau, and the scalars
//!   `cmax_ss`, `tmax_ss`, `cmin_ss`, `cav_ss`, `auc_tau_ss`, `accum_cmax`, `accum_auc` (MOD-MD-08 to
//!   10; `tmax_ss` and `accum_c` are not-available rows where the quantity is not defined).
//!
//! `group` is `superposition`, `steady_state` or `convention` (the two convention points of OM-17 (e)
//! and (f), made explicit). The error suite `model_md_errors` is [`load_md_errors`].
//!
//! The directory is a subdirectory of `oracle/expected/models/` so that the one-compartment loaders
//! and the conformance table do not see it until the interface card wires it in
//! (`cargo xtask conformance` needs `list_md_cases` and an evaluation of `caladrius-models`).

use std::path::PathBuf;

use serde::Deserialize;

use crate::oracle::{OracleError, oracle_dir};
use crate::pk2::{Pk2ErrorCase, load_error_suite};
use crate::step3::{ModelCase, json, load_model_case_in, names_in};

/// Name of the error suite (`model_md_errors.csv` and `.options.json`).
pub const MD_ERRORS: &str = "model_md_errors";

/// `oracle/expected/models/md/`.
pub fn md_dir() -> PathBuf {
    oracle_dir().join("expected").join("models").join("md")
}

/// The kind of regimen of a case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegimenKind {
    /// Doses `(time, dose, dur)` listed in `records`.
    Schedule,
    /// `n_doses` equal doses at interval `tau` starting at time 0.
    Regular,
    /// Steady state of interval `tau`; times are since the last dose.
    SteadyState,
}

/// One dose of a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct DoseRecord {
    pub time: f64,
    pub dose: f64,
    /// Duration of the input, for the infusion and zero-order models.
    #[serde(default)]
    pub dur: Option<f64>,
}

/// The regimen of a case.
#[derive(Debug, Clone, PartialEq)]
pub struct Regimen {
    pub kind: RegimenKind,
    pub tau: Option<f64>,
    pub n_doses: Option<u32>,
    /// The records of a schedule, in the order of the file (not necessarily sorted).
    pub records: Vec<DoseRecord>,
}

/// The printed form of S-36 for the interval before the end of the lag, evaluated by the oracle
/// script at the times of the case that lie at or below the lag (OM-17 (e)).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PrintedForm {
    /// True when at least one time of the case has a printed value that differs from the oracle's.
    pub differs_from_oracle: bool,
    pub times_that_differ: Vec<f64>,
    pub printed_values: Vec<f64>,
    pub oracle_values: Vec<f64>,
}

/// One multiple-dosing case: the common model case plus the regimen and what only these cases carry.
#[derive(Debug, Clone, PartialEq)]
pub struct MdCase {
    /// Model id, nominal dose, model parameters (`v`, `cl`, or `cl, vc, q, vp`, or the macro set,
    /// plus `ka`, `dur`, `tlag` as the id needs them; a schedule of an input with a duration carries
    /// the duration in each record and has no `dur`), time grid and expected values.
    pub case: ModelCase,
    /// `superposition`, `steady_state` or `convention`.
    pub group: String,
    pub regimen: Regimen,
    /// `clearance` or `macro`.
    pub parameterisation: String,
    /// True when the textbook forms in double precision keep their digits for this case.
    pub textbook_double_ok: bool,
    /// Steady state: true when the maximum is attained at every s (a continuous infusion), so that
    /// `tmax_ss` is a not-available row.
    pub tmax_not_unique: bool,
    /// Steady state with a lag: the printed branch of S-36 against the oracle.
    pub printed_form: Option<PrintedForm>,
}

#[derive(Debug, Deserialize)]
struct RawRegimen {
    kind: String,
    #[serde(default)]
    tau: Option<f64>,
    #[serde(default)]
    n_doses: Option<u32>,
    #[serde(default)]
    records: Vec<DoseRecord>,
}

#[derive(Debug, Default, Deserialize)]
struct RawSteady {
    #[serde(default)]
    tmax_ss_is_not_unique: bool,
}

#[derive(Debug, Default, Deserialize)]
struct RawChecks {
    #[serde(default)]
    steady_state: RawSteady,
    #[serde(default)]
    s36_printed_form: Option<PrintedForm>,
}

#[derive(Debug, Deserialize)]
struct Extra {
    kind: String,
    group: String,
    parameterisation: String,
    regimen: RawRegimen,
    #[serde(default)]
    textbook_double_ok: bool,
    #[serde(default)]
    results_of_checks: RawChecks,
}

/// Names of the value cases of the multiple-dosing oracle, sorted (the error suite is [`MD_ERRORS`]).
pub fn list_md_cases() -> Result<Vec<String>, OracleError> {
    let mut names = names_in(&md_dir())?;
    names.retain(|n| n != MD_ERRORS);
    Ok(names)
}

/// Loads one case and checks that its files agree.
pub fn load_md_case(name: &str) -> Result<MdCase, OracleError> {
    let dir = md_dir();
    let case = load_model_case_in(&dir, name)?;
    let path = dir.join(format!("{name}.options.json"));
    let extra: Extra = json(&path)?;
    let here = path.display().to_string();
    let inconsistent = |message: String| OracleError::Inconsistent {
        path: here.clone(),
        message,
    };
    if extra.kind != "model_md" {
        return Err(inconsistent(format!("unknown kind {:?}", extra.kind)));
    }
    let kind = match extra.regimen.kind.as_str() {
        "schedule" => RegimenKind::Schedule,
        "regular" => RegimenKind::Regular,
        "steady_state" => RegimenKind::SteadyState,
        other => return Err(inconsistent(format!("unknown regimen kind {other:?}"))),
    };
    let r = &extra.regimen;
    let well_formed = match kind {
        RegimenKind::Schedule => !r.records.is_empty() && r.tau.is_none() && r.n_doses.is_none(),
        RegimenKind::Regular => r.tau.is_some() && r.n_doses.is_some_and(|n| n >= 1),
        RegimenKind::SteadyState => r.tau.is_some() && r.n_doses.is_none() && r.records.is_empty(),
    };
    if !well_formed {
        return Err(inconsistent(format!("regimen {:?} is incomplete", r.kind)));
    }
    Ok(MdCase {
        case,
        group: extra.group,
        regimen: Regimen {
            kind,
            tau: extra.regimen.tau,
            n_doses: extra.regimen.n_doses,
            records: extra.regimen.records,
        },
        parameterisation: extra.parameterisation,
        textbook_double_ok: extra.textbook_double_ok,
        tmax_not_unique: extra.results_of_checks.steady_state.tmax_ss_is_not_unique,
        printed_form: extra.results_of_checks.s36_printed_form,
    })
}

/// Loads the error suite `model_md_errors` and checks that its table lists exactly its cases, all
/// not available. The regimen of a case is encoded in its `parameters` (`tau`, `n_doses`,
/// `dose_time[i]`, `dose_amount[i]`, `dose_dur[i]`, `i` from 0), the same names as
/// [`dose_parameters`].
pub fn load_md_errors() -> Result<Vec<Pk2ErrorCase>, OracleError> {
    load_error_suite(&md_dir(), MD_ERRORS)
}

/// The regimen of a case as named parameters, the provisional encoding of the oracle files (the
/// engine card may replace it by a proper type; the one place to change in the tests is the helper
/// that calls this): `tau` and `n_doses` for a regular regimen, `tau` alone for a steady state,
/// `dose_time[i]`, `dose_amount[i]` and, when the input has a duration, `dose_dur[i]` (`i` from 0) for
/// a schedule.
pub fn dose_parameters(regimen: &Regimen) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    match regimen.kind {
        RegimenKind::SteadyState => out.extend(regimen.tau.map(|t| ("tau".to_string(), t))),
        RegimenKind::Regular => {
            out.extend(regimen.tau.map(|t| ("tau".to_string(), t)));
            out.extend(
                regimen
                    .n_doses
                    .map(|n| ("n_doses".to_string(), f64::from(n))),
            );
        }
        RegimenKind::Schedule => {
            for (i, r) in regimen.records.iter().enumerate() {
                out.push((format!("dose_time[{i}]"), r.time));
                out.push((format!("dose_amount[{i}]"), r.dose));
                if let Some(d) = r.dur {
                    out.push((format!("dose_dur[{i}]"), d));
                }
            }
        }
    }
    out
}
