#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Population-fit oracle (card T-152, rules POP-* of `specs/population.md`): the ground truth for the
//! population fit, written before any engine code exists.
//!
//! What is compared, and with what (`oracle/README.md`, section "Population fit"):
//!
//! * `oracle/expected/pop/exact/`: the exact identification suite (POP-ORA-01). Objectives and conditional
//!   modes at fixed parameters, computed at 256 bits by `oracle/scripts/pop_exact.R` with no population
//!   software, on the test-only model `identity_linear` (POP-MOD-05) and on the catalogue models. Three kinds by
//!   the field `frozen_sigma` (OP-14, POP-OBJ-06): `none` cases are given to the engine with `foce_i` and `fo`
//!   (`POP_EXACT_ENGINE_CASES`); `pred0` cases (the variance frozen at the population prediction) are the exact
//!   cases of `method = foce` (`POP_EXACT_FOCE_CASES`); `mode` cases (the variance frozen at the mode) are not an
//!   engine method, they identify the objective forms against each other and are checked in `caladrius-testkit`
//!   only (`POP_EXACT_IDENTITY_CASES`).
//! * `oracle/expected/pop/ofv/`: the objective at fixed parameters in double precision, with the
//!   diagnostics (PRED, IPRED, IWRES, CWRES in three forms, shrinkage with both divisors, AIC and BIC with k),
//!   for three models by four error models, on seeded synthetic data, Theoph and Indometh.
//! * `oracle/expected/pop/fit_exact/`: the exact maximum of the balanced linear case (card T-168), with three
//!   references that agree (`oracle/scripts/pop_fit_exact.R`): q = 1, q = 2 with a diagonal Omega and with a block,
//!   interior and on the boundary; the engine is asked for the fit from two starts (`POP_FIT_EXACT_CASES`).
//! * `oracle/expected/pop/errors.json`: refusals as rows (POP-INN-03, PX4), each refused row with a control.
//! * `oracle/expected/pop/sealed/` and `oracle/data/pop/sealed/`: the sealed simulate-then-fit data
//!   (POP-ORA-04): 200 replicates per design, committed before any engine code; only aggregates are asserted.
//!
//! Written before the engine exists: every engine test fails at run time with "population fit not built",
//! from the one function below, `population_fit`. **The engine card (T-153) replaces the body of that
//! function and nothing else.** The guards at the end are green today.
//!
//! # The request and the answer (assumed; the plainest name where `specs/population.md` is silent)
//!
//! The request is data (golden rule 4), in the vocabulary of the refusal rows of `errors.json` (the same
//! JSON, so a refusal row can be given to the adapter unchanged):
//!
//! ```text
//! { "model": "pk1.iv_bolus" | "pk1.oral_1" | "pk2.iv_bolus" | "identity_linear",
//!   "initial": { "<par>": x },                 typical values (and the start of a fit)
//!   "random_effects": ["cl", "v"],             in the order of the rows of Omega
//!   "omega": [[..]], "omega_shape": "diagonal" | "block",
//!   "error_model": "additive" | "proportional" | "combined" | "combined_sd",
//!   "error_init": [a] | [b] | [a, b],          user units, as FIT-ML-21
//!   "model_options": { "z_powers": [..] },     identity_linear only: Z column k = t^z_powers[k]
//!   "fixed": { "<par>": x }, "bounds": { "<par>": { "lower": x, "upper": x } },   refusal rows only (POP-API-01 item 12)
//!   "dataset_file": "data/pop/..csv",          in place of "subjects" (refusal rows)
//!   "options": { "method": "foce_i" | "foce" | "fo",   POP-OBJ-03, POP-OBJ-06 (foce_i is the default)
//!                "covariance": "sandwich" | "r" | "s", "exclude_subjects": ["<label>"],
//!                "ofv_convention": "full" | "no_2pi",       POP-OBJ-05
//!                "evaluate_only": true },      assumed name: the objective, the modes and the
//!                                              diagnostics at the given parameters, no outer iteration
//!   "subjects": [ { "id", "dose", "time": [..], "y": [..] } ] }
//! ```
//!
//! The answer, read by name:
//!
//! ```text
//! { "method", "ofv_convention", "ofv": total,
//!   "subjects": [ { "id", "ofv", "eta_hat": [..],
//!                   "pred", "ipred", "iwres", "cwres_diagonal", "cwres_cholesky": [..] } ],
//!   "eta_shrinkage": [..]  (divisor m - 1, POP-DIA-03),  "eps_shrinkage": x (divisor N - 1),
//!   "k": n, "aic": x, "bic": x  (BIC with ln N, OP-05) }
//! and, for a fit (no evaluate_only):
//!   "estimate": { "<par>": x, "omega_diagonal": { "<random effect>": x }, "omega_full": [[..]] (q by q, in the
//!                 order of "random_effects"), "xi": { "a": x, "b": x } },           POP-API-01 item 13
//!   "se": { same keys },  "status": "converged" | .. | "evaluated",
//!   "covariance": { "flags": ["few_subjects", "few_observations"], .. } }
//! ```
//!
//! The sealed test reads `status`: `converged` and `at_bound` both count as converged (FIT-CNV-03, FIT-ML-08:
//! `at_bound` is a converged fit with a parameter on a bound, POP-EST-02), so an `at_bound` replicate stays in
//! the aggregates, with the `se` of the printed kind (the SE of the free block, FIT-BND-02, POP-ORA-04). A
//! replicate with any other status is counted, printed and left out of the aggregates, and an asserted design
//! may have at most `MAX_NOT_CONVERGED_SHARE` of them. The number of `at_bound` replicates is printed. The
//! estimates of Omega (`estimate.omega_full`) and of the error parameters (`estimate.xi`) are printed, not
//! asserted. An accepted row of `errors.json` that carries `expect_flags` is run as a full fit and the array
//! `covariance.flags` of the answer must hold them.
//!
//! If T-153 renames fields, the adapter `population_fit` absorbs it, with one exception: the refusal rows of
//! `errors.json` carry their own `request` JSON (in the vocabulary above), which the tests also give to the
//! adapter unchanged, so a renamed request field must be accepted by the adapter in that JSON as well.
//!
//! A refusal is a `PopError { code, message }` (serialised code: `unbounded_likelihood`,
//! `non_positive_sigma`, `too_few_subjects`, `too_few_observations`, `inner_failed`, `invalid_option`,
//! `unsupported_history`). The engine must return `cwres_cholesky` and `cwres_diagonal`; `cwres_symmetric` is
//! kept in the reference as a cross-check and is not compared with the engine (POP-API-01 item 3, OP-06 decided).
//! FO has no EBE-based quantity (POP-OBJ-03).
//!
//! # Tolerances
//!
//! All from `caladrius_testkit::AbsoluteTolerance` (`POP_OFV`, `POP_ETA`, `POP_DIAGNOSTIC`, `POP_DERIVED`:
//! absolute, because the objective crosses zero; the reasons and the measurements behind them are in the
//! report of T-152 and in `crates/caladrius-testkit/src/pop.rs`). Nothing existing is loosened. The sealed
//! aggregates use the constants written in the sealed files (`aggregate_checks.constants`).

