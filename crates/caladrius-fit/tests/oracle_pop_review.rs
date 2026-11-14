#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
//! Oracle tests for blocker 1 of the review of T-154 (card T-188), written from `specs/population.md`
//! and the oracle, not from the engine. Rules cited as POP-*; a test that fails stays failing with its
//! numbers and its rule: the engine is fixed in another card.
//!
//! Items: (1) the three covariance kinds against the exact standard errors of `fit_exact/`;
//! (2) RSE, correlation, eigenvalues and condition number recomputed from the printed covariance;
//! (5) `bounds` and a fixed Omega; (4) the statuses; (3) the two-stage start.

use std::fmt;

use caladrius_testkit::{
    AbsoluteTolerance, PopFitCase, PopFitStart, PopModel, PopSubject, Tolerance, load_pop_fit_case,
    load_pop_sealed, load_pop_sealed_replicate,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------- the adapter (same as `oracle_pop.rs`)

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

fn population_fit(request: &Value) -> Result<Value, PopError> {
    let input: caladrius_fit::pop::PopRequest =
        serde_json::from_value(request.clone()).map_err(|e| PopError {
            code: "invalid_request".to_owned(),
            message: e.to_string(),
        })?;
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

// ---------------------------------------------------------------- helpers

fn f(text: &str) -> f64 {
    text.trim()
        .parse::<f64>()
        .unwrap_or_else(|e| panic!("`{text}` is not a number: {e}"))
}

/// A request from a model block and subjects (the vocabulary of `oracle_pop.rs`).
fn request(model: &PopModel, shape: &str, subjects: &[PopSubject]) -> Value {
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
        "omega_shape": shape,
        "error_model": model.error_model,
        "error_init": error_init,
        "options": { "method": "foce_i", "ofv_convention": "full" },
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

fn fit_case(name: &str) -> PopFitCase {
    load_pop_fit_case(name).expect("the fit case loads")
}

fn fit_start<'a>(case: &'a PopFitCase, start: &str) -> &'a PopFitStart {
    case.starts
        .iter()
        .find(|s| s.name == start)
        .unwrap_or_else(|| panic!("{}: no start named `{start}`", case.id))
}

fn case_request(case: &PopFitCase, start: &str) -> Value {
    request(
        &fit_start(case, start).model,
        &case.design.omega_shape,
        &case.subjects,
    )
}

fn fit_ok(request: &Value, what: &str) -> Value {
    population_fit(request)
        .unwrap_or_else(|e| panic!("{what}: the engine refuses a valid fit: {e}"))
}

/// Mismatches are collected so that one failing test prints every number.
struct Problems(Vec<String>);

impl Problems {
    fn new() -> Self {
        Problems(Vec::new())
    }
    fn say(&mut self, line: String) {
        self.0.push(line);
    }
    fn finish(self, what: &str) {
        assert!(
            self.0.is_empty(),
            "{what}: {} problem(s)\n  {}",
            self.0.len(),
            self.0.join("\n  ")
        );
    }
}

/// A labelled quantity of a block of the answer (`estimate`, `se`, or `se_by_kind.<kind>`): `theta1`,
/// `xi.a`, `omega_full[i][j]` (POP-API-01 item 13).
fn labelled(block: &Value, label: &str) -> Option<f64> {
    match label {
        "xi.a" => block["xi"]["a"].as_f64(),
        l if l.starts_with("omega_full") => {
            let idx: Vec<usize> = l
                .trim_start_matches("omega_full")
                .split(['[', ']'])
                .filter(|t| !t.is_empty())
                .map(|t| t.parse().expect("an index"))
                .collect();
            block["omega_full"][idx[0]][idx[1]].as_f64()
        }
        other => block[other].as_f64(),
    }
}

// ---------------------------------------------------------------- item 1: the three kinds, exact

const INTERIOR_CASES: [&str; 3] = ["fit_q1_h38", "fit_q2_diag", "fit_q2_block"];
const KINDS: [&str; 3] = ["sandwich", "r", "s"];

/// The oracle's guard, no engine: the exact `r` standard errors in `standard_errors` are the square roots of
/// `2 * rinv_diag` of `tolerance_basis` (the Hessian basis of the same case), so the two stored sets agree.
#[test]
fn the_stored_exact_r_standard_errors_agree_with_the_curvature_basis() {
    for name in INTERIOR_CASES {
        let case = fit_case(name);
        let labels: Vec<&str> = case.standard_errors["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            labels,
            case.tolerance_basis
                .parameters
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            "{name}: the two parameter lists"
        );
        for (i, label) in labels.iter().enumerate() {
            let r = case.standard_errors["r"][i].as_f64().unwrap();
            let basis = (2.0 * case.tolerance_basis.rinv_diag[i]).sqrt();
            assert!(
                Tolerance::MODEL_VALUES.accepts(basis, r),
                "{name} {label}: sqrt(2 rinv_diag) = {basis:e} but the stored exact r SE is {r:e}"
            );
        }
    }
}

fn check_se_kinds(name: &str, start: &str) -> [f64; 3] {
    let case = fit_case(name);
    let answer = fit_ok(
        &case_request(&case, start),
        &format!("{name} from `{start}`"),
    );
    let tol = Tolerance::POP_SE;
    let labels: Vec<&str> = case.standard_errors["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut p = Problems::new();
    let mut worst = [0.0f64; 3];
    for (k, kind) in KINDS.iter().enumerate() {
        let block = &answer["se_by_kind"][*kind];
        for (i, label) in labels.iter().enumerate() {
            let expected = case.standard_errors[*kind][i].as_f64().unwrap();
            match labelled(block, label) {
                None => p.say(format!(
                    "{kind} {label}: the engine has no SE (NC), expected {expected:e}"
                )),
                Some(actual) => {
                    let err = tol.error(actual, expected);
                    worst[k] = worst[k].max(err);
                    if !tol.accepts(actual, expected) {
                        p.say(format!(
                            "{kind} {label}: engine {actual:e}, exact {expected:e}, relative error {err:.3e}, {tol}"
                        ));
                    }
                }
            }
        }
    }
    eprintln!(
        "{name} from `{start}`: worst relative SE error: sandwich {:.3e}, r {:.3e}, s {:.3e}",
        worst[0], worst[1], worst[2]
    );
    p.finish(&format!(
        "{name} from `{start}` (POP-COV-01, exact SEs of the three kinds)"
    ));
    worst
}

macro_rules! se_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                #[test]
                fn se_of_the_three_kinds_from_the_generating_values() {
                    super::check_se_kinds(stringify!($case), "generating_values");
                }
                #[test]
                fn se_of_the_three_kinds_from_a_far_start() {
                    super::check_se_kinds(stringify!($case), "far");
                }
            }
        )*
    };
}

se_tests!(fit_q1_h38, fit_q2_diag, fit_q2_block);

