//! The population fit (`specs/population.md`): FOCEI (`foce_i`, the default, POP-OBJ-02), plain
//! FOCE with σ frozen at the population prediction (`foce`, POP-OBJ-06) and FO (`fo`, POP-OBJ-03),
//! in the conventions `full` and `no_2pi` (POP-OBJ-05), with the conditional modes η̂ of every
//! subject (POP-INN-01, POP-INN-02) and the refusals of POP-INN-03 that are decided before any
//! outer iteration.
//!
//! With `options.evaluate_only = true` the objective, the modes and the diagnostics are returned at
//! the given parameters. Otherwise the outer fit of POP-EST-01 to POP-EST-05 runs ([`outer`]), with
//! the covariance of POP-COV-01 to POP-COV-04 ([`covariance`]).
//!
//! The model `identity_linear` (POP-MOD-05) is a test model: it is reachable through this API only,
//! is not in the catalogue of `caladrius-models` and is not named in any message of this module.

mod checks;
mod covariance;
mod data;
mod dense;
mod diagnostics;
mod evaluation;
mod inner;
#[cfg(test)]
mod measure;
mod outer;
mod phi;
mod start;
mod structure;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use caladrius_models::ModelId;
use serde::{Deserialize, Serialize};

use crate::linalg::Mat;
use crate::model::FitModel;
use crate::{Bounds, ErrorModel};

use checks::LagDomain;
use data::Subject;
use evaluation::{Evaluation, Settings};
use phi::{Holds, Layout, Params};
use structure::{Structure, SubjectModel};

pub use covariance::{CovarianceBlock, Kind as CovarianceKind};

// ---------------------------------------------------------------- the request

/// A population request (POP-API-01): the vocabulary of `oracle/expected/pop/errors.json` and of the
/// header of `crates/caladrius-fit/tests/oracle_pop.rs`. Words that select a behaviour are read as
/// text, so that a wrong word is refused with a message that names the option.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopRequest {
    /// A `pk1.*` or `pk2.*` model id.
    pub model: Option<String>,
    /// Typical values by parameter (the free typical values).
    #[serde(default)]
    pub initial: BTreeMap<String, f64>,
    /// The parameters that carry a random effect, in the order of the rows of Ω.
    pub random_effects: Option<Vec<String>>,
    /// Ω, q × q.
    pub omega: Option<Vec<Vec<f64>>>,
    /// `diagonal` (the default) or `block`.
    pub omega_shape: Option<String>,
    /// `additive`, `proportional`, `combined` or `combined_sd`; required.
    pub error_model: Option<String>,
    /// The error parameters in the user's units: `[a]`, `[b]` or `[a, b]` (FIT-ML-21).
    pub error_init: Option<Vec<f64>>,
    /// Options of the model (the test model's design).
    #[serde(default)]
    pub model_options: ModelOptions,
    /// Typical values held at a value (they are not counted as free), the error parameters `a`
    /// and `b` held at a value in the user's units, and `omega.<random effect>`, a variance of Ω
    /// held at a value (POP-EST-02: one ω_k of a diagonal Ω, or every variance of a block).
    #[serde(default)]
    pub fixed: BTreeMap<String, f64>,
    /// Bounds by typical value (FIT-BND-01, POP-EST-02); also read for the domain of `tlag`
    /// (POP-INN-03 item 1).
    #[serde(default)]
    pub bounds: BTreeMap<String, Bounds>,
    /// A table in the event layout in place of `subjects`; the host reads it into `dataset_text`.
    pub dataset_file: Option<String>,
    /// The text of the table named by `dataset_file`, set by the host (not part of the JSON).
    #[serde(skip)]
    pub dataset_text: Option<String>,
    /// Options of the run.
    #[serde(default)]
    pub options: PopOptions,
    /// The subjects, inline.
    pub subjects: Option<Vec<SubjectInput>>,
}

/// Options of the structural model.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelOptions {
    /// Z column k = t^z_powers[k] (POP-MOD-05).
    pub z_powers: Option<Vec<u32>>,
}

/// Options of a population run.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopOptions {
    /// `foce_i` (the default), `foce` or `fo`.
    pub method: Option<String>,
    /// `full` (the default) or `no_2pi`.
    pub ofv_convention: Option<String>,
    /// `sandwich` (the default), `r` or `s`: the kind that is printed (POP-COV-01).
    pub covariance: Option<String>,
    /// Labels of subjects left out of the run.
    #[serde(default)]
    pub exclude_subjects: Vec<String>,
    /// The objective and the modes at the given parameters, with no outer iteration.
    #[serde(default)]
    pub evaluate_only: bool,
}

/// One subject, inline: the times are measured from the dose (POP-DAT-01).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectInput {
    pub id: String,
    pub dose: f64,
    pub time: Vec<f64>,
    /// `null` for a missing observation.
    pub y: Vec<Option<f64>>,
    /// The duration of a zero-order input (POP-API-01 item 11, the name is `assumed`).
    pub duration: Option<f64>,
}

// ---------------------------------------------------------------- the answer

/// The method of the objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// FOCE with interaction (POP-OBJ-02), the default.
    FoceI,
    /// FOCE without interaction, σ frozen at the population prediction (POP-OBJ-06).
    Foce,
    /// First order, η̂ = 0 (POP-OBJ-03).
    Fo,
}

/// The constant of the objective (POP-OBJ-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OfvConvention {
    /// The constant N·ln(2π) kept (the default).
    #[serde(rename = "full")]
    Full,
    /// OFV_full − N·ln(2π).
    #[serde(rename = "no_2pi")]
    No2Pi,
}

/// The answer of an evaluation or of a fit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PopAnswer {
    /// `evaluated` (POP-API-01 item 6), or the status of the fit (POP-EST-03).
    pub status: String,
    pub method: Method,
    pub ofv_convention: OfvConvention,
    /// The total objective, the per-subject values summed in dataset order.
    pub ofv: f64,
    /// Subjects with at least one observation used.
    pub n_subjects: usize,
    /// Observations used, all subjects.
    pub n_observations: usize,
    pub random_effects: Vec<String>,
    /// The error parameters the evaluation used, in the user's units, by key (`a`, `b`), with where
    /// each value comes from. Not the estimate of a fit (that is `estimate.xi`, T-154).
    pub xi_used: BTreeMap<String, ErrorValue>,
    /// Every subject of the run, in dataset order.
    pub subjects: Vec<SubjectAnswer>,
    /// η-shrinkage per random effect (POP-DIA-03); null where NC.
    pub eta_shrinkage: Vec<Option<f64>>,
    /// ε-shrinkage (POP-DIA-03); null where NC.
    pub eps_shrinkage: Option<f64>,
    /// The number of free parameters (POP-DIA-04).
    pub k: usize,
    /// AIC and BIC (ln N) in the convention of the answer (POP-DIA-04); null when NC.
    pub aic: Option<f64>,
    pub bic: Option<f64>,
    /// A fit only: the estimates (POP-EST-04, POP-API-01 item 13).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<Estimate>,
    /// A fit only: the standard errors of the printed kind, same keys (POP-COV-04).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub se: Option<StandardErrors>,
    /// A fit only: the standard errors of the three kinds (`sandwich`, `r`, `s`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub se_by_kind: Option<BTreeMap<String, StandardErrors>>,
    /// A fit only: the covariance block (POP-EST-04).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub covariance: Option<CovarianceBlock>,
    /// A fit only: the accepted outer iterations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations: Option<usize>,
    /// A fit only: one row per accepted iteration, iteration 0 included (POP-EST-03).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<TraceAnswer>>,
    /// A fit only: where the start came from (POP-EST-05).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<StartRecord>,
}

