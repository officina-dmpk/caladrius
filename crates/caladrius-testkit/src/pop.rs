//! Loaders and helpers for the population-fit oracle (task T-152, `specs/population.md`, POP-ORA-*):
//! `oracle/expected/pop/` and `oracle/data/pop/`.
//!
//! Files (produced by the scripts in `oracle/scripts/pop_*.R`):
//!
//! - `exact/<case>.json` (`pop_exact`, `pop_exact.R`, POP-ORA-01): the exact identification suite. One case is one
//!   model, one set of parameters, one dataset, one frozen-sigma setting; the expected values are decimal strings
//!   computed with 256-bit arithmetic (50 significant digits) for the five objective forms, per subject and in
//!   total, with the conditional modes. [`PopExactCase`].
//! - `ofv/<case>.json` (`pop_ofv`, `pop_ofv.R`): the objective at fixed parameters, in double precision
//!   (the nearest double to the 256-bit value), with the diagnostics (PRED, IPRED, IWRES, CWRES in three forms,
//!   shrinkage with both divisors, AIC and BIC). [`PopOfvCase`].
//! - `errors.json` (`pop_errors.R`): the requests the population fit must refuse (POP-INN-03 rules 1 to 6), each
//!   with the code and words of its message and, where one exists, a control that must not be refused.
//!   [`load_pop_errors`].
//! - `sealed/<design>.json` plus `oracle/data/pop/sealed/<design>/rep_NNN.csv` (`pop_sealed.R`, POP-ORA-04): the
//!   sealed simulate-then-fit data. The test recomputes the SHA-256 of every file ([`verify_pop_sealed`]).
//! - `second_look/*.json`: nlme and saemix (informational) and the nlmixr2 script's output once it is run; never
//!   a pass or fail reference, and not loaded here.
//!
//! The absolute tolerances of the population tests are in `impl AbsoluteTolerance` below, next to their reasons.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use crate::ml::AbsoluteTolerance;
use crate::oracle::{OracleError, oracle_dir};
use crate::step3::{json, read};

/// `oracle/expected/pop/`.
pub fn pop_dir() -> PathBuf {
    oracle_dir().join("expected").join("pop")
}

/// `oracle/data/pop/`.
pub fn pop_data_dir() -> PathBuf {
    oracle_dir().join("data").join("pop")
}

/// The five objective forms, in the order of the files (POP-OBJ-*): the linearised form at the conditional
/// modes, FO, and the three Laplace forms.
pub const POP_FORMS: [&str; 5] = [
    "linearised",
    "fo",
    "laplace_full",
    "laplace_expected",
    "laplace_no_variance",
];

/// The sealed designs (POP-ORA-04, OP-13).
pub const POP_SEALED_DESIGNS: [&str; 2] = ["accurate", "limits"];

// ---------------------------------------------------------------- tolerances

impl crate::tolerance::Tolerance {
    /// A standard error of the population fit (any of the three kinds of POP-COV-01, on theta, Omega or xi, in the
    /// user's units) against the exact value of `oracle/expected/pop/fit_exact/*.json` (`standard_errors`):
    /// relative error at most 1e-4. PROPOSED by card T-188 for Q-026 (no SE tolerance existed; POP-EST-06 asks for
    /// one). Measured on the three interior cases (`fit_q1_h38`, `fit_q2_diag`, `fit_q2_block`), two starts each,
    /// with the engine at aedeaca: the worst relative error is 1.3e-5 (sandwich), 2.0e-5 (r) and 2.8e-5 (s), the
    /// largest being `s` on `fit_q2_diag` from the far start; the review of T-154 measured 2.8e-5 too. 1e-4 is
    /// the parameter tolerance of `AGENTS.md` section 5, a margin of 3.6 over the worst error. The SE depends on the
    /// estimate and on the finite differences, so it follows the stopping point of the optimiser (POP-EST-06).
    pub const POP_SE: crate::tolerance::Tolerance = crate::tolerance::Tolerance::Relative {
        rel: 1e-4,
        abs: 0.0,
    };
}