// ---------------------------------------------------------------- item 1b: the printed kind (POP-COV-01)

fn with_covariance_kind(mut request: Value, kind: &str) -> Value {
    request["options"]["covariance"] = json!(kind);
    request
}

/// `options.covariance` picks only the kind that is printed: `se` is `se_by_kind[kind]`, `covariance.printed` names
/// it, and the three kinds are returned whatever the option is (POP-COV-01).
#[test]
fn the_option_covariance_picks_the_printed_kind_and_the_three_are_always_returned() {
    let case = fit_case("fit_q2_diag");
    let base = case_request(&case, "generating_values");
    let default = fit_ok(&base, "default printed kind");
    assert_eq!(
        default["covariance"]["printed"].as_str(),
        Some("sandwich"),
        "POP-COV-01: the sandwich is printed by default"
    );
    let mut p = Problems::new();
    for kind in KINDS {
        let answer = fit_ok(&with_covariance_kind(base.clone(), kind), kind);
        if answer["covariance"]["printed"].as_str() != Some(kind) {
            p.say(format!(
                "covariance = {kind}: `covariance.printed` is {}",
                answer["covariance"]["printed"]
            ));
        }
        if answer["se"] != answer["se_by_kind"][kind] {
            p.say(format!(
                "covariance = {kind}: `se` is not `se_by_kind.{kind}`: {} against {}",
                answer["se"], answer["se_by_kind"][kind]
            ));
        }
        for other in KINDS {
            if answer["se_by_kind"][other].is_null()
                || answer["covariance"][other]["matrix"].is_null()
            {
                p.say(format!(
                    "covariance = {kind}: the kind `{other}` is not returned"
                ));
            }
            if default["se_by_kind"][other] != answer["se_by_kind"][other] {
                p.say(format!(
                    "covariance = {kind}: `se_by_kind.{other}` changes with the printed kind"
                ));
            }
        }
    }
    let mut bad = base.clone();
    bad["options"]["covariance"] = json!("banana");
    match population_fit(&bad) {
        Err(e) if e.code == "invalid_option" => {}
        other => p.say(format!(
            "covariance = banana: POP-COV-01 says `invalid_option`, got {other:?}"
        )),
    }
    p.finish("printed kind (POP-COV-01)");
}

// ---------------------------------------------------------------- item 2: derived statistics (POP-COV-04)

/// Eigenvalues of a symmetric matrix by cyclic Jacobi rotations (independent of the engine), decreasing.
#[allow(clippy::needless_range_loop)] // index arithmetic of a rotation reads better with indices
fn jacobi_eigenvalues(a: &[Vec<f64>]) -> Vec<f64> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    for _ in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| m[i][j] * m[i][j])
            .sum();
        if off == 0.0 {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                if m[p][q] == 0.0 {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (kp, kq) = (m[k][p], m[k][q]);
                    m[k][p] = c * kp - s * kq;
                    m[k][q] = s * kp + c * kq;
                }
                for k in 0..n {
                    let (pk, qk) = (m[p][k], m[q][k]);
                    m[p][k] = c * pk - s * qk;
                    m[q][k] = s * pk + c * qk;
                }
            }
        }
    }
    let mut ev: Vec<f64> = (0..n).map(|i| m[i][i]).collect();
    ev.sort_by(|x, y| y.partial_cmp(x).unwrap());
    ev
}

fn matrix_of(v: &Value) -> Vec<Vec<f64>> {
    v.as_array()
        .unwrap_or_else(|| panic!("a matrix is expected, got {v}"))
        .iter()
        .map(|r| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect()
        })
        .collect()
}

/// The (row, column) of an Omega label `omega[a,b]` in the order of `random_effects`.
fn omega_position(answer: &Value, label: &str) -> Option<(usize, usize)> {
    let inner = label.strip_prefix("omega[")?;
    let names: Vec<&str> = inner.trim_end_matches(']').split(',').collect();
    let re: Vec<&str> = answer["random_effects"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    Some((
        re.iter().position(|r| *r == names[0])?,
        re.iter().position(|r| *r == names[1])?,
    ))
}

/// A quantity of `estimate`, `se` or one `se_by_kind` block, by the covariance label (`theta1`, `omega[a,b]`,
/// `xi.a`).
fn by_label(block: &Value, answer: &Value, label: &str) -> Option<f64> {
    if let Some((i, j)) = omega_position(answer, label) {
        block["omega_full"][i][j].as_f64()
    } else if label == "xi.a" {
        block["xi"]["a"].as_f64()
    } else {
        block[label].as_f64()
    }
}

/// RSE, correlation, eigenvalues and condition number are those of the printed kind (POP-COV-04), recomputed here
/// from the printed matrix: SE_j = sqrt(C_jj); RSE_j = 100 SE_j / |estimate_j|; Corr = C_jk / (SE_j SE_k); the
/// eigenvalues of the correlation matrix, decreasing, and kappa = largest / smallest (FIT-OUT-05).
fn check_derived(name: &str, kind: &str) {
    let case = fit_case(name);
    let request = with_covariance_kind(case_request(&case, "generating_values"), kind);
    let answer = fit_ok(&request, &format!("{name} printing `{kind}`"));
    let cov = &answer["covariance"];
    let labels: Vec<String> = cov["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    let c = matrix_of(&cov[kind]["matrix"]);
    let p = labels.len();
    let tol = Tolerance::FIT_PARAMETERS_SMALL_CASE;
    let mut probs = Problems::new();
    let se: Vec<f64> = (0..p).map(|j| c[j][j].sqrt()).collect();
    for (j, label) in labels.iter().enumerate() {
        match by_label(&answer["se"], &answer, label) {
            Some(v) if tol.accepts(v, se[j]) => {}
            other => probs.say(format!(
                "se {label}: sqrt(C_jj) = {:e}, the answer's `se` has {other:?}",
                se[j]
            )),
        }
    }
    for (j, label) in labels.iter().enumerate() {
        let estimate = by_label(&answer["estimate"], &answer, label).unwrap();
        let expected = 100.0 * se[j] / estimate.abs();
        match cov["rse"][label.as_str()].as_f64() {
            Some(v) if tol.accepts(v, expected) => {}
            other => probs.say(format!(
                "rse {label}: 100 SE/|estimate| = {expected:e}, the answer has {other:?}"
            )),
        }
    }
    let corr: Vec<Vec<f64>> = (0..p)
        .map(|j| (0..p).map(|k| c[j][k] / (se[j] * se[k])).collect())
        .collect();
    let engine_corr = matrix_of(&cov["correlation"]);
    for j in 0..p {
        for k in 0..p {
            let (a, e) = (engine_corr[j][k], corr[j][k]);
            if !tol.accepts(a, e) {
                probs.say(format!(
                    "correlation [{}][{}]: recomputed {e:e}, the answer has {a:e}",
                    labels[j], labels[k]
                ));
            }
        }
    }
    let ev = jacobi_eigenvalues(&corr);
    let engine_ev: Vec<f64> = cov["eigenvalues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    if engine_ev.len() != ev.len() {
        probs.say(format!(
            "eigenvalues: {} expected, {} returned",
            ev.len(),
            engine_ev.len()
        ));
    } else {
        for (i, (a, e)) in engine_ev.iter().zip(&ev).enumerate() {
            if !tol.accepts(*a, *e) {
                probs.say(format!(
                    "eigenvalue {}: recomputed {e:e}, the answer has {a:e}",
                    i + 1
                ));
            }
        }
    }
    let kappa = ev[0] / ev[p - 1];
    match cov["condition_number"].as_f64() {
        Some(v) if tol.accepts(v, kappa) => {}
        other => probs.say(format!(
            "condition number: largest/smallest eigenvalue of the correlation matrix = {kappa:e}, the answer has {other:?}"
        )),
    }
    probs.finish(&format!(
        "{name}, printed kind `{kind}` (POP-COV-04, FIT-OUT-05)"
    ));
}

macro_rules! derived_tests {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                #[test]
                fn derived_statistics_of_the_sandwich() { super::super::check_derived(stringify!($case), "sandwich"); }
                #[test]
                fn derived_statistics_of_r() { super::super::check_derived(stringify!($case), "r"); }
                #[test]
                fn derived_statistics_of_s() { super::super::check_derived(stringify!($case), "s"); }
            }
        )*
    };
}