/// The estimates of a fit, in the user's units.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Estimate {
    /// The typical values by name (the fixed ones included).
    #[serde(flatten)]
    pub theta: BTreeMap<String, f64>,
    /// The variances of Ω by random effect.
    pub omega_diagonal: BTreeMap<String, f64>,
    /// Ω, q × q, in the order of `random_effects` (exact zeros off a diagonal Ω).
    pub omega_full: Vec<Vec<f64>>,
    /// ω_k = √Ω_kk by random effect.
    pub omega_sd: BTreeMap<String, f64>,
    /// The correlations of Ω, q × q.
    pub omega_correlation: Vec<Vec<f64>>,
    /// The error parameters by key (`a`, `b`), FIT-ML-21.
    pub xi: BTreeMap<String, f64>,
}

/// Standard errors with the keys of [`Estimate`]; null where NC, fixed, on a bound, or held at 0.
/// The correlations of Ω have none in this version.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StandardErrors {
    #[serde(flatten)]
    pub theta: BTreeMap<String, Option<f64>>,
    pub omega_diagonal: BTreeMap<String, Option<f64>>,
    pub omega_full: Vec<Vec<Option<f64>>>,
    /// SE(ω_k) = SE(Ω_kk)/(2·ω_k), the delta method.
    pub omega_sd: BTreeMap<String, Option<f64>>,
    pub xi: BTreeMap<String, Option<f64>>,
}

/// One row of the trace of a fit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraceAnswer {
    pub iteration: usize,
    pub ofv: f64,
    pub ofv_convention: OfvConvention,
    /// The parameters in the user's units, by the labels of the covariance block (`omega[a,b]`,
    /// `xi.a`).
    pub parameters: BTreeMap<String, f64>,
    pub nu: Option<f64>,
    pub halvings: usize,
    pub decrement: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Where the start of a fit came from (POP-EST-05, as FIT-ML-20).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StartRecord {
    /// `user`, `two_stage` or `mixed`.
    pub kind: String,
    /// By parameter: `user`, `fixed`, `two_stage`, `error_init` or `moments`.
    pub sources: BTreeMap<String, String>,
    /// The subjects left out of the two-stage means (empty without a two-stage start).
    pub left_out: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One error parameter used by an evaluation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorValue {
    /// In the user's units (FIT-ML-21).
    pub value: f64,
    /// `fixed`, `error_init`, or, when the request gives neither, `two_stage` (POP-EST-05, a fit
    /// whose start ran the individual fits) or `moments` (FIT-ML-10 at the population prediction).
    pub source: String,
}

/// One subject of the answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SubjectAnswer {
    pub id: String,
    pub n_observations: usize,
    /// OFV_i in the convention of the answer (0 for a subject with no observation used).
    pub ofv: f64,
    /// η̂_i in the order of `random_effects`; null after FO and for a subject with no observation.
    pub eta_hat: Option<Vec<f64>>,
    /// Why `eta_hat` is null: `fo` or `no_observations`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_hat_reason: Option<String>,
    /// The Newton decrement Δ of the returned η̂ (convention of H37); null when there is no η̂.
    pub newton_decrement: Option<f64>,
}

// ---------------------------------------------------------------- the refusals

/// The codes of a refusal (POP-INN-03, POP-API-01 item 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopErrorCode {
    InvalidOption,
    UnsupportedHistory,
    UnboundedLikelihood,
    NonPositiveSigma,
    TooFewSubjects,
    TooFewObservations,
    InnerFailed,
}

impl PopErrorCode {
    /// The machine name.
    pub fn id(&self) -> &'static str {
        match self {
            PopErrorCode::InvalidOption => "invalid_option",
            PopErrorCode::UnsupportedHistory => "unsupported_history",
            PopErrorCode::UnboundedLikelihood => "unbounded_likelihood",
            PopErrorCode::NonPositiveSigma => "non_positive_sigma",
            PopErrorCode::TooFewSubjects => "too_few_subjects",
            PopErrorCode::TooFewObservations => "too_few_observations",
            PopErrorCode::InnerFailed => "inner_failed",
        }
    }
}

/// A refusal: a code and a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PopError {
    pub code: PopErrorCode,
    pub message: String,
}

impl fmt::Display for PopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.id(), self.message)
    }
}

impl std::error::Error for PopError {}

/// "option `{name}`: {what is wrong}; {what to give}" (POP-INN-03 item 5).
pub(crate) fn invalid(option: &str, what: impl fmt::Display) -> PopError {
    PopError {
        code: PopErrorCode::InvalidOption,
        message: format!("option `{option}`: {what}"),
    }
}

fn invalid_plain(message: String) -> PopError {
    PopError {
        code: PopErrorCode::InvalidOption,
        message,
    }
}

fn inner_failed(id: &str) -> PopError {
    PopError {
        code: PopErrorCode::InnerFailed,
        message: format!(
            "the inner problem of subject {id} did not converge at the starting values; check the starting values, the model and the data of that subject"
        ),
    }
}

/// FO needs G_i at η = 0 and the model cannot give it there (the code is that of POP-INN-03 item 4).
fn fo_derivatives_failed(id: &str) -> PopError {
    PopError {
        code: PopErrorCode::InnerFailed,
        message: format!(
            "the derivatives of the prediction of subject {id} with respect to its random effects are not finite at the typical values, and the method `fo` needs them there; check the typical values, the model and the data of that subject"
        ),
    }
}

/// C_i = G·Ω·Gᵀ + diag(σ²) is not positive definite, or OFV_i is not finite, at η̂ (at 0 for FO).
fn covariance_failed(id: &str, method: Method) -> PopError {
    let at = if method == Method::Fo {
        "at the typical values"
    } else {
        "at the conditional mode"
    };
    PopError {
        code: PopErrorCode::InnerFailed,
        message: format!(
            "the linearised covariance of the observations of subject {id} (G·Ω·Gᵀ + diag σ²) is not positive definite {at}, or the objective of that subject is not finite; check omega, the error parameters and the data of that subject"
        ),
    }
}