use std::fmt;

use caladrius_testkit::{
    AbsoluteTolerance, POP_FORMS, POP_SEALED_DESIGNS, PopErrorCase, PopExactCase, PopFitCase,
    PopFitStart, PopModel, PopOfvCase, PopSealed, PopSubject, list_pop_exact_cases,
    list_pop_fit_cases, list_pop_ofv_cases, load_pop_errors, load_pop_exact_case,
    load_pop_fit_case, load_pop_ofv_case, load_pop_sealed, load_pop_sealed_replicate, pop_data_dir,
    verify_pop_sealed,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------- the adapter (the only thing T-153 changes)

/// A refusal or a failure of the population fit, as the adapter reports it.
#[derive(Debug, Clone)]
struct PopError {
    code: String,
    message: String,
}

impl fmt::Display for PopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// THE ADAPTER. Today: the population fit is not built. T-153 replaces this body (and only this body) with
/// the call into `caladrius-fit`, building the engine's input from `request` and returning the answer in the
/// shape described in the header of this file.
fn population_fit(request: &Value) -> Result<Value, PopError> {
    let mut input: caladrius_fit::pop::PopRequest = serde_json::from_value(request.clone())
        .map_err(|e| PopError {
            code: "invalid_request".to_owned(),
            message: e.to_string(),
        })?;
    if let Some(path) = &input.dataset_file {
        let text = std::fs::read_to_string(path).map_err(|e| PopError {
            code: "invalid_option".to_owned(),
            message: format!("option `dataset_file`: the table `{path}` cannot be read ({e})"),
        })?;
        input.dataset_text = Some(text);
    }
    match caladrius_fit::pop::run(&input) {
        Ok(answer) => serde_json::to_value(&answer).map_err(|e| PopError {
            code: "invalid_answer".to_owned(),
            message: e.to_string(),
        }),
        Err(e) => Err(PopError {
            code: e.code.id().to_owned(),
            message: e.message,
        }),
    }
}

// ---------------------------------------------------------------- building requests from the cases

fn f(text: &str) -> f64 {
    text.trim()
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("`{text}` is not a number: {e}"))
}

fn model_request(
    model: &PopModel,
    subjects: &[PopSubject],
    method: &str,
    convention: &str,
    evaluate_only: bool,
) -> Value {
    let initial: serde_json::Map<String, Value> = model
        .theta
        .iter()
        .map(|(k, v)| (k.clone(), json!(f(v))))
        .collect();
    let omega: Vec<Vec<f64>> = model
        .omega
        .iter()
        .map(|r| r.iter().map(|t| f(t)).collect())
        .collect();
    let error_init: Vec<f64> = match model.error_model.as_str() {
        "additive" => vec![f(&model.xi["a"])],
        "proportional" => vec![f(&model.xi["b"])],
        _ => vec![f(&model.xi["a"]), f(&model.xi["b"])],
    };
    let mut request = json!({
        "model": model.name,
        "initial": initial,
        "random_effects": model.random_effects,
        "omega": omega,
        "omega_shape": if model.omega_is_block() { "block" } else { "diagonal" },
        "error_model": model.error_model,
        "error_init": error_init,
        "options": {
            "method": method,
            "ofv_convention": convention,
            "evaluate_only": evaluate_only,
        },
        "subjects": subjects.iter().map(|s| json!({
            "id": s.id,
            "dose": f(&s.dose),
            "time": s.time.iter().map(|t| f(t)).collect::<Vec<_>>(),
            "y": s.y.iter().map(|t| f(t)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    if let Some(z) = &model.z_powers {
        request["model_options"] = json!({ "z_powers": z });
    }
    request
}

fn evaluate(model: &PopModel, subjects: &[PopSubject], method: &str, convention: &str) -> Value {
    let request = model_request(model, subjects, method, convention, true);
    population_fit(&request)
        .unwrap_or_else(|e| panic!("{}: the engine refuses a valid evaluation: {e}", model.name))
}

// ---------------------------------------------------------------- reading answers

fn number(v: &Value, what: &str) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("the answer has no number at `{what}`: {v}"))
}

fn vector(v: &Value, what: &str) -> Vec<f64> {
    v.as_array()
        .unwrap_or_else(|| panic!("the answer has no array at `{what}`: {v}"))
        .iter()
        .map(|x| number(x, what))
        .collect()
}

fn answer_subjects(answer: &Value, expected_count: usize) -> &[Value] {
    let subjects = answer["subjects"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer has no `subjects`: {answer}"));
    assert_eq!(subjects.len(), expected_count, "subjects in the answer");
    subjects
}

/// Collects the values outside tolerance, one line each.
struct Problems(Vec<String>, usize);

impl Problems {
    fn new() -> Self {
        Problems(Vec::new(), 0)
    }
    fn check(&mut self, label: &str, actual: f64, expected: f64, tol: AbsoluteTolerance) {
        self.1 += 1;
        if !tol.accepts(actual, expected) {
            self.0.push(format!(
                "{label}: expected {expected:e}, got {actual:e} (absolute {:e})",
                tol.abs
            ));
        }
    }
    fn check_vec(&mut self, label: &str, actual: &[f64], expected: &[f64], tol: AbsoluteTolerance) {
        if actual.len() != expected.len() {
            self.1 += 1;
            self.0.push(format!(
                "{label}: {} values, expected {}",
                actual.len(),
                expected.len()
            ));
            return;
        }
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            self.check(&format!("{label}[{i}]"), *a, *e, tol);
        }
    }
    fn finish(self, case: &str) {
        assert!(self.1 > 0, "{case}: nothing was compared");
        assert!(
            self.0.is_empty(),
            "{case}: {} of {} values outside tolerance\n  {}",
            self.0.len(),
            self.1,
            self.0.join("\n  ")
        );
    }
}

fn ofv_label(method: &str) -> &'static str {
    match method {
        "fo" => "fo",
        _ => "linearised",
    }
}

// ---------------------------------------------------------------- exact cases (POP-ORA-01)

/// An exact case given to the engine: `frozen_sigma` is `none` for `foce_i` and `fo`, `pred0` for `foce`.
fn exact_case(name: &str, frozen: &str) -> PopExactCase {
    let case = load_pop_exact_case(name).expect("the exact case loads");
    assert_eq!(
        case.frozen_sigma, frozen,
        "{name}: not a case with the variance frozen as `{frozen}`"
    );
    case
}

fn check_exact_ofv(name: &str, frozen: &str, method: &str) {
    let case = exact_case(name, frozen);
    let answer = evaluate(&case.model, &case.subjects, method, "full");
    let form = ofv_label(method);
    let mut p = Problems::new();
    p.check(
        &format!("{name} total {form}"),
        number(&answer["ofv"], "ofv"),
        f(&case.expected.total_full[form]),
        AbsoluteTolerance::POP_OFV,
    );
    let subjects = answer_subjects(&answer, case.expected.per_subject.len());
    for (a, e) in subjects.iter().zip(&case.expected.per_subject) {
        p.check(
            &format!("{name} {} {form}", e.id),
            number(&a["ofv"], "subjects.ofv"),
            f(&e.ofv_full[form]),
            AbsoluteTolerance::POP_OFV,
        );
    }
    p.finish(name);
}

fn check_exact_modes(name: &str, frozen: &str, method: &str) {
    let case = exact_case(name, frozen);
    let answer = evaluate(&case.model, &case.subjects, method, "full");
    let mut p = Problems::new();
    let subjects = answer_subjects(&answer, case.expected.per_subject.len());
    for (a, e) in subjects.iter().zip(&case.expected.per_subject) {
        let expected: Vec<f64> = e.eta_hat.iter().map(|t| f(t)).collect();
        p.check_vec(
            &format!("{name} {} eta_hat", e.id),
            &vector(&a["eta_hat"], "eta_hat"),
            &expected,
            AbsoluteTolerance::POP_ETA,
        );
    }
    p.finish(name);
}

macro_rules! exact_case_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::*;
                const NAME: &str = stringify!($case);
                #[test]
                fn ofv_foce_i() { check_exact_ofv(NAME, "none", "foce_i"); }
                #[test]
                fn ofv_fo() { check_exact_ofv(NAME, "none", "fo"); }
                #[test]
                fn conditional_modes() { check_exact_modes(NAME, "none", "foce_i"); }
            }
        )*
    };
}