mod derived {
    derived_tests!(fit_q1_h38, fit_q2_diag, fit_q2_block);
}

/// The fit of `fit_q2_block` with its sandwich covariance and the positions of the three Omega elements in it.
fn block_fit() -> (Value, Vec<Vec<f64>>, [usize; 3], [f64; 3]) {
    let case = fit_case("fit_q2_block");
    let answer = fit_ok(&case_request(&case, "generating_values"), "fit_q2_block");
    let labels: Vec<String> = answer["covariance"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    let c = matrix_of(&answer["covariance"]["sandwich"]["matrix"]);
    let ix = |l: &str| labels.iter().position(|x| x == l).unwrap();
    let idx = [
        ix("omega[intercept,intercept]"),
        ix("omega[slope,intercept]"),
        ix("omega[slope,slope]"),
    ];
    let v = [
        answer["estimate"]["omega_full"][0][0].as_f64().unwrap(),
        answer["estimate"]["omega_full"][1][0].as_f64().unwrap(),
        answer["estimate"]["omega_full"][1][1].as_f64().unwrap(),
    ];
    (answer, c, idx, v)
}

/// The standard deviation of a random effect has the delta-method SE of the variance divided by 2 sd (POP-COV-04:
/// "derived values with their own delta-method standard errors").
#[test]
fn the_standard_error_of_an_omega_standard_deviation_follows_the_delta_method() {
    let (answer, c, idx, v) = block_fit();
    let tol = Tolerance::FIT_PARAMETERS_SMALL_CASE;
    let mut p = Problems::new();
    for (name, i, var) in [("intercept", idx[0], v[0]), ("slope", idx[2], v[2])] {
        let expected = c[i][i].sqrt() / (2.0 * var.sqrt());
        match answer["se"]["omega_sd"][name].as_f64() {
            Some(a) if tol.accepts(a, expected) => {}
            other => p.say(format!(
                "se.omega_sd.{name}: SE(variance) / (2 sd) = {expected:e}, the answer has {other:?}"
            )),
        }
    }
    p.finish("derived Omega standard deviations (POP-COV-04)");
}

/// The correlation of a block has the delta-method SE of C_12 / sqrt(C_11 C_22) from the 3 by 3 covariance of
/// (Omega_11, Omega_21, Omega_22) (POP-COV-04). The key `se.omega_correlation` mirrors `estimate.omega_correlation`;
/// its name is the plainest, not the spec's. Fails today: the answer has no SE for the correlations (the review's
/// blocker 2, card T-154 goal 4).
#[test]
fn the_standard_error_of_an_omega_correlation_follows_the_delta_method() {
    let (answer, c, idx, v) = block_fit();
    let (v1, c12, v2) = (v[0], v[1], v[2]);
    let rho = c12 / (v1 * v2).sqrt();
    let g = [-rho / (2.0 * v1), 1.0 / (v1 * v2).sqrt(), -rho / (2.0 * v2)];
    let mut var = 0.0;
    for a in 0..3 {
        for b in 0..3 {
            var += g[a] * c[idx[a]][idx[b]] * g[b];
        }
    }
    let expected = var.sqrt();
    match answer["se"]["omega_correlation"][1][0].as_f64() {
        Some(a) if Tolerance::FIT_PARAMETERS_SMALL_CASE.accepts(a, expected) => {}
        other => panic!(
            "se.omega_correlation[1][0]: the delta-method SE of the correlation is {expected:e}, the answer has {other:?} (POP-COV-04)"
        ),
    }
}

// ---------------------------------------------------------------- item 5: bounds and a fixed Omega (POP-EST-02)

/// The exact optimum of an interior case against an answer: theta, the elements of Omega and sigma within the
/// curvature bound of `POP_OFV` (the rule of `oracle_pop.rs`), the objective within `POP_OFV`. `skip` lists labels not
/// compared (a parameter held somewhere else).
fn compare_exact(case: &PopFitCase, answer: &Value, skip: &[&str], p: &mut Problems) {
    for label in case.labels() {
        if skip.contains(&label.as_str()) {
            continue;
        }
        let expected = case.expected_value(&label).unwrap();
        let abs = case
            .curvature_bound(&label, AbsoluteTolerance::POP_OFV)
            .unwrap();
        match labelled(&answer["estimate"], &label) {
            Some(actual) if (actual - expected).abs() <= abs => {}
            other => p.say(format!(
                "{label}: engine {other:?}, exact {expected:e} (allowed difference {abs:e})"
            )),
        }
    }
    let exact_ofv = f(&case.expected.ofv_full);
    match answer["ofv"].as_f64() {
        Some(a) if AbsoluteTolerance::POP_OFV.accepts(a, exact_ofv) => {}
        other => p.say(format!("ofv: engine {other:?}, exact {exact_ofv:e}")),
    }
}

/// A binding bound: the value comes back on the bound, the status and the flag say `at_bound`, its SE is null (never
/// 0) in every kind, the others are conditional and have one, and the optimum is the one with that parameter held
/// there (a different request, `fixed`, of the same problem: the active-set condition of FIT-BND-01).
fn check_binding_bound(parameter: &str, bound: Value, on: f64, start: f64) {
    let case = fit_case("fit_q2_diag");
    let mut req = case_request(&case, "generating_values");
    req["initial"][parameter] = json!(start);
    req["bounds"] = json!({ parameter: bound });
    let what = format!("{parameter} bound at {on}");
    let answer = fit_ok(&req, &what);
    let mut p = Problems::new();
    if answer["status"].as_str() != Some("at_bound") {
        p.say(format!(
            "status {}, expected `at_bound` (POP-EST-02)",
            answer["status"]
        ));
    }
    if !answer["covariance"]["flags"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x == "at_bound"))
    {
        p.say(format!(
            "the flag `at_bound` is missing: {}",
            answer["covariance"]["flags"]
        ));
    }
    if answer["estimate"][parameter].as_f64() != Some(on) {
        p.say(format!(
            "estimate.{parameter} is {}, expected exactly {on} (on the bound)",
            answer["estimate"][parameter]
        ));
    }
    for block in [
        &answer["se"],
        &answer["se_by_kind"]["sandwich"],
        &answer["se_by_kind"]["r"],
        &answer["se_by_kind"]["s"],
    ] {
        if !block[parameter].is_null() {
            p.say(format!(
                "SE of the parameter on its bound is {}, expected null (NC, POP-COV-03 `at_bound`, never 0)",
                block[parameter]
            ));
        }
        if block["xi"]["a"].is_null() || block["omega_full"][0][0].is_null() {
            p.say(format!(
                "the other parameters are conditional on the bound and have an SE (POP-EST-02, FIT-BND-02): {block}"
            ));
        }
    }
    // the unconstrained optimum is the minimum: a constrained one cannot be lower
    let exact_ofv = f(&case.expected.ofv_full);
    match answer["ofv"].as_f64() {
        Some(o) if o >= exact_ofv - AbsoluteTolerance::POP_OFV.abs => {}
        other => p.say(format!(
            "ofv {other:?} is below the unconstrained exact minimum {exact_ofv:e}"
        )),
    }
    // the same problem with the parameter fixed on the bound
    let mut fixed = case_request(&case, "generating_values");
    fixed["initial"].as_object_mut().unwrap().remove(parameter);
    fixed["fixed"] = json!({ parameter: on });
    match population_fit(&fixed) {
        Err(e) => p.say(format!("the control with `fixed` is refused: {e}")),
        Ok(control) => {
            match (answer["ofv"].as_f64(), control["ofv"].as_f64()) {
                (Some(a), Some(c)) if AbsoluteTolerance::POP_OFV.accepts(a, c) => {}
                (a, c) => p.say(format!(
                    "ofv with the bound {a:?}, with the parameter fixed there {c:?}: they must agree to POP_OFV"
                )),
            }
            for label in [
                "theta1",
                "theta2",
                "omega_full[0][0]",
                "omega_full[1][1]",
                "xi.a",
            ] {
                if label == parameter {
                    continue;
                }
                match (
                    labelled(&answer["estimate"], label),
                    labelled(&control["estimate"], label),
                ) {
                    (Some(a), Some(c)) if Tolerance::FIT_PARAMETERS.accepts(a, c) => {}
                    (a, c) => p.say(format!(
                        "{label}: with the bound {a:?}, with the parameter fixed there {c:?} ({})",
                        Tolerance::FIT_PARAMETERS
                    )),
                }
            }
        }
    }
    p.finish(&format!("{what} (POP-EST-02)"));
}