// ---------------------------------------------------------------- the run

/// Evaluates the population objective at the parameters of the request (`options.evaluate_only`),
/// or fits the population model (POP-EST-03).
pub fn run(request: &PopRequest) -> Result<PopAnswer, PopError> {
    run_with(request, &|_| false)
}

/// [`run`], with a hook that makes the inner problem of the named subjects fail at the starting
/// values (the unit test of `inner_failed`, POP-INN-03 item 4; never an option of the request).
pub(crate) fn run_with(
    request: &PopRequest,
    fail_inner: &dyn Fn(&str) -> bool,
) -> Result<PopAnswer, PopError> {
    let setup = Setup::new(request)?;
    if request.options.evaluate_only {
        setup.evaluate(fail_inner)
    } else {
        setup.fit(fail_inner)
    }
}

/// Everything the evaluation and the fit need, checked.
pub(crate) struct Setup {
    pub(crate) method: Method,
    pub(crate) convention: OfvConvention,
    pub(crate) error_model: ErrorModel,
    pub(crate) structure: Structure,
    pub(crate) random_effects: Vec<String>,
    pub(crate) subjects: Vec<Subject>,
    pub(crate) block: bool,
    /// The parameters given or computed for the start (θ with the fixed ones, L, moved ξ).
    pub(crate) start: Params,
    /// The error parameters used at the start, for the answer.
    pub(crate) xi_used: BTreeMap<String, ErrorValue>,
    /// The typical values held at their value.
    pub(crate) fixed_theta: Vec<String>,
    /// Bounds of the free typical values on the natural scale (a fit only).
    pub(crate) bounds: BTreeMap<String, (f64, f64)>,
    /// Which ω_k are held (all true or all false for a block).
    pub(crate) fixed_omega: Vec<bool>,
    /// Which error parameters are held.
    pub(crate) fixed_xi: Vec<bool>,
    /// The printed kind of covariance.
    pub(crate) printed: String,
    /// Where the start came from (a fit only).
    pub(crate) start_record: Option<StartRecord>,
}

fn method_of(word: Option<&str>) -> Result<Method, PopError> {
    match word {
        None | Some("foce_i") => Ok(Method::FoceI),
        Some("foce") => Ok(Method::Foce),
        Some("fo") => Ok(Method::Fo),
        Some(other) => Err(invalid(
            "method",
            format!("unknown method `{other}`; use `foce_i` (the default), `foce` or `fo`"),
        )),
    }
}

fn convention_of(word: Option<&str>) -> Result<OfvConvention, PopError> {
    match word {
        None | Some("full") => Ok(OfvConvention::Full),
        Some("no_2pi") => Ok(OfvConvention::No2Pi),
        Some(other) => Err(invalid(
            "ofv_convention",
            format!("unknown convention `{other}`; use `full` (the default) or `no_2pi`"),
        )),
    }
}

/// The printed kind of covariance (POP-COV-01).
fn covariance_of(word: Option<&str>) -> Result<String, PopError> {
    match word {
        None => Ok("sandwich".to_string()),
        Some(kind @ ("sandwich" | "r" | "s")) => Ok(kind.to_string()),
        Some(other) => Err(invalid(
            "covariance",
            format!("unknown covariance kind `{other}`; use `sandwich`, `r` or `s`"),
        )),
    }
}

/// `true` for a block Ω, `false` for a diagonal one.
fn block_of(word: Option<&str>) -> Result<bool, PopError> {
    match word {
        None | Some("diagonal") => Ok(false),
        Some("block") => Ok(true),
        Some(other) => Err(invalid(
            "omega_shape",
            format!("unknown shape of omega `{other}`; use `diagonal` (the default) or `block`"),
        )),
    }
}

fn error_model_of(word: Option<&str>) -> Result<ErrorModel, PopError> {
    let give = "give `additive`, `proportional`, `combined` or `combined_sd`";
    match word {
        None => Err(invalid(
            "error_model",
            format!("no error model is given and there is no default; {give}"),
        )),
        Some(id) => ErrorModel::from_id(id)
            .ok_or_else(|| invalid("error_model", format!("unknown error model `{id}`; {give}"))),
    }
}

fn structure_of(request: &PopRequest) -> Result<Structure, PopError> {
    let give = "give a `pk1.*` or a `pk2.*` model id";
    let Some(id) = request.model.as_deref() else {
        return Err(invalid("model", format!("no model is given; {give}")));
    };
    if id == "identity_linear" {
        let Some(z) = &request.model_options.z_powers else {
            return Err(invalid(
                "model_options",
                "the test model needs `z_powers`; give one power per random effect",
            ));
        };
        let z_powers = z
            .iter()
            .map(|&p| i32::try_from(p))
            .collect::<Result<Vec<i32>, _>>()
            .map_err(|_| invalid("model_options", "a power is too large; give small powers"))?;
        return Ok(Structure::IdentityLinear { z_powers });
    }
    if request.model_options.z_powers.is_some() {
        return Err(invalid(
            "model_options",
            format!("`z_powers` does not apply to `{id}`; leave it out"),
        ));
    }
    ModelId::from_id(id)
        .map(Structure::Catalogue)
        .ok_or_else(|| invalid("model", format!("unknown model `{id}`; {give}")))
}

/// POP-MOD-02: the list of random effects. A fit may leave the typical value of a random effect
/// out: the two-stage start computes it (POP-EST-05).
fn random_effects_of(
    request: &PopRequest,
    structure: &Structure,
    fit: bool,
) -> Result<Vec<String>, PopError> {
    let Some(list) = &request.random_effects else {
        return Err(invalid(
            "random_effects",
            "no random effect is declared and there is no default; list the parameters that carry one (for example `cl` and `v`)",
        ));
    };
    if list.is_empty() {
        return Err(invalid(
            "random_effects",
            "the list is empty; list at least one parameter (with none the fit is a pooled fit)",
        ));
    }
    for (i, p) in list.iter().enumerate() {
        if list.iter().take(i).any(|x| x == p) {
            return Err(invalid(
                "random_effects",
                format!("`{p}` is listed twice; list each parameter once"),
            ));
        }
    }
    let Structure::Catalogue(model) = structure else {
        // POP-MOD-05: free labels, rules 1 and 5 of POP-MOD-02 do not apply.
        return Ok(list.clone());
    };
    for p in list {
        match p.as_str() {
            "tlag" => {
                return Err(invalid_plain(
                    "random effect on `tlag`: not available in this version (the prediction has a kink at the lag time); give `tlag` as a shared typical value".to_string(),
                ));
            }
            "dur" => {
                return Err(invalid_plain(
                    "random effect on `dur`: not available in this version (the prediction has a kink at the end of the input); give `dur` as a shared typical value".to_string(),
                ));
            }
            _ => {}
        }
        let names = structure::model_parameters(*model);
        if !names.contains(&p.as_str()) {
            return Err(invalid(
                "random_effects",
                format!(
                    "`{p}` is not a parameter of `{}`; list parameters of that model ({})",
                    model.id(),
                    names.join(", ")
                ),
            ));
        }
        if !fit && !request.initial.contains_key(p) && !request.fixed.contains_key(p) {
            return Err(invalid(
                "random_effects",
                format!("`{p}` has no typical value; give it in `initial`"),
            ));
        }
    }
    Ok(list.clone())
}