impl AbsoluteTolerance {
    /// Population objective at fixed parameters, one value (a total over subjects, one subject, or AIC and BIC,
    /// which are the total plus a constant), the conditional modes being found by the code under test. Absolute,
    /// because the objective crosses zero. Same value as `FIT_M2LL` (1e-7). Measured in T-152: the independent
    /// double-precision code (`naive_pop`) is within 2.9e-12 of the 256-bit totals on the exact cases, and the
    /// double rounding of a stored value is far below 1e-12, so 1e-7 leaves five orders of margin for the code under
    /// test and does not loosen any existing constant.
    pub const POP_OFV: AbsoluteTolerance = AbsoluteTolerance { abs: 1e-7 };
    /// The conditional mode itself (an eta, a dimensionless log-scale deviation), and every quantity the tests
    /// compare per subject that is a function of it. The linearised objective of a subject is NOT stationary in its
    /// own eta (POP-INN-02, POP-COV-02): an error `d` in eta costs about `slope * d` in that subject objective,
    /// and the total adds the subjects. The measured `checks.eta_perturbation_ofv_slope_max` over the 20 `ofv`
    /// cases is 1.7 to 37.4 (the largest: `theoph_proportional`, 12 subjects). To hold `POP_OFV` on the largest
    /// case, `d <= POP_OFV / (37.4 * 12) = 2.2e-10`; the constant is 1e-10, the power of ten below. The naive code
    /// is within 8.5e-15 of the 256-bit modes (exact and ofv cases), and the Newton polish of FIT-ML-08 took the
    /// individual fits to 6.04e-14 in -2LL (specs/fit.md), so this is a requirement on the inner solve (POP-INN-02:
    /// polish to the floor), not on the outer one.
    ///
    /// The slope is per component of eta. With q random effects the first-order worst case adds up to q times the
    /// slope per subject: with the largest slope for every subject, every component off by exactly the constant in
    /// the worst direction, and q = 3 (`theoph_proportional`), the sum is 3 x 37.4 x 12 x 1e-10 = 1.3e-7. That is
    /// 1.3 times `POP_OFV`, so the margin in that pessimistic q = 3 case is below 1 (`POP_OFV` divided by 1.3e-7 is
    /// 0.77): the error in the total could exceed `POP_OFV` by a factor 1.3.
    /// That case does not occur in practice (the slope is a maximum over subjects and components, the components
    /// err with different signs, and a polished inner solve is at about 1e-13), so the constant keeps a power of
    /// ten of margin for q = 1 and q = 2.
    /// The cost is that a flat objective in eta needs the inner solver polished to the floor (POP-INN-02).
    pub const POP_ETA: AbsoluteTolerance = AbsoluteTolerance { abs: 1e-10 };
    /// IPRED, IWRES and the three CWRES forms (PRED does not depend on eta): concentration-scale or standardised
    /// values derived from the modes. Same value as `FIT_IWRES` (1e-5), which the engine meets with a margin of
    /// two orders (9e-8, FIT-ML-08). The modes are accurate to `POP_ETA`, and an error `d` in an eta changes a
    /// standardised residual by about `|d yhat / d eta| / sigma * d`. Measured at the conditional modes of the 20
    /// `ofv` cases and of the 23 exact cases given to the engine: the maximum of `|d yhat / d eta| / sigma` is
    /// 3.24e2 (`lin_q3_block`, subject S3, observation 5, third random effect), and the exact derivative of IWRES
    /// (interaction term included) has the same maximum (test
    /// `the_amplification_of_an_error_in_eta_is_below_the_documented_factor`). An error of 1e-10 in eta therefore
    /// becomes at most 3.2e-8 in a residual. The naive code is within 5.0e-14 of the stored values.
    pub const POP_DIAGNOSTIC: AbsoluteTolerance = AbsoluteTolerance { abs: 1e-5 };
    /// Shrinkage (both divisors), derived from the modes and the variances: an error `d` in the modes moves the
    /// standard deviation of the modes by at most `d`, divided by the square root of the smallest omega in the
    /// cases (0.04, so 0.2), which gives at most 5 times `POP_ETA`; 1e-7 is the same order as `POP_OFV`.
    pub const POP_DERIVED: AbsoluteTolerance = AbsoluteTolerance { abs: 1e-7 };
    /// The independent double-precision code (`naive_pop`) against the stored values at the same parameters:
    /// the floor of the naive code, measured by `tests/pop_consistency.rs` (objectives within 2.9e-12, modes
    /// within 8.5e-15, diagnostics within 5.0e-14, so 1e-9 is a bound with a margin of two to three orders).
    pub const NAIVE_POP_OFV: AbsoluteTolerance = AbsoluteTolerance { abs: 1e-9 };
}