#[test]
fn an_upper_bound_that_binds_holds_theta1_on_it() {
    check_binding_bound("theta1", json!({ "upper": 9.0 }), 9.0, 8.5);
}

#[test]
fn a_lower_bound_that_binds_holds_theta1_on_it() {
    check_binding_bound("theta1", json!({ "lower": 11.0 }), 11.0, 12.0);
}

#[test]
fn an_upper_bound_that_binds_holds_theta2_on_it() {
    check_binding_bound("theta2", json!({ "upper": -0.9 }), -0.9, -1.0);
}

/// A bound that does not bind changes nothing: the exact optimum, status `converged`, no `at_bound`, an SE for theta1.
#[test]
fn a_bound_that_does_not_bind_gives_the_exact_optimum() {
    let case = fit_case("fit_q2_diag");
    let mut req = case_request(&case, "generating_values");
    req["bounds"] = json!({ "theta1": { "lower": 5.0, "upper": 15.0 } });
    let answer = fit_ok(&req, "theta1 in [5, 15]");
    let mut p = Problems::new();
    compare_exact(&case, &answer, &[], &mut p);
    if answer["status"].as_str() != Some("converged") {
        p.say(format!("status {}, expected `converged`", answer["status"]));
    }
    if answer["covariance"]["flags"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x == "at_bound"))
    {
        p.say("the flag `at_bound` is raised for a bound that does not bind".to_owned());
    }
    if answer["se"]["theta1"].is_null() {
        p.say("the SE of theta1 is null although its bound does not bind".to_owned());
    }
    p.finish("a bound that does not bind (POP-EST-02)");
}

/// The fit of `fit_q2_diag` from the far start with the diagonal Omega fixed at the exact optimum. The key
/// `omega.<random effect>` is the variance and the engine's, the spec is silent (POP-API-01 item 12 names `fixed`
/// for typical values and error parameters only).
fn fixed_diagonal_fit(case: &PopFitCase) -> (Value, f64, f64) {
    let v1 = case.expected_value("omega_full[0][0]").unwrap();
    let v2 = case.expected_value("omega_full[1][1]").unwrap();
    let mut req = case_request(case, "far");
    req["omega"] = json!([[v1, 0.0], [0.0, v2]]);
    req["fixed"] = json!({ "omega.intercept": v1, "omega.slope": v2 });
    (
        fit_ok(&req, "diagonal Omega fixed at the exact optimum"),
        v1,
        v2,
    )
}

/// POP-EST-02: a fixed diagonal Omega comes back exactly (the card: to the last bit).
#[test]
fn a_fixed_diagonal_omega_comes_back_exactly() {
    let case = fit_case("fit_q2_diag");
    let (answer, v1, v2) = fixed_diagonal_fit(&case);
    let mut p = Problems::new();
    for (label, want) in [("omega_full[0][0]", v1), ("omega_full[1][1]", v2)] {
        let got = labelled(&answer["estimate"], label);
        if got != Some(want) {
            p.say(format!(
                "{label} is {got:?}, a fixed value must come back exactly ({want:e})"
            ));
        }
    }
    p.finish("a fixed diagonal Omega comes back exactly (POP-EST-02)");
}

/// A maximum with Omega held at its own optimum is the same point: theta, sigma and the objective are the exact
/// ones, from a far start, and the SE of a fixed element is null (NC), never 0.
#[test]
fn with_a_fixed_diagonal_omega_the_rest_is_the_exact_optimum_and_its_se_is_null() {
    let case = fit_case("fit_q2_diag");
    let (answer, _, _) = fixed_diagonal_fit(&case);
    let mut p = Problems::new();
    compare_exact(
        &case,
        &answer,
        &["omega_full[0][0]", "omega_full[1][1]"],
        &mut p,
    );
    for block in [&answer["se"], &answer["se_by_kind"]["r"]] {
        for label in ["omega_full[0][0]", "omega_full[1][1]"] {
            if labelled(block, label).is_some() {
                p.say(format!(
                    "SE of the fixed {label} is {:?}, expected null (NC, POP-COV-03 `at_bound`)",
                    labelled(block, label)
                ));
            }
        }
    }
    p.finish("a fixed diagonal Omega, the rest (POP-EST-02)");
}