/// The exact cases of `method = foce` (POP-OBJ-06): the variance frozen at the population prediction. The total
/// and the per-subject objectives are the `linearised` form of the file, the modes are those of the frozen
/// problem.
macro_rules! exact_foce_case_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::*;
                const NAME: &str = stringify!($case);
                #[test]
                fn ofv_foce() { check_exact_ofv(NAME, "pred0", "foce"); }
                #[test]
                fn conditional_modes_foce() { check_exact_modes(NAME, "pred0", "foce"); }
            }
        )*
    };
}

// ---------------------------------------------------------------- objectives and diagnostics at fixed parameters

fn ofv_case(name: &str) -> PopOfvCase {
    load_pop_ofv_case(name).expect("the ofv case loads")
}

fn check_ofv(name: &str, method: &str, convention: &str) {
    let case = ofv_case(name);
    let answer = evaluate(&case.model, &case.subjects, method, convention);
    assert_eq!(
        answer["ofv_convention"].as_str(),
        Some(convention),
        "{name}: every answer names its convention (POP-OBJ-05)"
    );
    let form = ofv_label(method);
    let (total, per_subject) = if convention == "full" {
        (&case.expected.total_full, true)
    } else {
        (&case.expected.total_no_2pi, false)
    };
    let mut p = Problems::new();
    p.check(
        &format!("{name} total {form} ({convention})"),
        number(&answer["ofv"], "ofv"),
        total[form],
        AbsoluteTolerance::POP_OFV,
    );
    let subjects = answer_subjects(&answer, case.expected.per_subject.len());
    for (a, e) in subjects.iter().zip(&case.expected.per_subject) {
        let table = if per_subject {
            &e.ofv_full
        } else {
            &e.ofv_no_2pi
        };
        p.check(
            &format!("{name} {} {form} ({convention})", e.id),
            number(&a["ofv"], "subjects.ofv"),
            table[form],
            AbsoluteTolerance::POP_OFV,
        );
    }
    p.finish(name);
}

fn check_ofv_modes(name: &str) {
    let case = ofv_case(name);
    let answer = evaluate(&case.model, &case.subjects, "foce_i", "full");
    let mut p = Problems::new();
    let subjects = answer_subjects(&answer, case.expected.per_subject.len());
    for (a, e) in subjects.iter().zip(&case.expected.per_subject) {
        p.check_vec(
            &format!("{name} {} eta_hat", e.id),
            &vector(&a["eta_hat"], "eta_hat"),
            &e.eta_hat,
            AbsoluteTolerance::POP_ETA,
        );
    }
    p.finish(name);
}

fn check_diagnostics(name: &str) {
    let case = ofv_case(name);
    let answer = evaluate(&case.model, &case.subjects, "foce_i", "full");
    let tol = AbsoluteTolerance::POP_DIAGNOSTIC;
    let mut p = Problems::new();
    let subjects = answer_subjects(&answer, case.expected.per_subject.len());
    for (a, e) in subjects.iter().zip(&case.expected.per_subject) {
        let d = &e.diagnostics;
        for (key, expected) in [
            ("pred", &d.pred),
            ("ipred", &d.ipred),
            ("iwres", &d.iwres),
            ("cwres_diagonal", &d.cwres_diagonal),
            ("cwres_cholesky", &d.cwres_cholesky),
        ] {
            p.check_vec(
                &format!("{name} {} {key}", e.id),
                &vector(&a[key], key),
                expected,
                tol,
            );
        }
    }
    p.finish(name);
}

