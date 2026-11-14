//! The covariance of the estimates (POP-COV-01 to POP-COV-04) and the flags of POP-EST-04.
//!
//! On the free coordinates of φ that are not on a bound: R, the Hessian of the OFV by second
//! differences (the confirmation Hessian of the stop when it was taken there), and S = Σ_i g_i·g_iᵀ
//! from the per-subject central differences, summed in subject order. Then `sandwich` = R⁻¹·S·R⁻¹,
//! `r` = 2·R⁻¹ and `s` = 4·S⁻¹, each with its own NC reasons, carried to the user's scale by the
//! delta method (the Jacobian of [`Layout::user_jacobian`]).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::linalg::{Mat, symmetric_eigenvalues};

use super::outer::{Outcome, Problem, Status, spd_inverse};
use super::phi::{Layout, at};

/// The ratio ω̂_k/ω_k⁰ (standard deviations, the fitted value over the start used) below which the
/// flag `omega_collapse` is raised (POP-EST-04). OP-11, waits for Q-026. Measured (T-154): on the
/// exact fit cases whose variance is 0 at the maximum the ratio ends between 9.9e-6 and 4.0e-4; on
/// the other exact cases it is at least 4.0e-2 (`fit_q2_block_boundary` from the far start, a
/// rank-one Ω whose diagonal is not 0) and otherwise at least 0.29; on the sealed designs it is at
/// least 0.507 (`accurate`, 200 replicates) and, on `limits`, at least 0.271 except replicate 191,
/// where ω_cl collapses (ratio below 5e-4, η-shrinkage 1.0). 1e-2 lies between the two groups.
pub(crate) const OMEGA_COLLAPSE_RATIO: f64 = 1e-2;

/// The η-shrinkage above which the flag `high_eta_shrinkage` is raised (POP-DIA-03). OP-11, waits
/// for Q-026. The paper of POP-DIA-03 discusses ranges, not a threshold; 0.3 is the upper end of
/// the range it discusses (decision taken meanwhile, 2026-11-08, reversible by Abdou). Measured
/// (T-154): on `accurate` (40 subjects × 8 observations) the largest η-shrinkage over the 200
/// replicates is 0.170, so the flag never fires; on `limits` (30 × 3) the median of the 400 values
/// is 0.086, the 90th percentile 0.314, the largest 1.0, and the flag fires on 50 replicates.
pub(crate) const HIGH_ETA_SHRINKAGE: f64 = 0.3;

/// The number of accepted outer iterations above which the flag `slow_convergence` is raised
/// (POP-EST-04). OP-11, waits for Q-026. Decision taken meanwhile (2026-11-08), reversible by Abdou:
/// half of [`super::outer::K_MAX`]. Measured (T-154): no fit of the oracle takes more than 40
/// iterations (exact cases 40, `accurate` 19, `limits` 35), so the flag never fires there.
pub(crate) const SLOW_ITERATIONS: usize = 100;

/// One kind of covariance on the user's scale.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Kind {
    /// Over `labels`, row by row; null when NC.
    pub matrix: Option<Vec<Vec<f64>>>,
    /// The NC reasons of POP-COV-03 (empty when computed).
    pub nc: Vec<String>,
}

/// The covariance block of the answer (POP-EST-04).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CovarianceBlock {
    /// The kind printed (`options.covariance`, `sandwich` by default).
    pub printed: String,
    /// The estimates the matrices are over, in the user's units: the free typical values, the free
    /// elements of Ω, the free error parameters.
    pub labels: Vec<String>,
    pub sandwich: Kind,
    pub r: Kind,
    pub s: Kind,
    /// The correlation matrix of the printed kind, its eigenvalues (decreasing) and their ratio.
    pub correlation: Option<Vec<Vec<f64>>>,
    pub eigenvalues: Option<Vec<f64>>,
    pub condition_number: Option<f64>,
    /// RSE = 100·SE/|estimate| of the printed kind, by label; null at an estimate of 0 or NC.
    pub rse: BTreeMap<String, Option<f64>>,
    /// `at_bound`, `omega_collapse`, `high_eta_shrinkage`, `few_subjects`, `few_observations`,
    /// `slow_convergence`.
    pub flags: Vec<String>,
}

/// The standard errors of every user quantity, by kind, in the order of
/// [`Layout::user_names`]; `None` where NC, fixed, on a bound, or not a free quantity.
pub(crate) struct Errors {
    pub block: CovarianceBlock,
    /// By kind (`sandwich`, `r`, `s`).
    pub by_kind: BTreeMap<String, Vec<Option<f64>>>,
}

/// The inputs of the flags that do not come from the matrices.
pub(crate) struct Counts {
    /// m, the subjects used.
    pub m: usize,
    /// N, the observations used.
    pub n: usize,
    pub theta_free: usize,
    pub omega_free: usize,
    /// Q, the free error parameters.
    pub xi_free: usize,
    /// ω_k at the start (standard deviations).
    pub omega_start_sd: Vec<f64>,
    /// ω̂_k (standard deviations).
    pub omega_sd: Vec<f64>,
    pub eta_shrinkage: Vec<Option<f64>>,
}