/// One value fixed that is not round, so that a round trip through a logarithm shows: the reviewer saw 0.16 come
/// back as 0.16000000000000003.
#[test]
fn a_fixed_variance_that_is_not_round_comes_back_to_the_last_bit() {
    let case = fit_case("fit_q2_diag");
    let mut req = case_request(&case, "generating_values");
    req["omega"] = json!([[4.0, 0.0], [0.0, 0.16]]);
    req["fixed"] = json!({ "omega.slope": 0.16 });
    let answer = fit_ok(&req, "omega.slope fixed at 0.16");
    assert_eq!(
        answer["estimate"]["omega_full"][1][1].as_f64(),
        Some(0.16),
        "POP-EST-02: a fixed variance comes back exactly"
    );
    assert_eq!(
        answer["estimate"]["omega_diagonal"]["slope"].as_f64(),
        Some(0.16),
        "POP-EST-02: a fixed variance comes back exactly (omega_diagonal)"
    );
}

/// The fit of `fit_q2_block` from the far start with the whole block fixed at the exact optimum. The engine's rule
/// for a whole block is "every `omega.<parameter>` equal to the diagonal of `omega`" (its own message; the spec says
/// only "all of its log-Cholesky entries together").
fn fixed_block_fit(case: &PopFitCase) -> (Value, [f64; 3]) {
    let v1 = case.expected_value("omega_full[0][0]").unwrap();
    let c = case.expected_value("omega_full[1][0]").unwrap();
    let v2 = case.expected_value("omega_full[1][1]").unwrap();
    let mut req = case_request(case, "far");
    req["omega"] = json!([[v1, c], [c, v2]]);
    req["fixed"] = json!({ "omega.intercept": v1, "omega.slope": v2 });
    (
        fit_ok(&req, "block Omega fixed at the exact optimum"),
        [v1, c, v2],
    )
}

/// POP-EST-02: a fixed whole block comes back exactly, every element (the card: to the last bit). The covariance is
/// rebuilt from log-Cholesky entries (POP-EST-01), so a 1-ulp difference there may be a design cost: the
/// orchestrator decides whether this test relaxes it.
#[test]
fn a_fixed_whole_block_comes_back_exactly() {
    let case = fit_case("fit_q2_block");
    let (answer, [v1, c, v2]) = fixed_block_fit(&case);
    let mut p = Problems::new();
    for (i, j, want) in [(0, 0, v1), (1, 0, c), (0, 1, c), (1, 1, v2)] {
        let got = answer["estimate"]["omega_full"][i][j].as_f64();
        if got != Some(want) {
            p.say(format!(
                "omega_full[{i}][{j}] is {got:?}, a fixed value must come back exactly ({want:e})"
            ));
        }
    }
    p.finish("a fixed whole block comes back exactly (POP-EST-02)");
}

/// With the whole block held at its own optimum, theta, sigma and the objective are the exact ones.
#[test]
fn with_a_fixed_whole_block_the_rest_is_the_exact_optimum() {
    let case = fit_case("fit_q2_block");
    let (answer, _) = fixed_block_fit(&case);
    let mut p = Problems::new();
    compare_exact(
        &case,
        &answer,
        &["omega_full[0][0]", "omega_full[1][0]", "omega_full[1][1]"],
        &mut p,
    );
    p.finish("a fixed whole block, the rest (POP-EST-02)");
}

/// Fixing one element of a block is `later` (POP-EST-02): refused with `invalid_option` that names `fixed`.
#[test]
fn fixing_one_element_of_a_block_is_refused_with_the_option_named() {
    let case = fit_case("fit_q2_block");
    let mut req = case_request(&case, "generating_values");
    req["fixed"] = json!({ "omega.intercept": 4.0 });
    match population_fit(&req) {
        Err(e) if e.code == "invalid_option" && e.message.contains("fixed") => {}
        other => panic!(
            "POP-EST-02 and POP-INN-03 item 5: fixing one element of a block is `invalid_option` naming `fixed`, got {other:?}"
        ),
    }
}

// ---------------------------------------------------------------- item 4: statuses (POP-EST-03, -04)

/// The names of POP-EST-03 and POP-API-01 item 9.
const STATUSES: [&str; 8] = [
    "converged",
    "max_iterations",
    "no_decrease",
    "singular",
    "non_finite",
    "at_bound",
    "inner_failed",
    "evaluated",
];

fn has_zero(v: &Value) -> bool {
    match v {
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::Array(a) => a.iter().any(has_zero),
        Value::Object(o) => o.values().any(has_zero),
        _ => false,
    }
}

fn has_number(v: &Value) -> bool {
    match v {
        Value::Number(_) => true,
        Value::Array(a) => a.iter().any(has_number),
        Value::Object(o) => o.values().any(has_number),
        _ => false,
    }
}

/// What a fit that did not converge must show (POP-EST-03, POP-COV-03): a status of the list, the last trace row
/// carrying it, no standard error equal to 0 and the reason `not_converged` in every kind (the first bullet of
/// POP-COV-03; its parenthesis, "shown with the mark" of FIT-CNV-03, leaves open whether a number is also shown, so
/// the test does not ask for null). With `expected`, the status itself.
fn check_not_converged(answer: &Value, expected: Option<&str>, what: &str) {
    let status = answer["status"].as_str().unwrap_or("");
    let mut p = Problems::new();
    if !STATUSES.contains(&status) {
        p.say(format!(
            "status `{status}` is not one of POP-EST-03: {STATUSES:?}"
        ));
    }
    if let Some(e) = expected {
        if status != e {
            p.say(format!("status `{status}`, expected `{e}`"));
        }
    } else if ["converged", "at_bound", "evaluated"].contains(&status) {
        p.say(format!(
            "status `{status}`: a fit that cannot converge is reported as converged"
        ));
    }
    let last = answer["trace"].as_array().and_then(|t| t.last());
    if last.and_then(|r| r["status"].as_str()) != Some(status) {
        p.say(format!(
            "the last trace row carries the status {:?}, the answer says `{status}` (POP-EST-03: \"the last row carries the status\")",
            last.map(|r| &r["status"])
        ));
    }
    if has_zero(&answer["se"]) {
        p.say(format!(
            "`se` holds a 0 after `{status}`: {} (POP-COV-03: a standard error is NC, never 0)",
            answer["se"]
        ));
    }
    for kind in KINDS {
        let nc = &answer["covariance"][kind]["nc"];
        if !nc
            .as_array()
            .is_some_and(|a| a.iter().any(|r| r == "not_converged"))
        {
            p.say(format!(
                "covariance.{kind}.nc is {nc}, POP-COV-03 says `not_converged` after `{status}`"
            ));
        }
        if has_zero(&answer["se_by_kind"][kind]) {
            p.say(format!("`se_by_kind.{kind}` holds a 0 after `{status}`"));
        }
    }
    p.finish(&format!("{what}, status `{status}`"));
}