fn check_shrinkage_and_criteria(name: &str) {
    let case = ofv_case(name);
    let answer = evaluate(&case.model, &case.subjects, "foce_i", "full");
    let d = &case.diagnostics_population;
    let mut p = Problems::new();
    p.check_vec(
        &format!("{name} eta_shrinkage"),
        &vector(&answer["eta_shrinkage"], "eta_shrinkage"),
        &d.eta_shrinkage_divisor_m_minus_1,
        AbsoluteTolerance::POP_DERIVED,
    );
    p.check(
        &format!("{name} eps_shrinkage"),
        number(&answer["eps_shrinkage"], "eps_shrinkage"),
        d.eps_shrinkage_divisor_n_minus_1,
        AbsoluteTolerance::POP_DERIVED,
    );
    p.check(
        &format!("{name} k"),
        number(&answer["k"], "k"),
        d.k.total as f64,
        AbsoluteTolerance { abs: 0.0 },
    );
    let c = &d.information_criteria_full["linearised"];
    p.check(
        &format!("{name} aic"),
        number(&answer["aic"], "aic"),
        c.aic,
        AbsoluteTolerance::POP_OFV,
    );
    p.check(
        &format!("{name} bic (ln N)"),
        number(&answer["bic"], "bic"),
        c.bic_n_observations,
        AbsoluteTolerance::POP_OFV,
    );
    p.finish(name);
}

macro_rules! ofv_case_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::*;
                const NAME: &str = stringify!($case);
                #[test]
                fn ofv_foce_i_full() { check_ofv(NAME, "foce_i", "full"); }
                #[test]
                fn ofv_fo_full() { check_ofv(NAME, "fo", "full"); }
                #[test]
                fn ofv_foce_i_no_2pi() { check_ofv(NAME, "foce_i", "no_2pi"); }
                #[test]
                fn ofv_fo_no_2pi() { check_ofv(NAME, "fo", "no_2pi"); }
                #[test]
                fn conditional_modes() { check_ofv_modes(NAME); }
                #[test]
                fn diagnostics() { check_diagnostics(NAME); }
                #[test]
                fn shrinkage_and_criteria() { check_shrinkage_and_criteria(NAME); }
            }
        )*
    };
}

// ---------------------------------------------------------------- refusals (POP-INN-03, PX4)

fn error_row(name: &str) -> PopErrorCase {
    load_pop_errors()
        .expect("errors.json loads")
        .into_iter()
        .find(|r| r.case == name)
        .unwrap_or_else(|| panic!("no refusal row {name}"))
}

/// The request of a row, made cheap: the objective at the given parameters, no outer iteration (every
/// refusal of POP-INN-03 comes before the first outer iteration), and the dataset file resolved.
fn row_request(request: &Value) -> Value {
    let mut r = request.clone();
    if let Some(options) = r.get_mut("options").and_then(Value::as_object_mut) {
        options.insert("evaluate_only".to_owned(), json!(true));
    } else {
        r["options"] = json!({ "evaluate_only": true });
    }
    if let Some(path) = r.get("dataset_file").and_then(Value::as_str) {
        let relative = path.strip_prefix("data/pop/").unwrap_or(path);
        r["dataset_file"] = json!(pop_data_dir().join(relative).display().to_string());
    }
    r
}

fn check_refused(name: &str) {
    let row = error_row(name);
    assert_eq!(row.expect, "refused", "{name}");
    let request = row_request(
        row.request
            .as_ref()
            .expect("a constructible row has a request"),
    );
    let expected = row.error.as_ref().expect("a refused row has an error");
    let error = match population_fit(&request) {
        Ok(answer) => panic!("{name}: the request must be refused, got an answer: {answer}"),
        Err(e) => e,
    };
    if let Some(code) = &expected.code {
        assert_eq!(&error.code, code, "{name}: the code of the refusal");
    }
    // the words are compared without regard to case, as `oracle_fit_ml.rs` does: the pr1_* rows name the word
    // `exclude`, and the sentence of FIT-ML-13c that POP-INN-03 item 1 reuses says "Exclude those observations"
    // (decision taken meanwhile, 2026-11-08, reversible by Abdou). The words that must NOT appear (subject labels
    // such as `S1`) stay case-sensitive: a lowercase `s1` could occur inside another word.
    let lower = error.message.to_lowercase();
    for word in &expected.words {
        assert!(
            lower.contains(&word.to_lowercase()),
            "{name}: the message must contain `{word}` (any case): {}",
            error.message
        );
    }
    for word in &expected.must_not_contain {
        assert!(
            !error.message.contains(word.as_str()),
            "{name}: the message must not contain `{word}`: {}",
            error.message
        );
    }
    // the control, where the row has one: the same request with the one change that removes the cause is not
    // refused (a row without a control is one whose cause has no single-change repair in the data)
    if let Some(control) = &row.control {
        let answer = population_fit(&row_request(&control.request));
        assert!(
            answer.is_ok(),
            "{name}: the control ({}) must not be refused: {:?}",
            control.change,
            answer.err()
        );
    }
}

fn check_accepted(name: &str) {
    let row = error_row(name);
    assert_eq!(row.expect, "accepted", "{name}");
    let mut request = row_request(row.request.as_ref().expect("an accepted row has a request"));
    if !row.expect_flags.is_empty() {
        // flags belong to a fit: the covariance exists only after the outer iteration
        request["options"]["evaluate_only"] = json!(false);
    }
    let answer = match population_fit(&request) {
        Ok(answer) => answer,
        Err(e) => panic!("{name}: the request must not be refused: {e}"),
    };
    let flags: Vec<&str> = answer["covariance"]["flags"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for wanted in &row.expect_flags {
        assert!(
            flags.contains(&wanted.as_str()),
            "{name}: the flags of the covariance must hold `{wanted}`, they are {flags:?}"
        );
    }
}

macro_rules! refusal_tests {
    ($($case:ident),* $(,)?) => {
        mod refusals {
            $(
                #[test]
                fn $case() { super::check_refused(stringify!($case)); }
            )*
        }
    };
}

macro_rules! accepted_tests {
    ($($case:ident),* $(,)?) => {
        mod accepted {
            $(
                #[test]
                fn $case() { super::check_accepted(stringify!($case)); }
            )*
        }
    };
}

// ---------------------------------------------------------------- sealed simulate-then-fit (POP-ORA-04)

/// The start of every sealed fit: the truth moved by these factors, so that the optimiser has work to do and
/// no start depends on an estimate of the engine (written once, before any engine code; OP-13).
const START_THETA_FACTOR: f64 = 1.25;
const START_OMEGA_FACTOR: f64 = 1.5;
const START_XI_FACTOR: f64 = 1.25;

fn sealed_start(sealed: &PopSealed) -> PopModel {
    let mut m = sealed.model.clone();
    m.theta = sealed
        .truth
        .typical_values
        .iter()
        .map(|(k, v)| (k.clone(), format!("{:e}", f(v) * START_THETA_FACTOR)))
        .collect();
    m.omega = sealed
        .truth
        .omega_full
        .iter()
        .map(|r| {
            r.iter()
                .map(|t| format!("{:e}", f(t) * START_OMEGA_FACTOR))
                .collect()
        })
        .collect();
    m.xi = sealed
        .truth
        .xi
        .iter()
        .map(|(k, v)| (k.clone(), format!("{:e}", f(v) * START_XI_FACTOR)))
        .collect();
    m
}

/// Mean and standard deviation (divisor K - 1) of a sample.
fn mean_sd(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let mean = x.iter().sum::<f64>() / n;
    let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0);
    (mean, var.sqrt())
}