// ---------------------------------------------------------------- data of the cases

/// A model as written in the case files (every number a decimal string, so that 256-bit inputs are exact).
#[derive(Debug, Clone, Deserialize)]
pub struct PopModel {
    /// `pk1.iv_bolus`, `pk1.oral_1`, `pk2.iv_bolus` or the test-only `identity_linear` (POP-MOD-05).
    pub name: String,
    /// The parameters that carry a random effect, in the order of the rows of Omega.
    pub random_effects: Vec<String>,
    /// The typical values by parameter name.
    pub theta: BTreeMap<String, String>,
    /// Omega, rows of decimal strings.
    pub omega: Vec<Vec<String>>,
    /// `additive`, `proportional`, `combined` or `combined_sd`.
    pub error_model: String,
    /// The error parameters: `a` and/or `b`.
    #[serde(default)]
    pub xi: BTreeMap<String, String>,
    /// `identity_linear` only: the power of the time in the column of Z of each random effect.
    #[serde(default)]
    pub z_powers: Option<Vec<u32>>,
}

/// One subject: dose, times and observations as decimal strings.
#[derive(Debug, Clone, Deserialize)]
pub struct PopSubject {
    pub id: String,
    pub dose: String,
    pub time: Vec<String>,
    pub y: Vec<String>,
}

fn num(path: &str, what: &str, text: &str) -> Result<f64, OracleError> {
    text.trim()
        .parse::<f64>()
        .map_err(|e| OracleError::Inconsistent {
            path: path.to_owned(),
            message: format!("{what}: `{text}` is not a number ({e})"),
        })
}

impl PopSubject {
    /// The dose as a double.
    pub fn dose_f64(&self) -> Result<f64, OracleError> {
        num(&self.id, "dose", &self.dose)
    }
    /// The times as doubles.
    pub fn times_f64(&self) -> Result<Vec<f64>, OracleError> {
        self.time.iter().map(|t| num(&self.id, "time", t)).collect()
    }
    /// The observations as doubles.
    pub fn y_f64(&self) -> Result<Vec<f64>, OracleError> {
        self.y.iter().map(|t| num(&self.id, "y", t)).collect()
    }
}

impl PopModel {
    /// A parameter value as a double.
    pub fn theta_f64(&self, name: &str) -> Result<f64, OracleError> {
        match self.theta.get(name) {
            Some(t) => num(name, "theta", t),
            None => Err(OracleError::Inconsistent {
                path: self.name.clone(),
                message: format!("no typical value named {name}"),
            }),
        }
    }
    /// Omega as doubles.
    pub fn omega_f64(&self) -> Result<Vec<Vec<f64>>, OracleError> {
        self.omega
            .iter()
            .map(|r| r.iter().map(|t| num(&self.name, "omega", t)).collect())
            .collect()
    }
    /// An error parameter (`a` or `b`) as a double, 0 when absent.
    pub fn xi_f64(&self, name: &str) -> Result<f64, OracleError> {
        match self.xi.get(name) {
            Some(t) => num(name, "xi", t),
            None => Ok(0.0),
        }
    }
    /// True when Omega has a non-zero off-diagonal element (a block).
    pub fn omega_is_block(&self) -> bool {
        self.omega.iter().enumerate().any(|(i, row)| {
            row.iter()
                .enumerate()
                .any(|(j, t)| i != j && t.trim().parse::<f64>().map(|v| v != 0.0).unwrap_or(true))
        })
    }
}

/// The five objective forms by name (decimal strings in the exact cases).
pub type FormsText = BTreeMap<String, String>;
/// The five objective forms by name (doubles in the `ofv` cases).
pub type FormsNumber = BTreeMap<String, f64>;

/// One subject of an exact case.
#[derive(Debug, Clone, Deserialize)]
pub struct PopExactSubject {
    pub id: String,
    /// The conditional mode (the mode of the frozen-sigma problem for the frozen cases), 50 digits.
    pub eta_hat: Vec<String>,
    /// The five forms, full convention, 50 digits; `exact_marginal` too where q = 1 allows it.
    pub ofv_full: FormsText,
}