fn sealed_replicate_request(
    ids: Option<&[&str]>,
    two_observations: &[&str],
) -> (Value, Vec<String>) {
    let sealed = load_pop_sealed("accurate").expect("the sealed design loads");
    let subjects = load_pop_sealed_replicate(&sealed, 1).expect("replicate 1 loads");
    let mut list = Vec::new();
    let mut all = Vec::new();
    for s in &subjects {
        all.push(s.id.clone());
        let truncated = two_observations.contains(&s.id.as_str());
        if ids.is_some_and(|ids| !ids.contains(&s.id.as_str())) && !truncated {
            continue;
        }
        let n = if truncated { 2 } else { s.time.len() };
        list.push(json!({
            "id": s.id,
            "dose": f(&s.dose),
            "time": s.time.iter().take(n).map(|t| f(t)).collect::<Vec<_>>(),
            "y": s.y.iter().take(n).map(|t| f(t)).collect::<Vec<_>>(),
        }));
    }
    (
        json!({
            "model": sealed.model.name,
            "random_effects": ["cl", "v"],
            "omega_shape": "diagonal",
            "error_model": sealed.model.error_model,
            "options": { "method": "foce_i", "ofv_convention": "full" },
            "subjects": list,
        }),
        all,
    )
}

/// The request of a whole replicate with the user's start, in the vocabulary of the sealed test.
fn sealed_user_start() -> Value {
    let (mut req, _) = sealed_replicate_request(None, &[]);
    req["initial"] = json!({ "cl": 5.0, "v": 50.0, "ka": 1.5 });
    req["omega"] = json!([[0.1, 0.0], [0.0, 0.1]]);
    req["error_init"] = json!([0.01, 0.1]);
    req
}

/// POP-EST-03: a second-order condition that fails at the candidate (R not positive definite) and a return to the
/// same point is `singular`. Here theta2 cannot be identified: every time is 0, so the slope column and theta2 have
/// no effect on the objective and R is singular at every point.
#[test]
fn an_unidentifiable_slope_ends_with_the_status_singular() {
    let case = fit_case("fit_q2_diag");
    let mut req = case_request(&case, "generating_values");
    for s in req["subjects"].as_array_mut().unwrap() {
        let n = s["time"].as_array().unwrap().len();
        s["time"] = json!(vec![0.0; n]);
    }
    let answer = fit_ok(&req, "all times 0");
    check_not_converged(&answer, Some("singular"), "all times 0 (R singular)");
}

/// POP-EST-03: `inner_failed` when every trial point of the line search has an inner problem that fails. A start
/// at cl = 5e8 and v = 5e-8 (the request is accepted, the inner problems of the first iterations fail).
#[test]
fn a_start_far_outside_the_data_ends_with_the_status_inner_failed() {
    let mut req = sealed_user_start();
    req["initial"]["cl"] = json!(5e8);
    req["initial"]["v"] = json!(5e-8);
    let answer = fit_ok(&req, "cl 5e8, v 5e-8");
    check_not_converged(
        &answer,
        Some("inner_failed"),
        "a start far outside the data",
    );
}

/// `no_decrease` is a name of POP-EST-03 and FIT-ML-08, and the spec gives no trigger. Reached through the public
/// request with data that are all 0 (the objective has no minimum: sigma tends to 0). Only the consequences are
/// asserted, plus the status the engine gives, as a record of what is reachable (card T-188 item 4).
#[test]
fn data_of_all_zeros_do_not_end_as_converged() {
    let case = fit_case("fit_q2_diag");
    let mut req = case_request(&case, "generating_values");
    for s in req["subjects"].as_array_mut().unwrap() {
        let n = s["y"].as_array().unwrap().len();
        s["y"] = json!(vec![0.0; n]);
    }
    let answer = fit_ok(&req, "y all 0");
    check_not_converged(&answer, None, "y all 0");
}

/// `max_iterations` and `non_finite`: not reachable through the public request (`PopRequest` has no iteration
/// limit, and every non-finite start tried (y x 1e150, theta1 1e150, omega 1e300) is refused before
/// the first iteration with `inner_failed`, POP-INN-03 item 4). The test says so and asserts what is true: those
/// requests are refusals with a code and a sentence, never a panic and never a status.
#[test]
fn non_finite_requests_are_refused_before_any_iteration() {
    let case = fit_case("fit_q2_diag");
    let base = case_request(&case, "generating_values");
    let mut cases: Vec<(&str, Value)> = Vec::new();
    let mut big_y = base.clone();
    for s in big_y["subjects"].as_array_mut().unwrap() {
        let y: Vec<f64> = s["y"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() * 1e150)
            .collect();
        s["y"] = json!(y);
    }
    cases.push(("y x 1e150", big_y));
    let mut big_theta = base.clone();
    big_theta["initial"]["theta1"] = json!(1e150);
    cases.push(("theta1 1e150", big_theta));
    let mut big_omega = base.clone();
    big_omega["omega"] = json!([[1e300, 0.0], [0.0, 1e300]]);
    cases.push(("omega 1e300", big_omega));
    for (what, req) in cases {
        match population_fit(&req) {
            Err(e) if e.code == "inner_failed" => {
                assert!(!e.message.is_empty(), "{what}: the refusal has no sentence");
            }
            other => {
                if let Ok(a) = &other {
                    eprintln!("{what}: status {}", a["status"]);
                }
                panic!(
                    "{what}: POP-INN-03 item 4: the inner problem has no finite h_i at the starting values, code `inner_failed`; got {other:?}"
                );
            }
        }
    }
}