/// The manifests of the sealed designs, pinned in this source. The seal in the JSON next to the data is not
/// enough alone: a commit that edited the data and the JSON together would pass it. The values are the
/// `manifest_sha256` written when the data were sealed (commit e8dc5d2); changing one of them is changing the seal.
const PINNED_MANIFESTS: [(&str, &str); 2] = [
    (
        "accurate",
        "16f92c8e312ef7670a7ce42b9193e2a2a81fc16368ed4ef5fdb2de832eb482be",
    ),
    (
        "limits",
        "25c9b7c4f41bb0a009bd9b302149c3c8f10bcc66a653e7ed1725034f2bb9f358",
    ),
];

/// Verifies a sealed design: every file against the hash list, the hash list against the manifest, and the
/// manifest against the value pinned in this source; the list has one file per replicate.
fn verify_sealed_pinned(sealed: &PopSealed) {
    verify_pop_sealed(sealed).expect("the sealed files are unchanged");
    let pinned = PINNED_MANIFESTS
        .iter()
        .find(|(name, _)| *name == sealed.name)
        .unwrap_or_else(|| panic!("no pinned manifest for the sealed design `{}`", sealed.name))
        .1;
    assert_eq!(
        sealed.manifest_sha256, pinned,
        "{}: the manifest differs from the one pinned in oracle_pop.rs",
        sealed.name
    );
    assert_eq!(
        sealed.files.len(),
        sealed.design.replicates,
        "{}: one file per replicate",
        sealed.name
    );
}

/// DECISION TAKEN MEANWHILE (2026-11-08), REVERSIBLE BY ABDOU: the largest share of replicates of an asserted
/// design that may end with a status that is neither `converged` nor `at_bound` (FIT-CNV-03: `at_bound` is a
/// converged fit with a parameter on a bound, FIT-ML-08, POP-EST-02). A replicate that does not converge is
/// counted, reported and left out of the aggregates (the sealed note says so); without a bound, an engine that never converges would
/// pass with nothing to aggregate. 2% is 4 of 200: a design chosen so that the method is accurate should
/// converge in nearly every replicate, and a defect shows as a share well above that. The value is a proposal
/// of the reviewer of T-152, not a measurement (no engine exists); it is to be revisited with the first
/// measured share of T-153.
const MAX_NOT_CONVERGED_SHARE: f64 = 0.02;

/// A sample that is recorded, not asserted: the true value, mean, spread and bias of the estimates.
fn print_recorded(design: &str, what: &str, truth: f64, estimates: &[f64]) {
    if estimates.len() < 2 {
        eprintln!(
            "{design} {what}: truth {truth:e}, {} estimates, nothing to aggregate",
            estimates.len()
        );
        return;
    }
    let (mean, sd) = mean_sd(estimates);
    eprintln!(
        "{design} {what} (recorded, not asserted): truth {truth:e} mean {mean:e} sd {sd:e} bias {:e} over {} replicates",
        mean - truth,
        estimates.len()
    );
}