/// The expected values of an exact case.
#[derive(Debug, Clone, Deserialize)]
pub struct PopExactExpected {
    pub n_obs: usize,
    pub n_ln_2pi: String,
    pub per_subject: Vec<PopExactSubject>,
    pub total_full: FormsText,
    pub total_no_2pi: FormsText,
}

/// A case of the exact identification suite (`oracle/expected/pop/exact/`).
#[derive(Debug, Clone, Deserialize)]
pub struct PopExactCase {
    pub id: String,
    /// `a_linear`, `b_nonlinear`, `c_frozen` or `d_interaction`.
    pub family: String,
    pub description: String,
    pub precision_bits: u32,
    pub digits: u32,
    /// `none`, `mode` or `pred0` (OP-14): where the variance is held for the frozen cases.
    pub frozen_sigma: String,
    pub model: PopModel,
    pub subjects: Vec<PopSubject>,
    pub expected: PopExactExpected,
    /// The checks the script made at 256 bits (identities, the bound of the exact marginal).
    pub checks: Value,
}

/// The diagnostics of one subject at the conditional mode (linearised form, interaction), doubles.
#[derive(Debug, Clone, Deserialize)]
pub struct PopSubjectDiagnostics {
    pub pred: Vec<f64>,
    pub ipred: Vec<f64>,
    pub iwres: Vec<f64>,
    pub sigma_hat: Vec<f64>,
    pub e: Vec<f64>,
    pub c_diag: Vec<f64>,
    pub cwres_diagonal: Vec<f64>,
    pub cwres_cholesky: Vec<f64>,
    pub cwres_symmetric: Vec<f64>,
    pub sum_w2: f64,
    pub logdet_c: f64,
}

/// One subject of an `ofv` case.
#[derive(Debug, Clone, Deserialize)]
pub struct PopOfvSubject {
    pub id: String,
    pub n_obs: usize,
    pub eta_hat: Vec<f64>,
    pub ofv_full: FormsNumber,
    pub ofv_no_2pi: FormsNumber,
    pub diagnostics: PopSubjectDiagnostics,
}

/// The expected values of an `ofv` case.
#[derive(Debug, Clone, Deserialize)]
pub struct PopOfvExpected {
    pub n_subjects: usize,
    pub n_obs: usize,
    pub n_ln_2pi: f64,
    pub per_subject: Vec<PopOfvSubject>,
    pub total_full: FormsNumber,
    pub total_no_2pi: FormsNumber,
}

/// The number of parameters of the information criteria (OP-05).
#[derive(Debug, Clone, Deserialize)]
pub struct PopParameterCount {
    pub theta: usize,
    pub omega: usize,
    pub xi: usize,
    pub total: usize,
}

/// AIC and BIC of one form, both counts of the BIC (OP-05).
#[derive(Debug, Clone, Deserialize)]
pub struct PopCriteria {
    pub aic: f64,
    pub bic_n_observations: f64,
    pub bic_n_subjects: f64,
}

/// The population-level diagnostics of an `ofv` case.
#[derive(Debug, Clone, Deserialize)]
pub struct PopPopulationDiagnostics {
    pub eta_shrinkage_divisor_m_minus_1: Vec<f64>,
    pub eta_shrinkage_divisor_m: Vec<f64>,
    pub sd_eta_hat_divisor_m_minus_1: Vec<f64>,
    pub sd_eta_hat_divisor_m: Vec<f64>,
    pub omega_sd: Vec<f64>,
    pub sd_iwres_divisor_n_minus_1: f64,
    pub sd_iwres_divisor_n: f64,
    pub eps_shrinkage_divisor_n_minus_1: f64,
    pub eps_shrinkage_divisor_n: f64,
    pub k: PopParameterCount,
    pub information_criteria_full: BTreeMap<String, PopCriteria>,
}