/// A key of `fixed` that holds a variance of Ω.
fn omega_key(key: &str) -> Option<&str> {
    key.strip_prefix("omega.")
}

/// The typical values given by `initial` and `fixed` (error parameters and Ω excluded), finite.
fn given_theta(request: &PopRequest) -> Result<BTreeMap<String, f64>, PopError> {
    let mut theta = request.initial.clone();
    for (p, x) in &request.fixed {
        if matches!(p.as_str(), "a" | "b") || omega_key(p).is_some() {
            continue;
        }
        if theta.insert(p.clone(), *x).is_some() {
            return Err(invalid(
                "fixed",
                format!("`{p}` is in both `initial` and `fixed`; give it in one of them"),
            ));
        }
    }
    for (p, x) in &theta {
        if !x.is_finite() {
            return Err(invalid(
                "initial",
                format!("the typical value of `{p}` is {x}; give a finite number"),
            ));
        }
    }
    Ok(theta)
}

/// The checks of the typical values against the model.
fn check_theta(
    request: &PopRequest,
    theta: &BTreeMap<String, f64>,
    structure: &Structure,
    random_effects: &[String],
) -> Result<(), PopError> {
    match structure {
        Structure::IdentityLinear { z_powers } => {
            let keys: Vec<&str> = theta.keys().map(String::as_str).collect();
            if keys != ["theta1", "theta2"] {
                return Err(invalid(
                    "initial",
                    "the test model takes `theta1` and `theta2`; give those two",
                ));
            }
            if z_powers.len() != random_effects.len() {
                return Err(invalid(
                    "model_options",
                    format!(
                        "`z_powers` has {} entries for {} random effects; give one power per random effect",
                        z_powers.len(),
                        random_effects.len()
                    ),
                ));
            }
        }
        Structure::Catalogue(model) => {
            if model.compartments() == 2 {
                let clearance = ["cl", "vc", "q", "vp"];
                if !clearance.iter().all(|k| theta.contains_key(*k)) {
                    return Err(invalid(
                        "initial",
                        "the population fit takes a two-compartment model in the clearance set only (POP-MOD-04); give `cl`, `vc`, `q` and `vp`",
                    ));
                }
            }
            // A name that is not a parameter of the model (an error parameter under another key,
            // for example) must not reach θ.
            let names = structure::model_parameters(*model);
            if let Some(p) = theta.keys().find(|p| !names.contains(&p.as_str())) {
                let option = if request.fixed.contains_key(p) {
                    "fixed"
                } else {
                    "initial"
                };
                return Err(invalid(
                    option,
                    format!(
                        "`{p}` is not a parameter of `{}`; give parameters of that model ({}), and the error parameters as `a` and `b`",
                        model.id(),
                        names.join(", ")
                    ),
                ));
            }
            for (p, &x) in theta {
                let zero_allowed = p == "tlag";
                let inside = if zero_allowed { x >= 0.0 } else { x > 0.0 };
                if !inside {
                    let (domain, give) = if zero_allowed {
                        ("0 or greater", "give a value of 0 or more")
                    } else {
                        ("greater than 0", "give a positive value")
                    };
                    return Err(invalid(
                        "initial",
                        format!(
                            "the typical value of `{p}` is {x}, outside its domain (it must be {domain}); {give}"
                        ),
                    ));
                }
            }
            // The names and the parameter set, checked by the model itself.
            if let Err(e) = model.predict(1.0, theta, &[0.0]) {
                return Err(invalid("initial", format!("{e}")));
            }
        }
    }
    Ok(())
}

/// Ω as given, checked; `None` when the request gives none and a fit may compute it (POP-EST-05).
fn omega_given(
    request: &PopRequest,
    q: usize,
    block: bool,
    may_compute: bool,
) -> Result<Option<Mat>, PopError> {
    let Some(rows) = &request.omega else {
        if may_compute {
            return Ok(None);
        }
        return Err(invalid(
            "omega",
            "no starting omega is given; give the q × q covariance matrix of the random effects",
        ));
    };
    if rows.len() != q || rows.iter().any(|r| r.len() != q) {
        return Err(invalid(
            "omega",
            format!("the matrix is not {q} × {q}; give one row and one column per random effect"),
        ));
    }
    let mut omega = Mat::zeros(q, q);
    for (i, row) in rows.iter().enumerate() {
        for (j, &x) in row.iter().enumerate() {
            omega.set(i, j, x);
        }
    }
    for i in 0..q {
        for j in 0..q {
            let x = omega.at(i, j);
            if !x.is_finite() || x != omega.at(j, i) {
                return Err(invalid(
                    "omega",
                    "the matrix is not symmetric with finite elements; give a symmetric matrix",
                ));
            }
            if !block && i != j && x != 0.0 {
                return Err(invalid(
                    "omega",
                    format!(
                        "the shape is `diagonal` but element ({}, {}) is {x}; give zeros off the diagonal, or set `omega_shape` to `block`",
                        i + 1,
                        j + 1
                    ),
                ));
            }
        }
    }
    Ok(Some(omega))
}