/// `a_request_without_evaluate_only_fits`: a request without `evaluate_only` is a fit and says so by its status (a
/// status of the fit, not `evaluated`), with a trace and iterations; with `evaluate_only` the status is `evaluated`
/// (POP-API-01 item 6), with no standard errors and no outer iteration.
#[test]
fn a_request_without_evaluate_only_fits_and_one_with_it_is_evaluated() {
    let case = fit_case("fit_q2_diag");
    let base = case_request(&case, "generating_values");
    let fitted = fit_ok(&base, "no evaluate_only");
    assert_eq!(
        fitted["status"].as_str(),
        Some("converged"),
        "a fit of an interior exact case from the generating values: status"
    );
    assert!(
        fitted["trace"].as_array().is_some_and(|t| t.len() >= 2),
        "a fit has a trace with iteration 0 and the last row (POP-EST-03)"
    );
    let mut ev = base.clone();
    ev["options"]["evaluate_only"] = json!(true);
    let evaluated = fit_ok(&ev, "evaluate_only");
    let mut p = Problems::new();
    if evaluated["status"].as_str() != Some("evaluated") {
        p.say(format!(
            "evaluate_only: status {}, expected `evaluated`",
            evaluated["status"]
        ));
    }
    if has_number(&evaluated["se"]) || has_number(&evaluated["se_by_kind"]) {
        p.say(
            "evaluate_only: the answer has standard errors (POP-API-01 item 6: no covariance)"
                .to_owned(),
        );
    }
    if evaluated["iterations"].as_u64().is_some_and(|i| i > 0) {
        p.say(format!(
            "evaluate_only: {} outer iterations (POP-API-01 item 6: no outer iteration)",
            evaluated["iterations"]
        ));
    }
    p.finish("evaluate_only (POP-API-01 item 6)");
}

// ---------------------------------------------------------------- item 3: the two-stage start (POP-EST-05)

/// A request with no `initial`, no `omega` and no `error_init` of the sealed replicate 1 (`pk1.oral_1`, combined
/// error): `ids` keeps only those subjects, `two_observations` adds subjects cut to two observations (an
/// individual fit of three parameters is refused for them, FIT-ML-14, so they are certainly left out of the means).
fn bare_request(
    ids: Option<&[&str]>,
    two_observations: &[&str],
    random_effects: &[&str],
    shape: &str,
) -> Value {
    let (mut req, _) = sealed_replicate_request(ids, two_observations);
    req["random_effects"] = json!(random_effects);
    req["omega_shape"] = json!(shape);
    req
}