/// A case of the objective at fixed parameters (`oracle/expected/pop/ofv/`).
#[derive(Debug, Clone, Deserialize)]
pub struct PopOfvCase {
    pub id: String,
    pub description: String,
    pub source: Value,
    pub model: PopModel,
    pub subjects: Vec<PopSubject>,
    pub mode_route: Value,
    pub expected: PopOfvExpected,
    pub diagnostics_population: PopPopulationDiagnostics,
    pub checks: Value,
}

/// The design of a fit case (`oracle/expected/pop/fit_exact/`).
#[derive(Debug, Clone, Deserialize)]
pub struct PopFitDesign {
    /// `q1`, `diag` or `block`.
    pub shape: String,
    /// `diagonal` or `block`: the value of the request key `omega_shape` (the start matrices of a block case
    /// may have zero off-diagonal elements, so the shape cannot be read from them).
    pub omega_shape: String,
    pub m: usize,
    pub n_per_subject: usize,
    pub n_observations: usize,
    pub times: Vec<String>,
    pub z_powers: Vec<u32>,
    /// The seed, the rule and the generating values, as written by the script.
    pub generator: Value,
}

/// One start of a fit: a name and the model block (typical values, Omega and xi) given to the engine.
#[derive(Debug, Clone, Deserialize)]
pub struct PopFitStart {
    pub name: String,
    pub model: PopModel,
}

/// The exact estimates of a fit case (256 bits, 50 digits).
#[derive(Debug, Clone, Deserialize)]
pub struct PopFitExpected {
    /// `interior` or `boundary`.
    pub kind: String,
    /// `closed_form`, `newton` or `newton_rank_one`.
    pub method: String,
    pub theta1: String,
    pub theta2: String,
    /// Omega, q by q (a diagonal Omega has "0" off the diagonal).
    pub omega: Vec<Vec<String>>,
    /// The error standard deviation (`xi.a`, user units) and its square.
    pub sigma: String,
    pub sigma2: String,
    /// The OFV at the maximum, convention `full` and `no_2pi`; `n_ln_2pi` is their difference.
    pub ofv_full: String,
    pub ofv_no_2pi: String,
    pub n_ln_2pi: String,
    /// What is held on its bound (empty for an interior maximum).
    pub at_bound: Vec<String>,
    /// The sign conditions of a constrained maximum (gradient of the OFV with respect to a parameter on its bound,
    /// or `G l = 0` and the trace of `G` for the rank-one boundary of a block).
    #[serde(default)]
    pub sign_conditions: Vec<Value>,
}

/// The curvature basis of the tolerances: the diagonal of the inverse of R, the Hessian of the OFV, per parameter.
#[derive(Debug, Clone, Deserialize)]
pub struct PopFitBasis {
    /// `all_parameters` or `free_block`.
    pub basis: String,
    /// The labels of the answer (`theta1`, `omega_full[1][0]`, `xi.a`, ...), in the order of `rinv_diag`.
    pub parameters: Vec<String>,
    pub rinv_diag: Vec<f64>,
}

/// A case of the exact fit-level oracle (`oracle/expected/pop/fit_exact/`, card T-168).
#[derive(Debug, Clone, Deserialize)]
pub struct PopFitCase {
    pub id: String,
    pub family: String,
    pub description: String,
    pub precision_bits: u32,
    pub digits: u32,
    pub design: PopFitDesign,
    /// The model block at the generating values (the first start).
    pub model: PopModel,
    pub starts: Vec<PopFitStart>,
    pub subjects: Vec<PopSubject>,
    pub expected: PopFitExpected,
    /// How the point was obtained and checked (stationarity, polish).
    pub exact_reference: Value,
    pub tolerance_basis: PopFitBasis,
    /// Recorded, not asserted: the three covariance kinds of POP-COV-01 as standard errors.
    pub standard_errors: Value,
    /// The three references and their disagreements.
    pub references: Value,
    pub versions: Value,
}

impl PopFitCase {
    /// The labels of the answer in the order of the parameter vector (`theta1`, `theta2`, the elements of Omega that
    /// are parameters, `xi.a`).
    pub fn labels(&self) -> Vec<String> {
        let mut v = vec!["theta1".to_owned(), "theta2".to_owned()];
        match self.design.shape.as_str() {
            "q1" => v.push("omega_full[0][0]".to_owned()),
            "diag" => {
                v.push("omega_full[0][0]".to_owned());
                v.push("omega_full[1][1]".to_owned());
            }
            _ => {
                v.push("omega_full[0][0]".to_owned());
                v.push("omega_full[1][0]".to_owned());
                v.push("omega_full[1][1]".to_owned());
            }
        }
        v.push("xi.a".to_owned());
        v
    }