/// The variances of Ω held by `fixed` (keys `omega.<random effect>`), one entry per random
/// effect (POP-EST-02). Decision taken meanwhile (2026-11-08), reversible by Abdou: the key holds
/// the variance; a fixed variance of 0 is refused (remove the random effect instead, POP-MOD-02);
/// for a block every variance must be given and equal the diagonal of `omega`, which then holds
/// the whole block.
fn fixed_omega_of(
    request: &PopRequest,
    random_effects: &[String],
    block: bool,
    omega: Option<&Mat>,
) -> Result<Vec<Option<f64>>, PopError> {
    let mut out = vec![None; random_effects.len()];
    for (key, &x) in &request.fixed {
        let Some(re) = omega_key(key) else {
            continue;
        };
        let Some(k) = random_effects.iter().position(|r| r == re) else {
            return Err(invalid(
                "fixed",
                format!(
                    "`{key}` names no random effect; use `omega.<parameter>` with a parameter of `random_effects`"
                ),
            ));
        };
        if !(x.is_finite() && x > 0.0) {
            return Err(invalid(
                "fixed",
                format!(
                    "`{key}` is {x}; give a positive variance, or remove the random effect of `{re}` to hold it at 0"
                ),
            ));
        }
        if let Some(slot) = out.get_mut(k) {
            *slot = Some(x);
        }
    }
    if block && out.iter().any(Option::is_some) {
        let whole = out.iter().all(Option::is_some);
        let matches =
            omega.is_some_and(|o| out.iter().enumerate().all(|(k, x)| *x == Some(o.at(k, k))));
        if !(whole && matches) {
            return Err(invalid(
                "fixed",
                "fixing one element of a block of omega is not available in this version; fix the whole block (every `omega.<parameter>` equal to the diagonal of `omega`) or none of it",
            ));
        }
    }
    Ok(out)
}

/// The lower factor L of the starting Ω, with the fixed variances put in place.
fn start_factor(omega: &Mat, fixed: &[Option<f64>]) -> Result<Mat, PopError> {
    let mut omega = omega.clone();
    for (k, x) in fixed.iter().enumerate() {
        if let Some(x) = x {
            omega.set(k, k, *x);
        }
    }
    dense::cholesky(&omega).ok_or_else(|| {
        invalid(
            "omega",
            "the starting matrix is not positive definite; give positive variances and correlations strictly between -1 and 1",
        )
    })
}

/// The keys of the error parameters in a request (POP-API-01 item 12): `a` for the first parameter
/// of the error model and `b` for the last, in the order of [`ErrorModel::names`].
pub(crate) fn error_keys(em: ErrorModel) -> &'static [&'static str] {
    match em {
        ErrorModel::Additive => &["a"],
        ErrorModel::Proportional => &["b"],
        ErrorModel::Combined | ErrorModel::CombinedSd => &["a", "b"],
    }
}

/// Where a value of an error parameter given by the request comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorSource {
    Fixed,
    Init,
}

impl ErrorSource {
    fn id(self) -> &'static str {
        match self {
            ErrorSource::Fixed => "fixed",
            ErrorSource::Init => "error_init",
        }
    }
}

/// The error parameters in the user's units given by `error_init` and `fixed` (keys `a`, `b`), one
/// entry per parameter of the error model in the order of [`error_keys`], `None` where neither
/// gives it. A value in `fixed` takes the place of the one in `error_init`.
fn error_given(
    request: &PopRequest,
    em: ErrorModel,
) -> Result<Vec<Option<(f64, ErrorSource)>>, PopError> {
    let keys = error_keys(em);
    let mut out: Vec<Option<(f64, ErrorSource)>> = vec![None; keys.len()];
    if let Some(init) = &request.error_init {
        if init.len() != keys.len() {
            return Err(invalid(
                "error_init",
                format!(
                    "the {} error model has {} parameter(s) ({}) but {} value(s) are given; give one value per parameter",
                    em.id(),
                    keys.len(),
                    keys.join(", "),
                    init.len()
                ),
            ));
        }
        out = init.iter().map(|x| Some((*x, ErrorSource::Init))).collect();
    }
    for name in ["a", "b"] {
        let Some(&x) = request.fixed.get(name) else {
            continue;
        };
        let Some(k) = keys.iter().position(|key| *key == name) else {
            return Err(invalid(
                "fixed",
                format!(
                    "the {} error model has no parameter `{name}`; leave it out",
                    em.id()
                ),
            ));
        };
        if let Some(slot) = out.get_mut(k) {
            *slot = Some((x, ErrorSource::Fixed));
        }
    }
    for (x, source) in out.iter().flatten() {
        if !(x.is_finite() && *x >= 0.0) {
            return Err(invalid(
                source.id(),
                format!(
                    "the error parameter {x} is not a finite number at or above 0; give such a number"
                ),
            ));
        }
    }
    Ok(out)
}

/// FIT-ML-10: the moment estimates of the moved error variables, here at the population prediction
/// pooled over the observations of all subjects. Used for a parameter the request does not give.
fn moment_xi(em: ErrorModel, subjects: &[Subject], pred0: &[Vec<f64>]) -> Vec<f64> {
    let pairs: Vec<(f64, f64)> = subjects
        .iter()
        .zip(pred0)
        .flat_map(|(s, p)| s.y.iter().copied().zip(p.iter().copied()))
        .collect();
    let n = pairs.len().max(1) as f64;
    let a0 = pairs.iter().map(|(y, f)| (y - f) * (y - f)).sum::<f64>() / n;
    let b0 = pairs
        .iter()
        .map(|(y, f)| {
            let m2 = (f * f + y * y) / 2.0;
            if m2 > 0.0 {
                (y - f) * (y - f) / m2
            } else {
                0.0
            }
        })
        .sum::<f64>()
        / n;
    match em {
        ErrorModel::Additive => vec![a0],
        ErrorModel::Proportional => vec![b0],
        ErrorModel::Combined => vec![a0 / 2.0, b0 / 2.0],
        ErrorModel::CombinedSd => vec![a0.sqrt() / 2.0, b0.sqrt() / 2.0],
    }
}

fn lag_domain(request: &PopRequest, model: ModelId) -> Option<LagDomain> {
    let lag = model.lag_parameter()?;
    if let Some(&c) = request.fixed.get(lag) {
        return Some(LagDomain { lower: c, upper: c });
    }
    request.initial.get(lag)?;
    let bounds = request.bounds.get(lag).copied().unwrap_or_default();
    Some(LagDomain {
        lower: bounds.lower.unwrap_or(0.0).max(0.0),
        upper: bounds.upper.unwrap_or(f64::INFINITY),
    })
}

/// Generated lower bound of FIT-BND-03 relative to the start (the value of the individual fit,
/// `engine::GENERATED_FLOOR`, mirrored here).
const GENERATED_FLOOR: f64 = 1e-6;

/// FIT-BND-03 as the individual fit applies it (`engine::generated_lower`): `v`, `cl`, `k`, `ka`,
/// `dur` above 1e-6 times their start, `tlag` at or above 0, any other name unbounded.
fn generated_lower(name: &str, start: f64) -> f64 {
    match name {
        "v" | "cl" | "k" | "ka" | "dur" => GENERATED_FLOOR * start,
        "tlag" => 0.0,
        _ => f64::NEG_INFINITY,
    }
}