fn reason(x: &str) -> Vec<String> {
    vec![x.to_string()]
}

/// A × B.
fn product(a: &Mat, b: &Mat) -> Mat {
    let mut c = Mat::zeros(a.rows(), b.cols());
    for i in 0..a.rows() {
        for j in 0..b.cols() {
            let s: f64 = (0..a.cols()).map(|k| a.at(i, k) * b.at(k, j)).sum();
            c.set(i, j, s);
        }
    }
    c
}

/// J·C·Jᵀ.
fn congruence(j: &Mat, c: &Mat) -> Mat {
    let jc = product(j, c);
    let mut out = Mat::zeros(j.rows(), j.rows());
    for a in 0..j.rows() {
        for b in 0..j.rows() {
            let s: f64 = (0..j.cols()).map(|k| jc.at(a, k) * j.at(b, k)).sum();
            out.set(a, b, s);
        }
    }
    out
}

fn scaled(a: &Mat, factor: f64) -> Mat {
    let mut out = a.clone();
    for i in 0..a.rows() {
        for j in 0..a.cols() {
            out.set(i, j, factor * a.at(i, j));
        }
    }
    out
}

fn rows_of(a: &Mat, keep: &[usize]) -> Vec<Vec<f64>> {
    keep.iter()
        .map(|&i| keep.iter().map(|&j| a.at(i, j)).collect())
        .collect()
}

/// The flags of POP-EST-04 that do not need the matrices.
fn flags(status: Status, iterations: usize, counts: &Counts, p: usize) -> Vec<String> {
    let mut out = Vec::new();
    if status == Status::AtBound {
        out.push("at_bound");
    }
    let collapsed = counts
        .omega_sd
        .iter()
        .zip(&counts.omega_start_sd)
        .any(|(w, w0)| *w < OMEGA_COLLAPSE_RATIO * w0);
    if matches!(status, Status::Converged | Status::AtBound) && collapsed {
        out.push("omega_collapse");
    }
    if counts
        .eta_shrinkage
        .iter()
        .flatten()
        .any(|s| *s > HIGH_ETA_SHRINKAGE)
    {
        out.push("high_eta_shrinkage");
    }
    // POP-EST-04 counts the elements of Ω and the free typical values; POP-COV-03 compares m with
    // p, the length of φ. Both are raised under the one name (decision taken meanwhile, 2026-11-08,
    // reversible by Abdou).
    if counts.m <= counts.theta_free + counts.omega_free || counts.m <= p {
        out.push("few_subjects");
    }
    if counts.n <= counts.theta_free + counts.xi_free + counts.omega_free {
        out.push("few_observations");
    }
    if iterations > SLOW_ITERATIONS {
        out.push("slow_convergence");
    }
    out.into_iter().map(str::to_string).collect()
}