fn check_sealed_recovery(design: &str) {
    let sealed = load_pop_sealed(design).expect("the sealed design loads");
    verify_sealed_pinned(&sealed);
    let start = sealed_start(&sealed);
    let k = sealed.design.replicates;
    let pars: Vec<String> = sealed.truth.typical_values.keys().cloned().collect();
    let xis: Vec<String> = sealed.truth.xi.keys().cloned().collect();
    let q = sealed.model.random_effects.len();
    let mut estimates: Vec<Vec<f64>> = vec![Vec::new(); pars.len()];
    let mut covered: Vec<usize> = vec![0; pars.len()];
    // Omega (lower triangle, row-major) and xi: recorded, never asserted (the note of the sealed file)
    let mut omega_hat: Vec<Vec<f64>> = vec![Vec::new(); q * (q + 1) / 2];
    let mut xi_hat: Vec<Vec<f64>> = vec![Vec::new(); xis.len()];
    let mut not_converged: Vec<usize> = Vec::new();
    let mut at_bound: Vec<usize> = Vec::new();
    let c = &sealed.aggregate_checks.constants;
    for rep in 1..=k {
        let subjects = load_pop_sealed_replicate(&sealed, rep).expect("the replicate loads");
        let request = model_request(&start, &subjects, "foce_i", "full", false);
        let answer = population_fit(&request).unwrap_or_else(|e| {
            panic!("{design} replicate {rep}: the engine refuses valid data: {e}")
        });
        // FIT-CNV-03, FIT-ML-08, POP-EST-02: `at_bound` is a converged fit with a parameter on a bound. Its `se`
        // is that of the printed kind, the SE of the free block (FIT-BND-02, POP-ORA-04), and is used as is.
        match answer["status"].as_str() {
            Some("converged") => {}
            Some("at_bound") => at_bound.push(rep),
            _ => {
                not_converged.push(rep);
                continue;
            }
        }
        for (i, par) in pars.iter().enumerate() {
            let est = number(&answer["estimate"][par.as_str()], par);
            let se = number(&answer["se"][par.as_str()], par);
            let truth = f(&sealed.truth.typical_values[par]);
            estimates[i].push(est);
            if (est - truth).abs() <= c.c_interval * se {
                covered[i] += 1;
            }
        }
        for (i, xi) in xis.iter().enumerate() {
            if let Some(v) = answer["estimate"]["xi"][xi.as_str()].as_f64() {
                xi_hat[i].push(v);
            }
        }
        let mut at = 0;
        for row in 0..q {
            for col in 0..=row {
                if let Some(v) = answer["estimate"]["omega_full"][row][col].as_f64() {
                    omega_hat[at].push(v);
                }
                at += 1;
            }
        }
    }
    let n_ok = k - not_converged.len();
    let share_bad = not_converged.len() as f64 / k as f64;
    eprintln!(
        "{design}: {} of {k} replicates did not converge (status neither `converged` nor `at_bound`, {:.1}%), left out of the aggregates{}",
        not_converged.len(),
        100.0 * share_bad,
        if not_converged.is_empty() {
            String::new()
        } else {
            format!(
                "; first replicates {:?}",
                &not_converged[..not_converged.len().min(10)]
            )
        }
    );
    eprintln!(
        "{design}: {} of {k} replicates ended `at_bound` (counted as converged, kept in the aggregates)",
        at_bound.len()
    );
    // Omega and xi: printed, never asserted
    let mut at = 0;
    for row in 0..q {
        for col in 0..=row {
            let truth = f(&sealed.truth.omega_full[row][col]);
            let what = format!(
                "omega[{},{}]",
                sealed.model.random_effects[row], sealed.model.random_effects[col]
            );
            print_recorded(design, &what, truth, &omega_hat[at]);
            at += 1;
        }
    }
    for (i, xi) in xis.iter().enumerate() {
        print_recorded(
            design,
            &format!("xi {xi}"),
            f(&sealed.truth.xi[xi]),
            &xi_hat[i],
        );
    }
    let mut problems = Vec::new();
    if share_bad > MAX_NOT_CONVERGED_SHARE {
        problems.push(format!(
            "{} of {k} replicates did not converge (neither `converged` nor `at_bound`), more than the {:.0}% allowed",
            not_converged.len(),
            100.0 * MAX_NOT_CONVERGED_SHARE
        ));
    }
    for (i, par) in pars.iter().enumerate() {
        let truth = f(&sealed.truth.typical_values[par]);
        if n_ok < 2 {
            problems.push(format!(
                "{par}: {n_ok} converged replicates (`converged` or `at_bound`), nothing to aggregate"
            ));
            continue;
        }
        let (mean, sd) = mean_sd(&estimates[i]);
        let bias_limit = c.c_bias * sd / (n_ok as f64).sqrt();
        let share = covered[i] as f64 / n_ok as f64;
        let coverage_limit =
            c.c_coverage * (c.nominal_coverage * (1.0 - c.nominal_coverage) / n_ok as f64).sqrt();
        eprintln!(
            "{design} {par}: truth {truth:e} mean {mean:e} sd {sd:e} bias {:e} (limit {bias_limit:e}) coverage {share:.3} (nominal {} +- {coverage_limit:.3}) over {n_ok} replicates",
            mean - truth,
            c.nominal_coverage
        );
        if (mean - truth).abs() > bias_limit {
            problems.push(format!(
                "{par}: mean {mean:e} differs from the truth {truth:e} by more than {bias_limit:e}"
            ));
        }
        if (share - c.nominal_coverage).abs() > coverage_limit {
            problems.push(format!(
                "{par}: coverage {share:.3} is not {} within {coverage_limit:.3}",
                c.nominal_coverage
            ));
        }
    }
    if sealed.aggregate_checks.asserted {
        assert!(
            problems.is_empty(),
            "{design}: aggregate checks failed\n  {}",
            problems.join("\n  ")
        );
    } else {
        // recorded only (POP-ORA-04: the design that shows the limits of the method): the fits were run, the
        // numbers are printed above and in the report, nothing is asserted about them
        eprintln!(
            "{design}: recorded only; {} aggregate lines outside the checks",
            problems.len()
        );
    }
}

macro_rules! sealed_tests {
    ($($design:ident),* $(,)?) => {
        mod sealed_recovery {
            $(
                #[test]
                fn $design() { super::check_sealed_recovery(stringify!($design)); }
            )*
        }
    };
}

// ---------------------------------------------------------------- exact fits (card T-168)

/// The fit-level oracle: `oracle/expected/pop/fit_exact/`, the exact maximum of the balanced linear case
/// (`identity_linear`, additive error), where `foce_i` = `foce` = `fo` = the exact marginal likelihood. The
/// maximum is known in closed form (q = 1, q = 2 block) or by Newton's method on exact derivatives (the diagonal
/// case and the rank-one boundary of the block), at 256 bits, and three references agree on it
/// (`oracle/scripts/pop_fit_exact.R`). The engine is given the data from the generating values and from a far
/// start, with the method `foce_i`, and its answer is compared with the exact one:
///
/// * **each parameter** within the error that an OFV excess of `POP_OFV` allows: the OFV excess of an error `d`
///   is `d^T R d / 2` (`R` the Hessian of the OFV at the maximum), so `|d_j| <= sqrt(2 * POP_OFV * (R^-1)_jj)`,
///   with the diagonal of `R^-1` stored in the case (`tolerance_basis`). The bound does not depend on how the
///   engine stops, and it is as tight as the OFV tolerance already in force: no new constant is introduced;
/// * **the objective at the maximum** within `POP_OFV` (the engine cannot be better than the maximum, and it
///   must be as good as `POP_OFV` says);
/// * **a variance on its bound** (q = 1 or diagonal Omega, the between-subject sum of squares too small): the
///   OFV rises at the rate `gradient` per unit of the variance (stored), so a variance of at most
///   `POP_OFV / gradient` is within the OFV tolerance; the engine, which works on the log scale of Omega, cannot
///   reach 0 and must report the collapse (flag `omega_collapse`, POP-EST-04) or `at_bound`;
/// * **the rank-one boundary of the block** (Omega is singular at the maximum, POP-EST-04 has no flag for it):
///   theta, the elements of Omega, sigma and the objective are compared, all with the bounds of the curvature
///   (`tolerance_basis`, all the parameters). The comparison of Omega is sound: `G` (the gradient of the objective
///   with respect to Omega) is positive semidefinite at the maximum, so `tr(G (Omega - Omega_hat)) >= 0` for any
///   feasible Omega and the OFV excess of an error is at least `d^T R d / 2`. The status must not be `non_finite`,
///   `inner_failed` or `max_iterations`; the other statuses and the flags are not asserted;
/// * **a diagonal Omega of two or more random effects**: the off-diagonal element of the answer is exactly 0 (a
///   1 by 1 Omega has none);
/// * **an interior maximum**: status `converged`, no `at_bound` (status or flag), and no `few_subjects` /
///   `few_observations` flag (the designs have more subjects and observations than parameters, POP-COV-03,
///   POP-INN-03). `omega_collapse` is not asserted (it depends on OP-11);
/// * **the standard errors** (the three kinds of POP-COV-01, exact) are printed next to the engine's, never
///   asserted (OP-04, OP-11 are open).
fn fit_case(name: &str) -> PopFitCase {
    load_pop_fit_case(name).expect("the fit case loads")
}