/// The bounds of the free typical values of a fit (POP-EST-02), checked: a bound names a free
/// typical value, its lower end is below its upper end, and the start lies inside.
fn bounds_of(
    request: &PopRequest,
    theta: &BTreeMap<String, f64>,
    fixed: &[String],
    structure: &Structure,
) -> Result<BTreeMap<String, (f64, f64)>, PopError> {
    for name in request.bounds.keys() {
        if !theta.contains_key(name) || fixed.contains(name) {
            return Err(invalid(
                "bounds",
                format!(
                    "`{name}` is not a free typical value of the fit; bound free typical values only"
                ),
            ));
        }
    }
    let mut out = BTreeMap::new();
    for (name, &x) in theta {
        if fixed.contains(name) {
            continue;
        }
        let given = request.bounds.get(name).copied().unwrap_or_default();
        let generated = match structure {
            Structure::Catalogue(_) => generated_lower(name, x),
            Structure::IdentityLinear { .. } => f64::NEG_INFINITY,
        };
        let lo = given.lower.unwrap_or(generated);
        let hi = given.upper.unwrap_or(f64::INFINITY);
        if lo.is_nan() || hi.is_nan() || lo >= hi {
            return Err(invalid(
                "bounds",
                format!(
                    "the bounds of `{name}` are [{lo}, {hi}]; give a lower bound below the upper one"
                ),
            ));
        }
        if !(lo <= x && x <= hi) {
            return Err(invalid(
                "bounds",
                format!(
                    "the start of `{name}` is {x}, outside its bounds [{lo}, {hi}]; give a start inside them"
                ),
            ));
        }
        out.insert(name.clone(), (lo, hi));
    }
    Ok(out)
}

impl Setup {
    fn new(request: &PopRequest) -> Result<Self, PopError> {
        let fit = !request.options.evaluate_only;
        let method = method_of(request.options.method.as_deref())?;
        let convention = convention_of(request.options.ofv_convention.as_deref())?;
        let printed = covariance_of(request.options.covariance.as_deref())?;
        let block = block_of(request.omega_shape.as_deref())?;
        let error_model = error_model_of(request.error_model.as_deref())?;
        let structure = structure_of(request)?;
        let random_effects = random_effects_of(request, &structure, fit)?;
        let q = random_effects.len();
        let mut theta = given_theta(request)?;
        let fixed_theta: Vec<String> = request
            .fixed
            .keys()
            .filter(|k| !matches!(k.as_str(), "a" | "b") && omega_key(k).is_none())
            .cloned()
            .collect();
        let catalogue = matches!(structure, Structure::Catalogue(_));
        // The typical values the two-stage start computes (POP-EST-05).
        let missing: Vec<String> = random_effects
            .iter()
            .filter(|re| catalogue && !theta.contains_key(*re))
            .cloned()
            .collect();
        // Without a two-stage start the checks keep the order of the evaluation (T-153).
        if missing.is_empty() {
            check_theta(request, &theta, &structure, &random_effects)?;
        }
        let omega0 = omega_given(request, q, block, fit && catalogue)?;
        let fixed_omega = fixed_omega_of(request, &random_effects, block, omega0.as_ref())?;
        let factor0 = match &omega0 {
            Some(o) => Some(start_factor(o, &fixed_omega)?),
            None => None,
        };
        let given = error_given(request, error_model)?;
        let subjects = data::subjects(request)?;

        // POP-INN-03 item 3. The counts are left out of an evaluation that gives every parameter
        // (θ, Ω and ξ): it fits nothing (design choice, see the report of T-153).
        let complete = given.iter().all(Option::is_some);
        if !(request.options.evaluate_only && complete) {
            let free_theta = request.initial.len() + missing.len();
            let free_xi = given
                .iter()
                .filter(|g| !matches!(g, Some((_, ErrorSource::Fixed))))
                .count();
            checks::refuse_too_few(error_model, &subjects, free_theta, free_xi)?;
        }
        // POP-INN-03 item 1, before anything that needs σ.
        if let Structure::Catalogue(model) = &structure {
            checks::refuse_unbounded(*model, error_model, &subjects, lag_domain(request, *model))?;
        }
        let mut sources: BTreeMap<String, String> = BTreeMap::new();
        for name in theta.keys() {
            let source = if fixed_theta.contains(name) {
                "fixed"
            } else {
                "user"
            };
            sources.insert(name.clone(), source.to_string());
        }
        let mut two_stage = None;
        if let (Structure::Catalogue(model), true) =
            (&structure, fit && (!missing.is_empty() || omega0.is_none()))
        {
            let names: Vec<String> = request
                .initial
                .keys()
                .cloned()
                .chain(missing.iter().cloned())
                .collect();
            let held: BTreeMap<String, f64> = fixed_theta
                .iter()
                .filter_map(|n| theta.get(n).map(|x| (n.clone(), *x)))
                .collect();
            let ts = start::two_stage(
                *model,
                &names,
                &random_effects,
                block,
                error_model,
                &held,
                &request.initial,
                &subjects,
            )?;
            for name in &missing {
                if let Some(x) = ts.theta.get(name) {
                    theta.insert(name.clone(), *x);
                    sources.insert(name.clone(), "two_stage".to_string());
                }
            }
            two_stage = Some(ts);
        }
        if !missing.is_empty() {
            check_theta(request, &theta, &structure, &random_effects)?;
        }
        let l = match (factor0, &two_stage) {
            (Some(l), _) => l,
            (None, Some(ts)) => start_factor(&ts.omega, &fixed_omega)?,
            (None, None) => {
                return Err(invalid(
                    "omega",
                    "no starting omega is given; give the q × q covariance matrix of the random effects",
                ));
            }
        };
        for (k, re) in random_effects.iter().enumerate() {
            let source = if fixed_omega.get(k).copied().flatten().is_some() {
                "fixed"
            } else if omega0.is_some() {
                "user"
            } else {
                "two_stage"
            };
            sources.insert(format!("omega[{re},{re}]"), source.to_string());
        }

        let mut pred0 = Vec::with_capacity(subjects.len());
        for s in &subjects {
            let theta_i = subject_theta(&theta, s)?;
            let model = SubjectModel {
                structure: &structure,
                theta: &theta_i,
                random_effects: &random_effects,
                dose: s.dose,
                times: &s.times,
            };
            let zero = vec![0.0; q];
            let Some(p) = model.predict(&zero) else {
                return Err(invalid(
                    "initial",
                    format!(
                        "the model gives no finite prediction for subject {} at the typical values; check the typical values and the dose",
                        s.id
                    ),
                ));
            };
            pred0.push(p);
        }
        let moments = moment_xi(error_model, &subjects, &pred0);
        // An evaluation uses `error_init` (POP-API-01 item 6). A parameter the request does not give
        // takes the two-stage mean when the start ran the individual fits, else the moment estimate,
        // and the answer says so in `xi_used`: the controls of the refusal rows of the oracle give no
        // `error_init`.
        let (fallback, fallback_source) = match &two_stage {
            Some(ts) => (error_model.user_values(&ts.xi), "two_stage"),
            None => (error_model.user_values(&moments), "moments"),
        };
        let mut user = Vec::with_capacity(given.len());
        let mut xi_used = BTreeMap::new();
        for ((g, m), key) in given.iter().zip(fallback).zip(error_keys(error_model)) {
            let (value, source) = match g {
                Some((x, source)) => (*x, source.id()),
                None => (m, fallback_source),
            };
            user.push(value);
            sources.insert(format!("xi.{key}"), source.to_string());
            xi_used.insert(
                (*key).to_string(),
                ErrorValue {
                    value,
                    source: source.to_string(),
                },
            );
        }
        let xi = error_model.moved_from_user(&user);
        // POP-INN-03 item 2.
        checks::refuse_non_positive_sigma(error_model, &xi, &subjects, &pred0)?;
        let bounds = if fit {
            bounds_of(request, &theta, &fixed_theta, &structure)?
        } else {
            BTreeMap::new()
        };
        let start_record = fit.then(|| {
            let computed = sources.values().any(|s| s == "two_stage");
            let user_given = sources
                .values()
                .any(|s| matches!(s.as_str(), "user" | "error_init" | "moments"));
            let kind = match (computed, user_given) {
                (false, _) => "user",
                (true, false) => "two_stage",
                (true, true) => "mixed",
            };
            StartRecord {
                kind: kind.to_string(),
                sources,
                left_out: two_stage
                    .as_ref()
                    .map(|t| t.left_out.clone())
                    .unwrap_or_default(),
                note: two_stage.as_ref().and_then(|t| t.note.clone()),
            }
        });
        Ok(Self {
            method,
            convention,
            error_model,
            structure,
            fixed_omega: fixed_omega.iter().map(Option::is_some).collect(),
            fixed_xi: given
                .iter()
                .map(|g| matches!(g, Some((_, ErrorSource::Fixed))))
                .collect(),
            random_effects,
            subjects,
            block,
            start: Params { theta, l, xi },
            xi_used,
            fixed_theta,
            bounds,
            printed,
            start_record,
        })
    }