fn subject_ids(req: &Value) -> Vec<String> {
    req["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect()
}

fn left_out(answer: &Value) -> Vec<String> {
    answer["start"]["left_out"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Sources a start may name (POP-EST-05: `user`, `two_stage`; a fixed parameter is not started; `error_init` is the
/// request key of a user value). Anything else, such as the engine's `moments`, is not in the spec.
const SOURCES: [&str; 4] = ["user", "two_stage", "fixed", "error_init"];

fn check_sources(answer: &Value, what: &str, p: &mut Problems) {
    match answer["start"]["sources"].as_object() {
        None => p.say(format!("{what}: the answer has no `start.sources`")),
        Some(map) => {
            for (k, v) in map {
                if !v.as_str().is_some_and(|s| SOURCES.contains(&s)) {
                    p.say(format!(
                        "{what}: start.sources[{k}] is {v}, not a source of POP-EST-05 ({SOURCES:?})"
                    ));
                }
            }
        }
    }
}

/// POP-EST-05: no start given anywhere. The fit runs, `start.kind` is `two_stage`, every parameter says
/// `two_stage`, the subjects that do not survive are listed (the truncated ones certainly), still take part in the
/// fit (every subject is in the answer) and are not more than the subjects.
#[test]
fn a_request_with_no_start_is_started_by_the_two_stage_method() {
    let req = bare_request(None, &[], &["cl", "ka", "v"], "diagonal");
    let ids = subject_ids(&req);
    let answer = fit_ok(&req, "no start");
    let mut p = Problems::new();
    if answer["start"]["kind"].as_str() != Some("two_stage") {
        p.say(format!(
            "start.kind is {}, expected `two_stage`",
            answer["start"]["kind"]
        ));
    }
    check_sources(&answer, "no start", &mut p);
    for k in [
        "cl",
        "ka",
        "v",
        "omega[cl,cl]",
        "omega[ka,ka]",
        "omega[v,v]",
        "xi.a",
        "xi.b",
    ] {
        if answer["start"]["sources"][k].as_str() != Some("two_stage") {
            p.say(format!(
                "start.sources[{k}] is {}, expected `two_stage`",
                answer["start"]["sources"][k]
            ));
        }
    }
    let out = left_out(&answer);
    if !out.iter().all(|o| ids.contains(o)) {
        p.say(format!(
            "left_out {out:?} names a subject that is not in the data"
        ));
    }
    if ids.len() - out.len().min(ids.len()) < 2 {
        p.say(format!(
            "{} of {} subjects left out: fewer than 2 survive",
            out.len(),
            ids.len()
        ));
    }
    let answered: Vec<&str> = answer["subjects"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s["id"].as_str()).collect())
        .unwrap_or_default();
    for o in &out {
        if !answered.contains(&o.as_str()) {
            p.say(format!(
                "subject {o} was left out of the means and must still take part in the fit (POP-EST-05 item 1), but it is not in the answer"
            ));
        }
    }
    p.finish("two-stage start, nothing given (POP-EST-05)");
}

/// Subjects cut to two observations cannot have an individual fit of three parameters: they are in `left_out`.
#[test]
fn subjects_with_two_observations_are_left_out_of_the_means() {
    let req = bare_request(
        Some(&["6", "18"]),
        &["1", "2", "3"],
        &["cl", "ka", "v"],
        "diagonal",
    );
    let answer = fit_ok(&req, "two survivors");
    let out = left_out(&answer);
    for must in ["1", "2", "3"] {
        assert!(
            out.iter().any(|o| o == must),
            "POP-EST-05 item 1: subject {must} has two observations, its individual fit is refused, it is left out; left_out = {out:?}"
        );
    }
    let mut p = Problems::new();
    check_sources(&answer, "two survivors", &mut p);
    if answer["start"].get("note").is_some_and(|n| !n.is_null()) {
        p.say(format!(
            "a diagonal Omega has no block note, the answer has {}",
            answer["start"]["note"]
        ));
    }
    p.finish("left_out (POP-EST-05 item 1)");
}

/// POP-EST-05 item 2: a parameter without a random effect has the natural-scale mean as start. `ka` has none here
/// and is not given: the spec asks for a start, not a refusal.
#[test]
fn a_parameter_without_a_random_effect_is_started_by_its_natural_scale_mean() {
    let req = bare_request(None, &[], &["cl", "v"], "diagonal");
    match population_fit(&req) {
        Ok(answer) => {
            assert_eq!(
                answer["start"]["sources"]["ka"].as_str(),
                Some("two_stage"),
                "POP-EST-05 item 2: `ka` is started by the two-stage mean"
            );
        }
        Err(e) => panic!(
            "POP-EST-05 item 2 (\"the natural-scale mean for a parameter without [a random effect]\"): the request is refused: {e}"
        ),
    }
}

/// Everything given: the start is the user's (kind `user`), nothing is left out.
#[test]
fn a_start_given_in_full_is_the_users() {
    let mut p = Problems::new();
    let mut full = bare_request(None, &[], &["cl", "v"], "diagonal");
    full["initial"] = json!({ "cl": 5.0, "v": 50.0, "ka": 1.5 });
    full["omega"] = json!([[0.1, 0.0], [0.0, 0.1]]);
    full["error_init"] = json!([0.01, 0.1]);
    let a = fit_ok(&full, "everything given");
    if a["start"]["kind"].as_str() != Some("user") {
        p.say(format!(
            "everything given: kind {}, expected `user`",
            a["start"]["kind"]
        ));
    }
    check_sources(&a, "everything given", &mut p);
    if !left_out(&a).is_empty() {
        p.say(format!(
            "everything given: left_out {:?}, expected none",
            left_out(&a)
        ));
    }
    p.finish("start given in full (POP-EST-05)");
}

/// Typical values given, `omega` and `error_init` not: kind `mixed`, theta `user`, Omega and xi `two_stage`.
#[test]
fn typical_values_given_and_nothing_else_make_a_mixed_start() {
    let mut p = Problems::new();
    let a = fit_ok(&theta_only_request(), "theta given");
    if a["start"]["kind"].as_str() != Some("mixed") {
        p.say(format!(
            "theta given: kind {}, expected `mixed`",
            a["start"]["kind"]
        ));
    }
    for (k, want) in [
        ("cl", "user"),
        ("v", "user"),
        ("ka", "user"),
        ("omega[cl,cl]", "two_stage"),
        ("omega[v,v]", "two_stage"),
        ("xi.a", "two_stage"),
        ("xi.b", "two_stage"),
    ] {
        if a["start"]["sources"][k].as_str() != Some(want) {
            p.say(format!(
                "theta given: start.sources[{k}] is {}, expected `{want}`",
                a["start"]["sources"][k]
            ));
        }
    }
    check_sources(&a, "theta given", &mut p);
    p.finish("start with theta given (POP-EST-05)");
}

fn theta_only_request() -> Value {
    let mut theta = bare_request(None, &[], &["cl", "v"], "diagonal");
    theta["initial"] = json!({ "cl": 5.0, "v": 50.0, "ka": 1.5 });
    theta
}

/// `initial` and `omega` given, `error_init` not: the error parameters are the two-stage means (POP-EST-05 item 2),
/// so the start is `mixed` and `xi.a`, `xi.b` say `two_stage`. Fails today: kind `user`, source `moments`.
#[test]
fn error_parameters_not_given_are_started_by_the_two_stage_method() {
    let mut p = Problems::new();
    let mut two = theta_only_request();
    two["omega"] = json!([[0.1, 0.0], [0.0, 0.1]]);
    let a = fit_ok(&two, "theta and omega given");
    if a["start"]["kind"].as_str() != Some("mixed") {
        p.say(format!(
            "theta and omega given: kind {}, expected `mixed` (the error parameters are started by the two-stage method)",
            a["start"]["kind"]
        ));
    }
    for k in ["xi.a", "xi.b"] {
        if a["start"]["sources"][k].as_str() != Some("two_stage") {
            p.say(format!(
                "theta and omega given: start.sources[{k}] is {}, expected `two_stage`",
                a["start"]["sources"][k]
            ));
        }
    }
    check_sources(&a, "theta and omega given", &mut p);
    p.finish("start with theta and omega given (POP-EST-05)");
}

/// The three sentences of POP-EST-05 item 3 say what to give: option form (POP-INN-03 item 5), the option named and
/// something to give.
fn check_start_refusal(req: &Value, words: &[&str], what: &str) {
    match population_fit(req) {
        Ok(a) => panic!(
            "{what}: POP-EST-05 item 3 says this start cannot be built and is an error; the fit ran with status {}",
            a["status"]
        ),
        Err(e) => {
            let mut p = Problems::new();
            if e.code != "invalid_option" {
                p.say(format!(
                    "code `{}`, expected `invalid_option` (POP-INN-03 item 5)",
                    e.code
                ));
            }
            if !e.message.starts_with("option `") {
                p.say(format!(
                    "the sentence does not start with option `name`: {}",
                    e.message
                ));
            }
            if !e.message.contains("give") {
                p.say(format!(
                    "the sentence does not say what to give: {}",
                    e.message
                ));
            }
            let lower = e.message.to_lowercase();
            for w in words {
                if !lower.contains(w) {
                    p.say(format!(
                        "the sentence does not mention `{w}`: {}",
                        e.message
                    ));
                }
            }
            p.finish(&format!("{what} (POP-EST-05 item 3)"));
        }
    }
}

/// Item 3, first sentence: fewer than 2 surviving subjects. Subject 6 survives and the others have two observations
/// (an individual fit of three parameters is refused for them), so exactly one subject survives.
#[test]
fn fewer_than_two_surviving_subjects_is_an_error_that_says_what_to_give() {
    let req = bare_request(
        Some(&["6"]),
        &["1", "2", "3"],
        &["cl", "ka", "v"],
        "diagonal",
    );
    check_start_refusal(&req, &["subject", "2"], "one surviving subject");
}

/// Item 3, first sentence again, with no survivor at all: every subject has two observations.
#[test]
fn no_surviving_subject_is_an_error_that_says_what_to_give() {
    let req = bare_request(
        Some(&[]),
        &["1", "2", "3", "4", "5", "6", "7", "8"],
        &["cl", "ka", "v"],
        "diagonal",
    );
    check_start_refusal(&req, &["subject", "2"], "no surviving subject");
}

/// Item 3, second sentence: a diagonal element of Omega0 equal to 0 because every subject returned the same value.
/// Six copies of one subject give the same individual fit.
#[test]
fn identical_subjects_give_a_zero_variance_and_an_error_that_says_what_to_give() {
    let mut req = bare_request(Some(&["6"]), &[], &["cl", "ka", "v"], "diagonal");
    let s6 = req["subjects"][0].clone();
    req["subjects"] = json!(
        (0..6)
            .map(|i| {
                let mut s = s6.clone();
                s["id"] = json!(format!("copy{i}"));
                s
            })
            .collect::<Vec<_>>()
    );
    check_start_refusal(&req, &["variance", "omega"], "identical subjects");
}

/// Item 3, third sentence, which is not an error: a block whose sample covariance is not positive definite uses the
/// diagonal for the start and says so in a note. Three surviving subjects for three random effects (rank 2).
#[test]
fn a_block_start_that_is_not_positive_definite_uses_the_diagonal_and_says_so() {
    let req = bare_request(
        Some(&["6", "18", "28"]),
        &["1", "2", "3"],
        &["cl", "ka", "v"],
        "block",
    );
    let answer = fit_ok(&req, "block, three survivors");
    let note = answer["start"]["note"].as_str().unwrap_or("");
    assert!(
        note.to_lowercase().contains("diagonal"),
        "POP-EST-05 item 3: a note says that the diagonal is used for the start; start.note is {:?}",
        answer["start"]["note"]
    );
    assert_eq!(
        answer["start"]["kind"].as_str(),
        Some("two_stage"),
        "the start is still two-stage"
    );
}