    /// The exact value of a labelled parameter as a double.
    pub fn expected_value(&self, label: &str) -> Result<f64, OracleError> {
        let e = &self.expected;
        let text = match label {
            "theta1" => &e.theta1,
            "theta2" => &e.theta2,
            "xi.a" => &e.sigma,
            "omega_full[0][0]" => &e.omega[0][0],
            "omega_full[1][0]" | "omega_full[0][1]" => &e.omega[1][0],
            "omega_full[1][1]" => &e.omega[1][1],
            other => {
                return Err(OracleError::Inconsistent {
                    path: self.id.clone(),
                    message: format!("unknown parameter label `{other}`"),
                });
            }
        };
        num(&self.id, label, text)
    }

    /// The bound on the error of a labelled parameter that an OFV excess of at most `ofv_tolerance` implies: the
    /// OFV excess of an error `d` is `d^T R d / 2`, so `|d_j| <= sqrt(2 * eps * (R^-1)_jj)` (`R` the Hessian of the
    /// OFV, `tolerance_basis`). A variance on its bound has its own bound, see [`Self::bound_gradient`].
    pub fn curvature_bound(
        &self,
        label: &str,
        ofv_tolerance: AbsoluteTolerance,
    ) -> Result<f64, OracleError> {
        let b = &self.tolerance_basis;
        match b.parameters.iter().position(|p| p == label) {
            Some(i) => Ok((2.0 * ofv_tolerance.abs * b.rinv_diag[i]).sqrt()),
            None => Err(OracleError::Inconsistent {
                path: self.id.clone(),
                message: format!("no curvature for `{label}`"),
            }),
        }
    }

    /// For a variance held on its bound at 0 (`q1`, `diag`): the gradient of the OFV with respect to the variance
    /// `label`, positive. An OFV excess of at most `eps` allows the variance at most `eps / gradient`: the OFV is
    /// concave in the variance (`m ln(lambda)`), so the excess is at most `gradient * variance`, not at least; the
    /// bound is therefore sufficient for an excess within `eps`, and slightly stricter than the OFV tolerance; it stays the
    /// test (an engine requirement, not a widened tolerance).
    pub fn bound_gradient(&self, label: &str) -> Option<f64> {
        self.expected
            .sign_conditions
            .iter()
            .find(|c| c.get("parameter").and_then(Value::as_str) == Some(label))
            .and_then(|c| c.get("gradient"))
            .and_then(Value::as_f64)
    }
}

fn exact_dir() -> PathBuf {
    pop_dir().join("exact")
}
fn ofv_dir() -> PathBuf {
    pop_dir().join("ofv")
}