    /// N, the observations used.
    pub(crate) fn n_observations(&self) -> usize {
        self.subjects.iter().map(|s| s.y.len()).sum()
    }

    /// The free coordinates of φ and their bounds.
    fn layout(&self) -> Layout {
        let with_random_effect: Vec<String> = match self.structure {
            Structure::Catalogue(_) => self.random_effects.clone(),
            Structure::IdentityLinear { .. } => Vec::new(),
        };
        Layout::new(
            self.start.clone(),
            self.error_model,
            self.block,
            &Holds {
                fixed_theta: &self.fixed_theta,
                with_random_effect: &with_random_effect,
                bounds: &self.bounds,
                fixed_omega: &self.fixed_omega,
                fixed_xi: &self.fixed_xi,
            },
        )
    }

    /// The answers of the subjects and the total OFV, in the convention of the answer, the
    /// per-subject values summed in dataset order.
    fn subject_answers(&self, evaluation: &Evaluation) -> (Vec<SubjectAnswer>, f64) {
        let ln_2pi = libm::log(std::f64::consts::TAU);
        let mut total = 0.0;
        let mut out = Vec::with_capacity(self.subjects.len());
        for (s, v) in self.subjects.iter().zip(&evaluation.subjects) {
            let n = s.y.len();
            if n == 0 {
                out.push(SubjectAnswer {
                    id: s.id.clone(),
                    n_observations: 0,
                    ofv: 0.0,
                    eta_hat: None,
                    eta_hat_reason: Some("no_observations".to_string()),
                    newton_decrement: None,
                });
                continue;
            }
            let ofv = match self.convention {
                OfvConvention::Full => n as f64 * ln_2pi + v.core,
                OfvConvention::No2Pi => v.core,
            };
            total += ofv;
            out.push(SubjectAnswer {
                id: s.id.clone(),
                n_observations: n,
                ofv,
                eta_hat_reason: v.eta.is_none().then(|| "fo".to_string()),
                eta_hat: v.eta.clone(),
                newton_decrement: v.decrement,
            });
        }
        (out, total)
    }

    /// The answer shared by an evaluation and a fit, at `p`.
    fn answer(&self, status: &str, p: &Params, evaluation: &Evaluation, k: usize) -> PopAnswer {
        let (subjects, ofv) = self.subject_answers(evaluation);
        let shrinkage = diagnostics::shrinkage(
            self.method,
            self.error_model,
            &p.xi,
            &p.omega(),
            &self.subjects,
            evaluation,
        );
        let n = self.n_observations();
        let (aic, bic) = if matches!(status, "evaluated" | "converged" | "at_bound") {
            diagnostics::criteria(ofv, k, n)
        } else {
            (None, None)
        };
        PopAnswer {
            status: status.to_string(),
            method: self.method,
            ofv_convention: self.convention,
            ofv,
            n_subjects: self.subjects.iter().filter(|s| !s.y.is_empty()).count(),
            n_observations: n,
            random_effects: self.random_effects.clone(),
            xi_used: self.xi_used.clone(),
            subjects,
            eta_shrinkage: shrinkage.eta,
            eps_shrinkage: shrinkage.eps,
            k,
            aic,
            bic,
            estimate: None,
            se: None,
            se_by_kind: None,
            covariance: None,
            iterations: None,
            trace: None,
            start: None,
        }
    }

    /// `evaluate_only`: the objective, the modes and the diagnostics at the start.
    fn evaluate(&self, fail_inner: &dyn Fn(&str) -> bool) -> Result<PopAnswer, PopError> {
        let settings = Settings {
            eps: inner::EPS_IN,
            keep: true,
            fail_inner,
        };
        let evaluation = self
            .evaluate_params(&self.start, &[], settings)
            .map_err(|f| f.error(self.method))?;
        let k = self.layout().len();
        Ok(self.answer("evaluated", &self.start, &evaluation, k))
    }