fn fit_start<'a>(case: &'a PopFitCase, start: &str) -> &'a PopFitStart {
    case.starts
        .iter()
        .find(|s| s.name == start)
        .unwrap_or_else(|| panic!("{}: no start named `{start}`", case.id))
}

/// The request that asks for the maximum (no `evaluate_only`) from a start. The shape of Omega is given by the
/// case: the start of a block case may have zero off-diagonal elements.
fn fit_request(case: &PopFitCase, start: &PopFitStart, convention: &str) -> Value {
    let mut request = model_request(&start.model, &case.subjects, "foce_i", convention, false);
    request["omega_shape"] = json!(case.design.omega_shape);
    request
}

/// The indices of a label such as `omega_full[1][0]`.
fn omega_indices(label: &str) -> (usize, usize) {
    let idx: Vec<usize> = label
        .trim_start_matches("omega_full")
        .split(['[', ']'])
        .filter(|t| !t.is_empty())
        .map(|t| t.parse().expect("an index"))
        .collect();
    (idx[0], idx[1])
}

/// A labelled quantity of the answer, in `block` (`estimate` or `se`), if the answer has it as a number.
fn labelled(answer: &Value, block: &str, label: &str) -> Option<f64> {
    let b = &answer[block];
    match label {
        "theta1" | "theta2" => b[label].as_f64(),
        "xi.a" => b["xi"]["a"].as_f64(),
        other => {
            let (i, j) = omega_indices(other);
            b["omega_full"][i][j].as_f64()
        }
    }
}

fn answer_flags(answer: &Value) -> Vec<&str> {
    answer["covariance"]["flags"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn check_fit(name: &str, start: &str, convention: &str) {
    let case = fit_case(name);
    let request = fit_request(&case, fit_start(&case, start), convention);
    let answer = population_fit(&request)
        .unwrap_or_else(|e| panic!("{name} from `{start}`: the engine refuses a valid fit: {e}"));
    let mut p = Problems::new();
    // The off-diagonal element exists only when Omega is at least 2 by 2: the q1 cases have a 1 by 1 Omega
    // (`expected.omega` is q by q), and `omega_full[1][0]` is not in their answer (T-173).
    if case.design.omega_shape == "diagonal" && case.expected.omega.len() >= 2 {
        for label in ["omega_full[1][0]", "omega_full[0][1]"] {
            let off = labelled(&answer, "estimate", label)
                .unwrap_or_else(|| panic!("{name}: the answer has no `{label}` in `estimate`"));
            assert!(
                off == 0.0,
                "{name}: a diagonal Omega must have `{label}` exactly 0, the answer has {off:e}"
            );
        }
    }
    for label in case.labels() {
        let expected = case.expected_value(&label).expect("an exact value");
        let actual = labelled(&answer, "estimate", &label)
            .unwrap_or_else(|| panic!("{name}: the answer has no `{label}` in `estimate`"));
        let abs = if let Some(gradient) = case.bound_gradient(&label) {
            AbsoluteTolerance::POP_OFV.abs / gradient
        } else {
            case.curvature_bound(&label, AbsoluteTolerance::POP_OFV)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        p.check(
            &format!("{name} {label}"),
            actual,
            expected,
            AbsoluteTolerance { abs },
        );
    }
    let exact_ofv = f(if convention == "full" {
        &case.expected.ofv_full
    } else {
        &case.expected.ofv_no_2pi
    });
    p.check(
        &format!("{name} ofv ({convention})"),
        number(&answer["ofv"], "ofv"),
        exact_ofv,
        AbsoluteTolerance::POP_OFV,
    );
    assert_eq!(
        answer["ofv_convention"].as_str(),
        Some(convention),
        "{name}: ofv_convention"
    );
    let flags = answer_flags(&answer);
    let status = answer["status"].as_str().unwrap_or("");
    if case.expected.kind == "interior" {
        assert_eq!(status, "converged", "{name} from `{start}`: status");
        assert!(
            !flags.contains(&"at_bound"),
            "{name}: the flag `at_bound` is raised at an interior maximum: {flags:?}"
        );
        for bad in ["few_subjects", "few_observations"] {
            assert!(
                !flags.contains(&bad),
                "{name}: the flag `{bad}` is raised, and the design has more subjects and observations than parameters: {flags:?}"
            );
        }
    } else if case.design.shape != "block" {
        assert!(
            flags.contains(&"omega_collapse") || status == "at_bound",
            "{name} from `{start}`: a variance at 0 must be reported (POP-EST-04: `omega_collapse` or `at_bound`), status `{status}`, flags {flags:?}"
        );
    } else {
        assert!(
            !["non_finite", "inner_failed", "max_iterations"].contains(&status),
            "{name} from `{start}`: the fit failed, status `{status}`"
        );
    }
    print_fit_standard_errors(&case, &answer);
    p.finish(name);
}

/// The exact standard errors of the three kinds next to the engine's (recorded, not asserted).
fn print_fit_standard_errors(case: &PopFitCase, answer: &Value) {
    let se = &case.standard_errors;
    let labels: Vec<&str> = se["parameters"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for (i, label) in labels.iter().enumerate() {
        let kind = |k: &str| {
            se[k][i]
                .as_f64()
                .map_or("NC".to_owned(), |v| format!("{v:.6e}"))
        };
        let engine = labelled(answer, "se", label).map_or("NC".to_owned(), |v| format!("{v:.6e}"));
        eprintln!(
            "{}: se {label}: exact r {} s {} sandwich {}; engine {engine}",
            case.id,
            kind("r"),
            kind("s"),
            kind("sandwich")
        );
    }
}

macro_rules! fit_exact_case_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::*;
                const NAME: &str = stringify!($case);
                #[test]
                fn fit_from_the_generating_values() { check_fit(NAME, "generating_values", "full"); }
                #[test]
                fn fit_from_a_far_start() { check_fit(NAME, "far", "full"); }
                #[test]
                fn fit_with_the_ofv_without_2pi() { check_fit(NAME, "generating_values", "no_2pi"); }
            }
        )*
    };
}

// ---------------------------------------------------------------- the generated list of cases