fn case_names(dir: &std::path::Path) -> Result<Vec<String>, OracleError> {
    let entries = std::fs::read_dir(dir).map_err(|e| OracleError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter_map(|n| n.strip_suffix(".json").map(str::to_owned))
        .collect();
    names.sort();
    Ok(names)
}

/// Names of the exact cases, sorted.
pub fn list_pop_exact_cases() -> Result<Vec<String>, OracleError> {
    Ok(case_names(&exact_dir())?
        .into_iter()
        .filter(|n| n != "options")
        .collect())
}

/// One exact case.
pub fn load_pop_exact_case(id: &str) -> Result<PopExactCase, OracleError> {
    json(&exact_dir().join(format!("{id}.json")))
}

fn fit_exact_dir() -> PathBuf {
    pop_dir().join("fit_exact")
}

/// Names of the exact fit cases (`oracle/expected/pop/fit_exact/`), sorted.
pub fn list_pop_fit_cases() -> Result<Vec<String>, OracleError> {
    case_names(&fit_exact_dir())
}

/// One exact fit case.
pub fn load_pop_fit_case(id: &str) -> Result<PopFitCase, OracleError> {
    json(&fit_exact_dir().join(format!("{id}.json")))
}

/// Names of the `ofv` cases, sorted.
pub fn list_pop_ofv_cases() -> Result<Vec<String>, OracleError> {
    Ok(case_names(&ofv_dir())?
        .into_iter()
        .filter(|n| n != "options")
        .collect())
}

/// One `ofv` case.
pub fn load_pop_ofv_case(id: &str) -> Result<PopOfvCase, OracleError> {
    json(&ofv_dir().join(format!("{id}.json")))
}

// ---------------------------------------------------------------- refusals

/// What the message of a refusal must contain (words, not the exact sentence: the engine writes its own).
#[derive(Debug, Clone, Deserialize)]
pub struct PopExpectedError {
    /// The serialised code of the error (every refused row has one; for a refusal of the dataset layer it is
    /// the dataset's own code of the first affected subject, POP-INN-03.6).
    #[serde(default)]
    pub code: Option<String>,
    /// Words that must all appear in the message.
    #[serde(default)]
    pub words: Vec<String>,
    /// `spec_sentence`, `reused_sentence` or `inferred`: where the words come from.
    #[serde(default)]
    pub words_basis: String,
    /// Words that must not appear (for example the other subjects of the dataset).
    #[serde(default)]
    pub must_not_contain: Vec<String>,
}

/// A request that must not be refused, next to a refused one.
#[derive(Debug, Clone, Deserialize)]
pub struct PopControl {
    pub change: String,
    pub request: Value,
}

/// One row of `errors.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PopErrorCase {
    pub case: String,
    /// The rule id(s), for example `POP-INN-03.1; PX4`.
    pub rule: String,
    pub description: String,
    /// `refused` or `accepted`.
    pub expect: String,
    /// False for the row that is a sentence only (no request can be built that is sure to fail).
    #[serde(default = "yes")]
    pub constructible: bool,
    /// The request, as data; `None` where `constructible` is false.
    #[serde(default)]
    pub request: Option<Value>,
    #[serde(default)]
    pub error: Option<PopExpectedError>,
    #[serde(default)]
    pub reuses: Option<String>,
    #[serde(default)]
    pub control: Option<PopControl>,
    #[serde(default)]
    pub note: Option<String>,
    /// For an accepted row: the flags the covariance block of the answer must hold (POP-COV-03, POP-API-01 item 4);
    /// the request is then run as a full fit, not as an evaluation.
    #[serde(default)]
    pub expect_flags: Vec<String>,
}

fn yes() -> bool {
    true
}

/// The refusal rows of `oracle/expected/pop/errors.json`, in file order.
pub fn load_pop_errors() -> Result<Vec<PopErrorCase>, OracleError> {
    #[derive(Deserialize)]
    struct File {
        cases: Vec<PopErrorCase>,
    }
    let f: File = json(&pop_dir().join("errors.json"))?;
    Ok(f.cases)
}

// ---------------------------------------------------------------- sealed data

/// One file of a sealed design.
#[derive(Debug, Clone, Deserialize)]
pub struct PopSealedFile {
    pub replicate: usize,
    pub seed: u64,
    /// Relative to `oracle/`.
    pub path: String,
    pub rows: usize,
    pub sha256: String,
}

/// The constants of the aggregate checks (POP-ORA-04), written in the file.
#[derive(Debug, Clone, Deserialize)]
pub struct PopAggregateConstants {
    pub c_bias: f64,
    pub c_interval: f64,
    pub c_coverage: f64,
    pub nominal_coverage: f64,
}

/// The aggregate checks of a sealed design.
#[derive(Debug, Clone, Deserialize)]
pub struct PopAggregateChecks {
    /// True: the typical-value checks are asserted; false: recorded only.
    pub asserted: bool,
    pub note: String,
    pub constants: PopAggregateConstants,
}

/// The shape of a sealed design.
#[derive(Debug, Clone, Deserialize)]
pub struct PopSealedDesign {
    pub subjects: usize,
    pub times: Vec<String>,
    pub observations_per_subject: usize,
    pub observations_per_replicate: usize,
    pub replicates: usize,
}

/// The true values the data were drawn from.
#[derive(Debug, Clone, Deserialize)]
pub struct PopTruth {
    pub typical_values: BTreeMap<String, String>,
    pub omega_diagonal: BTreeMap<String, String>,
    pub omega_full: Vec<Vec<String>>,
    pub xi: BTreeMap<String, String>,
}