    /// The outer fit (POP-EST-03), the covariance (POP-COV-01) and the answer (POP-EST-04).
    fn fit(&self, fail_inner: &dyn Fn(&str) -> bool) -> Result<PopAnswer, PopError> {
        let layout = self.layout();
        let phi0 = layout.phi(&self.start);
        let first = self
            .evaluate_params(
                &self.start,
                &[],
                Settings {
                    eps: inner::EPS_IN,
                    keep: false,
                    fail_inner,
                },
            )
            .map_err(|f| f.error(self.method))?;
        let problem = outer::Problem {
            setup: self,
            layout: &layout,
        };
        let outcome = problem.minimise(phi0, first);
        let p = layout.params(&outcome.phi);
        // The final point again, keeping the linearisations for the diagnostics, from its own modes.
        let q = self.random_effects.len();
        let kept = self
            .evaluate_params(
                &p,
                &outcome.evaluation.warm(q),
                Settings {
                    eps: inner::EPS_IN,
                    keep: true,
                    fail_inner: &|_| false,
                },
            )
            .unwrap_or_else(|_| outcome.evaluation.clone());
        let mut answer = self.answer(outcome.status.id(), &p, &kept, layout.len());
        let omega = p.omega();
        let start_omega = self.start.omega();
        let sd = |m: &Mat| -> Vec<f64> { (0..q).map(|k| m.at(k, k).sqrt()).collect() };
        let counts = covariance::Counts {
            m: answer.n_subjects,
            n: answer.n_observations,
            theta_free: layout
                .coords
                .iter()
                .filter(|c| c.part() == phi::Part::Theta)
                .count(),
            omega_free: layout
                .coords
                .iter()
                .filter(|c| c.part() == phi::Part::Omega)
                .count(),
            xi_free: layout
                .coords
                .iter()
                .filter(|c| c.part() == phi::Part::Xi)
                .count(),
            omega_start_sd: sd(&start_omega),
            omega_sd: sd(&omega),
            eta_shrinkage: answer.eta_shrinkage.clone(),
        };
        let errors = covariance::covariance(&problem, &outcome, &self.printed, &counts);
        let names = layout.user_names(&self.random_effects, error_keys(self.error_model));
        answer.estimate = Some(self.estimate(&p));
        answer.se = errors
            .by_kind
            .get(&self.printed)
            .map(|se| self.standard_errors(&p, se));
        answer.se_by_kind = Some(
            errors
                .by_kind
                .iter()
                .map(|(kind, se)| (kind.clone(), self.standard_errors(&p, se)))
                .collect(),
        );
        answer.covariance = Some(errors.block);
        answer.iterations = Some(outcome.iterations);
        answer.trace = Some(
            outcome
                .trace
                .iter()
                .map(|row| TraceAnswer {
                    iteration: row.iteration,
                    ofv: row.ofv,
                    ofv_convention: self.convention,
                    parameters: names.iter().cloned().zip(row.values.clone()).collect(),
                    nu: row.nu,
                    halvings: row.halvings,
                    decrement: row.decrement,
                    status: row.status.map(|s| s.id().to_string()),
                })
                .collect(),
        );
        answer.start = self.start_record.clone();
        Ok(answer)
    }

    /// The estimates in the user's units (POP-EST-04, POP-API-01 item 13).
    fn estimate(&self, p: &Params) -> Estimate {
        let omega = p.omega();
        let q = self.random_effects.len();
        let mut full = vec![vec![0.0; q]; q];
        let mut correlation = vec![vec![0.0; q]; q];
        for (i, row) in full.iter_mut().enumerate() {
            for (j, x) in row.iter_mut().enumerate() {
                if self.block || i == j {
                    *x = omega.at(i, j);
                }
            }
        }
        for (i, row) in correlation.iter_mut().enumerate() {
            for (j, x) in row.iter_mut().enumerate() {
                *x = if i == j {
                    1.0
                } else if self.block {
                    omega.at(i, j) / (omega.at(i, i) * omega.at(j, j)).sqrt()
                } else {
                    0.0
                };
            }
        }
        let diagonal: BTreeMap<String, f64> = self
            .random_effects
            .iter()
            .enumerate()
            .map(|(k, re)| (re.clone(), omega.at(k, k)))
            .collect();
        let sd = diagonal
            .iter()
            .map(|(k, v)| (k.clone(), v.sqrt()))
            .collect();
        let xi = error_keys(self.error_model)
            .iter()
            .zip(self.error_model.user_values(&p.xi))
            .map(|(k, v)| ((*k).to_string(), v))
            .collect();
        Estimate {
            theta: p.theta.clone(),
            omega_diagonal: diagonal,
            omega_full: full,
            omega_sd: sd,
            omega_correlation: correlation,
            xi,
        }
    }

    /// The standard errors of one kind, from the values in the order of [`Layout::user_names`].
    fn standard_errors(&self, p: &Params, se: &[Option<f64>]) -> StandardErrors {
        let q = self.random_effects.len();
        let n_theta = p.theta.len();
        let get = |i: usize| se.get(i).copied().flatten();
        let theta = p
            .theta
            .keys()
            .enumerate()
            .map(|(i, k)| (k.clone(), get(i)))
            .collect();
        let mut full = vec![vec![None; q]; q];
        let mut index = n_theta;
        for i in 0..q {
            for j in 0..=i {
                if self.block || i == j {
                    let v = get(index);
                    if let Some(row) = full.get_mut(i) {
                        if let Some(slot) = row.get_mut(j) {
                            *slot = v;
                        }
                    }
                    if let Some(row) = full.get_mut(j) {
                        if let Some(slot) = row.get_mut(i) {
                            *slot = v;
                        }
                    }
                    index += 1;
                }
            }
        }
        let omega = p.omega();
        let diagonal: BTreeMap<String, Option<f64>> = self
            .random_effects
            .iter()
            .enumerate()
            .map(|(k, re)| {
                let v = full.get(k).and_then(|r| r.get(k)).copied().flatten();
                (re.clone(), v)
            })
            .collect();
        let sd = self
            .random_effects
            .iter()
            .enumerate()
            .map(|(k, re)| {
                let w = omega.at(k, k).sqrt();
                let v = diagonal
                    .get(re)
                    .copied()
                    .flatten()
                    .filter(|_| w > 0.0)
                    .map(|s| s / (2.0 * w));
                (re.clone(), v)
            })
            .collect();
        let xi = error_keys(self.error_model)
            .iter()
            .enumerate()
            .map(|(k, key)| ((*key).to_string(), get(index + k)))
            .collect();
        StandardErrors {
            theta,
            omega_diagonal: diagonal,
            omega_full: full,
            omega_sd: sd,
            xi,
        }
    }
}

/// The typical values with the subject's own duration merged in (POP-API-01 item 11).
fn subject_theta(
    theta: &BTreeMap<String, f64>,
    s: &Subject,
) -> Result<BTreeMap<String, f64>, PopError> {
    let mut t = theta.clone();
    if let Some(d) = s.duration {
        if t.insert("dur".to_string(), d).is_some() {
            return Err(invalid(
                "subjects",
                format!(
                    "subject {} carries a `duration` and `dur` is also a typical value; give it in one place",
                    s.id
                ),
            ));
        }
    }
    Ok(t)
}