include!("oracle_pop.cases");

// ---------------------------------------------------------------- guards (green today)

fn strings(list: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = list.iter().map(|s| (*s).to_owned()).collect();
    v.sort();
    v
}

/// Guards the guard: every case file of the oracle is run by a test above, and every case named above exists.
#[test]
fn every_case_row_and_design_has_tests() {
    let mut exact = strings(POP_EXACT_ENGINE_CASES);
    exact.extend(strings(POP_EXACT_FOCE_CASES));
    exact.extend(strings(POP_EXACT_IDENTITY_CASES));
    exact.sort();
    assert_eq!(
        list_pop_exact_cases().unwrap(),
        exact,
        "the exact cases on disk and the cases listed differ"
    );
    assert_eq!(
        list_pop_ofv_cases().unwrap(),
        strings(POP_OFV_CASES),
        "the ofv cases on disk and the cases tested differ"
    );
    assert_eq!(
        list_pop_fit_cases().unwrap(),
        strings(POP_FIT_EXACT_CASES),
        "the exact fit cases on disk and the cases tested differ"
    );
    let rows = load_pop_errors().unwrap();
    let on_disk = |expect: &str, constructible: bool| -> Vec<String> {
        let mut v: Vec<String> = rows
            .iter()
            .filter(|r| r.expect == expect && r.constructible == constructible)
            .map(|r| r.case.clone())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        on_disk("refused", true),
        strings(POP_REFUSALS),
        "refused rows"
    );
    assert_eq!(
        on_disk("accepted", true),
        strings(POP_ACCEPTED),
        "accepted rows"
    );
    assert_eq!(
        on_disk("refused", false),
        strings(POP_SENTENCE_ONLY),
        "sentence-only rows"
    );
    assert_eq!(
        strings(&POP_SEALED_DESIGNS),
        strings(POP_SEALED),
        "the sealed designs on disk and tested differ"
    );
}

/// The exact cases are split by the frozen variance (POP-OBJ-06): none for `foce_i` and `fo`, at the population
/// prediction for `foce`, at the mode for the identity checks of the testkit.
#[test]
fn engine_foce_and_identity_cases_are_split_by_the_frozen_variance() {
    for (list, frozen) in [
        (POP_EXACT_ENGINE_CASES, "none"),
        (POP_EXACT_FOCE_CASES, "pred0"),
        (POP_EXACT_IDENTITY_CASES, "mode"),
    ] {
        assert!(!list.is_empty(), "{frozen}");
        for name in list {
            assert_eq!(
                load_pop_exact_case(name).unwrap().frozen_sigma,
                frozen,
                "{name}"
            );
        }
    }
}

/// The sealed files are recomputed against their hashes before they are used, and the aggregate rule is the
/// one written in the file (POP-ORA-04 items 3 and 5).
#[test]
fn the_sealed_hashes_are_recomputed() {
    for design in POP_SEALED {
        let sealed = load_pop_sealed(design).unwrap();
        assert!(sealed.sealed, "{design}");
        verify_sealed_pinned(&sealed);
        assert_eq!(sealed.design.replicates, 200, "{design}");
    }
}

/// The reference loads, every subject has the quantities this file compares, and the requests built from the
/// cases are well formed, so that a red test is red because of the engine.
#[test]
fn the_reference_is_complete_for_the_tests() {
    for name in POP_OFV_CASES {
        let case = ofv_case(name);
        assert_eq!(
            case.subjects.len(),
            case.expected.per_subject.len(),
            "{name}"
        );
        for (s, e) in case.subjects.iter().zip(&case.expected.per_subject) {
            assert_eq!(e.n_obs, s.time.len(), "{name} {}", e.id);
            assert_eq!(
                e.diagnostics.cwres_symmetric.len(),
                e.n_obs,
                "{name} {}",
                e.id
            );
            for form in ["linearised", "fo"] {
                assert!(
                    e.ofv_full.contains_key(form) && e.ofv_no_2pi.contains_key(form),
                    "{name} {form}"
                );
            }
        }
        for form in POP_FORMS {
            assert!(case.expected.total_full.contains_key(form), "{name} {form}");
        }
        assert!(
            case.diagnostics_population
                .information_criteria_full
                .contains_key("linearised"),
            "{name}"
        );
        let request = model_request(&case.model, &case.subjects, "foce_i", "full", true);
        assert_eq!(
            request["subjects"].as_array().unwrap().len(),
            case.subjects.len(),
            "{name}"
        );
    }
    for name in POP_EXACT_FOCE_CASES {
        let case = exact_case(name, "pred0");
        assert_eq!(
            case.subjects.len(),
            case.expected.per_subject.len(),
            "{name}"
        );
        assert!(
            case.expected.total_full.contains_key("linearised"),
            "{name}"
        );
        let request = model_request(&case.model, &case.subjects, "foce", "full", true);
        assert_eq!(
            request["subjects"].as_array().unwrap().len(),
            case.subjects.len(),
            "{name}"
        );
    }
    for name in POP_EXACT_ENGINE_CASES {
        let case = exact_case(name, "none");
        assert_eq!(
            case.subjects.len(),
            case.expected.per_subject.len(),
            "{name}"
        );
        for form in ["linearised", "fo"] {
            assert!(case.expected.total_full.contains_key(form), "{name} {form}");
        }
        let request = model_request(&case.model, &case.subjects, "fo", "full", true);
        assert_eq!(
            request["model"].as_str(),
            Some(case.model.name.as_str()),
            "{name}"
        );
    }
    for name in POP_FIT_EXACT_CASES {
        let case = fit_case(name);
        assert_eq!(case.family, "d_fit_exact", "{name}");
        assert_eq!(case.subjects.len(), case.design.m, "{name}");
        assert!(case.starts.len() >= 2, "{name}: two starts");
        for start in &case.starts {
            let request = fit_request(&case, start, "full");
            assert_eq!(
                request["subjects"].as_array().unwrap().len(),
                case.subjects.len(),
                "{name}"
            );
            assert_eq!(
                request["omega_shape"].as_str(),
                Some(case.design.omega_shape.as_str()),
                "{name}"
            );
            assert!(
                request["options"]["evaluate_only"] == json!(false),
                "{name}"
            );
        }
        for label in case.labels() {
            case.expected_value(&label).expect("an exact value");
        }
    }
    for row in load_pop_errors().unwrap() {
        if row.constructible {
            assert!(row.request.is_some(), "{}", row.case);
        }
    }
}