/// The covariance block and the standard errors at the end of a fit.
pub(crate) fn covariance(
    problem: &Problem<'_>,
    outcome: &Outcome,
    printed: &str,
    counts: &Counts,
) -> Errors {
    let layout: &Layout = problem.layout;
    let p_all = layout.len();
    let params = layout.params(&outcome.phi);
    let n_user = layout.user_values(&params).len();
    let names = layout.user_names(
        &problem.setup.random_effects,
        super::error_keys(problem.setup.error_model),
    );
    let flags = flags(outcome.status, outcome.iterations, counts, p_all);
    let empty = |why: &str| Errors {
        block: CovarianceBlock {
            printed: printed.to_string(),
            labels: Vec::new(),
            sandwich: Kind {
                matrix: None,
                nc: reason(why),
            },
            r: Kind {
                matrix: None,
                nc: reason(why),
            },
            s: Kind {
                matrix: None,
                nc: reason(why),
            },
            correlation: None,
            eigenvalues: None,
            condition_number: None,
            rse: BTreeMap::new(),
            flags: flags.clone(),
        },
        by_kind: ["sandwich", "r", "s"]
            .iter()
            .map(|k| ((*k).to_string(), vec![None; n_user]))
            .collect(),
    };
    if !matches!(outcome.status, Status::Converged | Status::AtBound) {
        return empty("not_converged");
    }
    // The free coordinates not on a bound (FIT-BND-02: the others are conditioned on).
    let free: Vec<usize> = (0..p_all)
        .filter(|&j| !layout.on_bound(&outcome.phi, j))
        .collect();
    let pf = free.len();
    let Some(gradient) = &outcome.gradient else {
        return empty("non_finite");
    };
    let r = match &outcome.hessian {
        Some((set, r)) if *set == free => Ok(r.clone()),
        _ => problem.hessian(
            &outcome.phi,
            &free,
            &outcome.evaluation.warm(problem.setup.random_effects.len()),
        ),
    };
    let Ok(r) = r else {
        return empty("non_finite");
    };
    // S over the free set, in subject order (POP-DET-01).
    let mut s = Mat::zeros(pf, pf);
    for g in &gradient.per_subject {
        for (a, &ja) in free.iter().enumerate() {
            for (b, &jb) in free.iter().enumerate() {
                s.set(a, b, s.at(a, b) + at(g, ja) * at(g, jb));
            }
        }
    }
    let r_inv = spd_inverse(&r);
    let s_inv = spd_inverse(&s);
    let m = counts.m;
    let phi_kinds: [(&str, Result<Mat, &str>); 3] = [
        (
            "sandwich",
            r_inv
                .as_ref()
                .map(|ri| product(&product(ri, &s), ri))
                .ok_or("singular"),
        ),
        (
            "r",
            r_inv.as_ref().map(|ri| scaled(ri, 2.0)).ok_or("singular"),
        ),
        (
            "s",
            if m <= pf {
                Err("few_subjects")
            } else {
                s_inv.as_ref().map(|si| scaled(si, 4.0)).ok_or("singular")
            },
        ),
    ];
    // The Jacobian of the user quantities over the free set.
    let full = layout.user_jacobian(&params);
    let mut jac = Mat::zeros(n_user, pf);
    for i in 0..n_user {
        for (a, &j) in free.iter().enumerate() {
            jac.set(i, a, full.at(i, j));
        }
    }
    // The user quantities that move with the free set: a row of zeros is a fixed quantity, one on
    // a bound, or one held at 0 (decision taken meanwhile, 2026-11-08, reversible by Abdou).
    let moving: Vec<usize> = (0..n_user)
        .filter(|&i| (0..pf).any(|a| jac.at(i, a) != 0.0))
        .collect();
    let labels: Vec<String> = moving
        .iter()
        .map(|&i| names.get(i).cloned().unwrap_or_default())
        .collect();
    let mut by_kind = BTreeMap::new();
    let mut kinds: BTreeMap<String, Kind> = BTreeMap::new();
    let mut printed_cov: Option<Mat> = None;
    for (name, cov) in phi_kinds {
        let (kind, se) = match cov {
            Ok(c) => {
                let user = congruence(&jac, &c);
                let se: Vec<Option<f64>> = (0..n_user)
                    .map(|i| {
                        let v = user.at(i, i);
                        (moving.contains(&i) && v.is_finite() && v >= 0.0).then(|| v.sqrt())
                    })
                    .collect();
                if name == printed {
                    printed_cov = Some(user.clone());
                }
                (
                    Kind {
                        matrix: Some(rows_of(&user, &moving)),
                        nc: Vec::new(),
                    },
                    se,
                )
            }
            Err(why) => (
                Kind {
                    matrix: None,
                    nc: reason(why),
                },
                vec![None; n_user],
            ),
        };
        kinds.insert(name.to_string(), kind);
        by_kind.insert(name.to_string(), se);
    }
    let mut take = |k: &str| {
        kinds.remove(k).unwrap_or(Kind {
            matrix: None,
            nc: reason("not_converged"),
        })
    };
    let (sandwich, r_kind, s_kind) = (take("sandwich"), take("r"), take("s"));
    let values = layout.user_values(&params);
    let printed_se = by_kind.get(printed).cloned().unwrap_or_default();
    let rse: BTreeMap<String, Option<f64>> = moving
        .iter()
        .map(|&i| {
            let est = at(&values, i);
            let se = printed_se.get(i).copied().flatten();
            let r = se.filter(|_| est != 0.0).map(|s| 100.0 * s / est.abs());
            (names.get(i).cloned().unwrap_or_default(), r)
        })
        .collect();
    let (correlation, eigenvalues, condition_number) = match &printed_cov {
        Some(c) => {
            let k = moving.len();
            let mut corr = Mat::zeros(k, k);
            for (a, &i) in moving.iter().enumerate() {
                for (b, &j) in moving.iter().enumerate() {
                    corr.set(a, b, c.at(i, j) / (c.at(i, i) * c.at(j, j)).sqrt());
                }
            }
            let e = symmetric_eigenvalues(&corr);
            let cond = match (e.first(), e.last()) {
                (Some(max), Some(min)) if *min > 0.0 => Some(max / min),
                _ => None,
            };
            let all: Vec<usize> = (0..k).collect();
            (Some(rows_of(&corr, &all)), Some(e), cond)
        }
        None => (None, None, None),
    };
    Errors {
        block: CovarianceBlock {
            printed: printed.to_string(),
            labels,
            sandwich,
            r: r_kind,
            s: s_kind,
            correlation,
            eigenvalues,
            condition_number,
            rse,
            flags,
        },
        by_kind,
    }
}