/// A sealed design (`oracle/expected/pop/sealed/<design>.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct PopSealed {
    pub name: String,
    pub description: String,
    pub sealed: bool,
    pub model: PopModel,
    pub design: PopSealedDesign,
    pub truth: PopTruth,
    pub aggregate_checks: PopAggregateChecks,
    pub files: Vec<PopSealedFile>,
    pub manifest_sha256: String,
}

/// One sealed design.
pub fn load_pop_sealed(design: &str) -> Result<PopSealed, OracleError> {
    json(&pop_dir().join("sealed").join(format!("{design}.json")))
}

/// The sealed replicates of a design: dataset of replicate `k` (1-based) as subjects, in the order of the file.
pub fn load_pop_sealed_replicate(
    sealed: &PopSealed,
    k: usize,
) -> Result<Vec<PopSubject>, OracleError> {
    let file = sealed
        .files
        .iter()
        .find(|f| f.replicate == k)
        .ok_or_else(|| OracleError::Inconsistent {
            path: sealed.name.clone(),
            message: format!("no replicate {k}"),
        })?;
    let path = oracle_dir().join(&file.path);
    let text = read(&path)?;
    parse_sealed_csv(&path.display().to_string(), &text)
}

fn parse_sealed_csv(path: &str, text: &str) -> Result<Vec<PopSubject>, OracleError> {
    let mut subjects: Vec<PopSubject> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 {
            if line != "subject,time,conc,dose" {
                return Err(OracleError::Csv {
                    path: path.to_owned(),
                    line: 1,
                    message: format!("unexpected header `{line}`"),
                });
            }
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        let [s, t, y, d] = f.as_slice() else {
            return Err(OracleError::Csv {
                path: path.to_owned(),
                line: i + 1,
                message: "expected 4 fields".to_owned(),
            });
        };
        if subjects.last().map(|x| x.id.as_str()) != Some(*s) {
            subjects.push(PopSubject {
                id: (*s).to_owned(),
                dose: (*d).to_owned(),
                time: Vec::new(),
                y: Vec::new(),
            });
        }
        if let Some(last) = subjects.last_mut() {
            last.time.push((*t).to_owned());
            last.y.push((*y).to_owned());
        }
    }
    Ok(subjects)
}

/// Recompute the SHA-256 of every file of a sealed design and of the manifest; the first difference is an error.
pub fn verify_pop_sealed(sealed: &PopSealed) -> Result<(), OracleError> {
    let mut manifest = String::new();
    for f in &sealed.files {
        let path = oracle_dir().join(&f.path);
        let bytes = std::fs::read(&path).map_err(|e| OracleError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let got = sha256_hex(&bytes);
        if got != f.sha256 {
            return Err(OracleError::Inconsistent {
                path: f.path.clone(),
                message: format!(
                    "sealed file changed: sha256 is {got}, the seal says {}",
                    f.sha256
                ),
            });
        }
        let rows = bytes
            .iter()
            .filter(|b| **b == b'\n')
            .count()
            .saturating_sub(1);
        if rows != f.rows {
            return Err(OracleError::Inconsistent {
                path: f.path.clone(),
                message: format!("{rows} data rows, the seal says {}", f.rows),
            });
        }
        manifest.push_str(&format!("{} {}\n", f.path, f.sha256));
    }
    let got = sha256_hex(manifest.as_bytes());
    if got != sealed.manifest_sha256 {
        return Err(OracleError::Inconsistent {
            path: sealed.name.clone(),
            message: format!(
                "manifest sha256 is {got}, the seal says {}",
                sealed.manifest_sha256
            ),
        });
    }
    if sealed.files.len() != sealed.design.replicates {
        return Err(OracleError::Inconsistent {
            path: sealed.name.clone(),
            message: format!(
                "{} files for {} replicates",
                sealed.files.len(),
                sealed.design.replicates
            ),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- SHA-256 (FIPS 180-4), for the seal

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data` as 64 lowercase hex digits. Written here because the test layer depends on nothing but
/// serde; checked against the FIPS 180-4 vectors in `tests/pop_consistency.rs`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for (k, wi) in K256.iter().zip(w.iter()) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(*wi);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
