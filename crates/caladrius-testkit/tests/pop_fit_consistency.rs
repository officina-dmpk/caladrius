#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: helpers panic on purpose
#![allow(clippy::disallowed_methods)] // independent reference arithmetic, with the platform's maths library

//! Cross-checks the exact fit-level oracle (task T-168, `oracle/expected/pop/fit_exact/`) against independent
//! arithmetic, so that the expected files are tested by something other than the R script that wrote them
//! (`AGENTS.md` section 5, source 3):
//!
//! - the files load, are complete, balanced, and carry the cases the card asks for;
//! - the data of H38 are the 30 values of the table of `specs/sources.md`;
//! - the closed forms (q = 1, interior and on the boundary; q = 2 block), written again here in double precision
//!   with 2 x 2 algebra, reproduce the stored estimates and the objective;
//! - `naive_pop` (the double-precision objective of the testkit, written without the R scripts) evaluates the
//!   stored objective at the stored estimate of every interior case;
//! - the stored point is a local minimum, the stored diagonal of `R^-1` (the basis of the tolerances of the engine
//!   tests) matches a finite-difference Hessian of the same objective, and the standard errors of the kind `r` are
//!   the square roots of `2 R^-1`;
//! - on a boundary, the objective rises at the stored rate when the variance leaves 0 (the sign conditions);
//! - the three references of the R script (the exact point, `nlme`, the direct maximisation) agree within limits
//!   chosen from the measured disagreements.
//!
//! This tests the oracle and the testkit, not the engine. The measurements are printed
//! (`cargo test -p caladrius-testkit --test pop_fit_consistency -- --nocapture`).

use caladrius_testkit::{
    AbsoluteTolerance, NaiveFrozen, PopFitCase, PopModel, list_pop_fit_cases, load_pop_fit_case,
    naive_pop, pop_dir,
};

/// The cases the card asks for: q = 1 (an interior and a boundary maximum), q = 2 with a diagonal Omega and with a
/// block (an interior and a boundary maximum each).
const REQUIRED: [&str; 6] = [
    "fit_q1_h38",
    "fit_q1_boundary",
    "fit_q2_block",
    "fit_q2_block_boundary",
    "fit_q2_diag",
    "fit_q2_diag_boundary",
];

fn parse(text: &str) -> f64 {
    text.trim().parse::<f64>().unwrap()
}

fn cases() -> Vec<PopFitCase> {
    list_pop_fit_cases()
        .unwrap()
        .iter()
        .map(|id| load_pop_fit_case(id).unwrap())
        .collect()
}

/// A point of the parameter space in doubles.
#[derive(Debug, Clone)]
struct Point {
    theta: [f64; 2],
    omega: Vec<Vec<f64>>,
    sigma: f64,
}

fn exact_point(c: &PopFitCase) -> Point {
    Point {
        theta: [parse(&c.expected.theta1), parse(&c.expected.theta2)],
        omega: c
            .expected
            .omega
            .iter()
            .map(|r| r.iter().map(|t| parse(t)).collect())
            .collect(),
        sigma: parse(&c.expected.sigma),
    }
}

fn get(p: &Point, label: &str) -> f64 {
    match label {
        "theta1" => p.theta[0],
        "theta2" => p.theta[1],
        "xi.a" => p.sigma,
        other => {
            let (i, j) = indices(other);
            p.omega[i][j]
        }
    }
}

fn set(p: &mut Point, label: &str, value: f64) {
    match label {
        "theta1" => p.theta[0] = value,
        "theta2" => p.theta[1] = value,
        "xi.a" => p.sigma = value,
        other => {
            let (i, j) = indices(other);
            p.omega[i][j] = value;
            p.omega[j][i] = value;
        }
    }
}

fn indices(label: &str) -> (usize, usize) {
    let v: Vec<usize> = label
        .trim_start_matches("omega_full")
        .split(['[', ']'])
        .filter(|t| !t.is_empty())
        .map(|t| t.parse().unwrap())
        .collect();
    (v[0], v[1])
}

fn model_at(c: &PopFitCase, p: &Point) -> PopModel {
    let mut m = c.model.clone();
    m.theta
        .insert("theta1".to_owned(), format!("{:e}", p.theta[0]));
    m.theta
        .insert("theta2".to_owned(), format!("{:e}", p.theta[1]));
    m.omega = p
        .omega
        .iter()
        .map(|r| r.iter().map(|v| format!("{v:e}")).collect())
        .collect();
    m.xi.insert("a".to_owned(), format!("{:e}", p.sigma));
    m
}

/// The objective of the testkit (double precision, full convention) at a point; for `identity_linear` with an
/// additive error the `linearised` form is the exact marginal.
fn ofv_at(c: &PopFitCase, p: &Point) -> f64 {
    let r = naive_pop(&model_at(c, p), &c.subjects, NaiveFrozen::None, false)
        .unwrap_or_else(|e| panic!("{}: {e}", c.id));
    r.totals["linearised"]
}

/// Solves `A x = b` (Gauss-Jordan with partial pivoting) and returns the inverse of `A`.
fn invert(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row = a[i].clone();
            row.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            row
        })
        .collect();
    for col in 0..n {
        let piv = (col..n)
            .max_by(|&x, &y| m[x][col].abs().total_cmp(&m[y][col].abs()))
            .unwrap();
        m.swap(col, piv);
        let d = m[col][col];
        assert!(d.abs() > 1e-300, "singular matrix");
        for v in &mut m[col] {
            *v /= d;
        }
        for r in 0..n {
            if r != col {
                let f = m[r][col];
                if f != 0.0 {
                    #[allow(clippy::needless_range_loop)]
                    for k in 0..2 * n {
                        let t = m[col][k];
                        m[r][k] -= f * t;
                    }
                }
            }
        }
    }
    m.into_iter().map(|r| r[n..].to_vec()).collect()
}

// ---------------------------------------------------------------- the files

#[test]
fn the_fit_cases_load_and_have_the_stated_shape() {
    let all = cases();
    let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
    for want in REQUIRED {
        assert!(ids.contains(&want), "the card asks for `{want}`: {ids:?}");
    }
    for c in &all {
        let d = &c.design;
        assert_eq!(c.family, "d_fit_exact", "{}", c.id);
        assert_eq!(c.precision_bits, 256, "{}", c.id);
        assert_eq!(c.subjects.len(), d.m, "{}", c.id);
        assert_eq!(d.n_observations, d.m * d.n_per_subject, "{}", c.id);
        assert_eq!(d.times.len(), d.n_per_subject, "{}", c.id);
        for s in &c.subjects {
            // balanced: every subject has the times of the design
            assert_eq!(s.time, d.times, "{} {}: balanced design", c.id, s.id);
            assert_eq!(s.y.len(), d.n_per_subject, "{} {}", c.id, s.id);
        }
        assert_eq!(c.model.name, "identity_linear", "{}", c.id);
        assert_eq!(c.model.error_model, "additive", "{}", c.id);
        let q = d.z_powers.len();
        assert_eq!(c.expected.omega.len(), q, "{}", c.id);
        assert_eq!(
            c.expected.kind == "boundary",
            !c.expected.at_bound.is_empty(),
            "{}",
            c.id
        );
        assert!(c.starts.len() >= 2, "{}: two starts", c.id);
        assert_eq!(c.starts[0].name, "generating_values", "{}", c.id);
        assert_eq!(c.starts[1].name, "far", "{}", c.id);
        // every label of the answer has an exact value; the basis is a subset of the labels
        let labels = c.labels();
        for l in &labels {
            c.expected_value(l).unwrap();
        }
        for l in &c.tolerance_basis.parameters {
            assert!(labels.contains(l), "{}: basis label `{l}`", c.id);
        }
        assert_eq!(
            c.tolerance_basis.parameters.len(),
            c.tolerance_basis.rinv_diag.len(),
            "{}",
            c.id
        );
        assert!(
            c.tolerance_basis.rinv_diag.iter().all(|v| *v > 0.0),
            "{}",
            c.id
        );
        // the conventions differ by N ln 2 pi, in the file and in doubles
        let n_ln = d.n_observations as f64 * (2.0 * std::f64::consts::PI).ln();
        assert!(
            (parse(&c.expected.n_ln_2pi) - n_ln).abs() < 1e-10,
            "{}: N ln 2pi",
            c.id
        );
        assert!(
            (parse(&c.expected.ofv_full) - parse(&c.expected.ofv_no_2pi) - n_ln).abs() < 1e-10,
            "{}: conventions",
            c.id
        );
        // the shape of Omega: a variance on its bound is 0 and the others are positive
        let om = &c.expected.omega;
        #[allow(clippy::needless_range_loop)]
        for k in 0..q {
            let v = parse(&om[k][k]);
            let on_bound = c
                .expected
                .at_bound
                .contains(&format!("omega_full[{k}][{k}]"));
            if on_bound {
                assert_eq!(v, 0.0, "{} omega[{k}][{k}]", c.id);
            } else {
                assert!(v >= 0.0, "{} omega[{k}][{k}]", c.id);
            }
        }
        if d.shape == "diag" {
            assert_eq!(parse(&om[1][0]), 0.0, "{}", c.id);
            assert_eq!(d.omega_shape, "diagonal", "{}", c.id);
        }
        if d.shape == "block" {
            assert_eq!(d.omega_shape, "block", "{}", c.id);
        }
    }
    // the kinds the card asks for are all there
    let kinds = |shape: &str, kind: &str| {
        all.iter()
            .any(|c| c.design.shape == shape && c.expected.kind == kind)
    };
    for (shape, kind) in [
        ("q1", "interior"),
        ("q1", "boundary"),
        ("diag", "interior"),
        ("diag", "boundary"),
        ("block", "interior"),
        ("block", "boundary"),
    ] {
        assert!(kinds(shape, kind), "no {kind} case of shape {shape}");
    }
}

/// The 30 values of the table "The data of H38" of `specs/sources.md` are the data of `fit_q1_h38`.
#[test]
fn h38_is_the_table_of_the_spec() {
    let root = pop_dir().ancestors().nth(3).unwrap().to_path_buf();
    let text = std::fs::read_to_string(root.join("specs").join("sources.md")).unwrap();
    let at = text.find("The data of H38").expect("the table of H38");
    let rows: Vec<Vec<String>> = text[at..]
        .lines()
        .filter(|l| {
            let t = l.trim_start_matches('|').trim_start();
            t.starts_with('S') && t.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
        })
        .take(6)
        .map(|l| {
            l.split('|')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .skip(1)
                .map(str::to_owned)
                .collect()
        })
        .collect();
    assert_eq!(rows.len(), 6);
    let c = load_pop_fit_case("fit_q1_h38").unwrap();
    assert_eq!(c.subjects.len(), 6);
    assert_eq!(c.design.times, ["0.5", "1", "2", "4", "8"]);
    for (s, row) in c.subjects.iter().zip(&rows) {
        assert_eq!(&s.y, row, "{}", s.id);
    }
}

// ---------------------------------------------------------------- the closed forms, written again

struct Closed {
    point: Point,
    ofv_full: f64,
    boundary: bool,
}

/// The closed form of the balanced linear case with the random-effect design `Z = [t^p]` (q = 1 or 2), in doubles.
/// The mean part is pooled least squares (balanced, Z in the span of [1, t]: GLS = OLS); `b_i` is the projection of
/// the residual of subject i on Z, `S` the mean of `b_i b_i^T`, `W` the residual sum of squares after the
/// projection; `sigma^2 = W / (m (n - q))`, `Omega = S - sigma^2 (Z^T Z)^-1`, and
/// `OFV = N ln 2pi + m ln|Z^T Z| + m ln|S| + m q + m (n - q) ln sigma^2 + m (n - q)`.
/// A q = 1 case whose `Omega` would be negative has `Omega = 0`, `sigma^2 = (W + B) / N`.
fn closed_form(c: &PopFitCase) -> Closed {
    let m = c.subjects.len();
    let n = c.design.n_per_subject;
    let nn = (m * n) as f64;
    let t: Vec<f64> = c.subjects[0].time.iter().map(|x| parse(x)).collect();
    let y: Vec<Vec<f64>> = c
        .subjects
        .iter()
        .map(|s| s.y.iter().map(|x| parse(x)).collect())
        .collect();
    // pooled least squares of y on [1, t]
    let (mut st, mut stt, mut sy, mut sty) = (0.0, 0.0, 0.0, 0.0);
    for yi in &y {
        for (tj, yj) in t.iter().zip(yi) {
            st += tj;
            stt += tj * tj;
            sy += yj;
            sty += tj * yj;
        }
    }
    let det = nn * stt - st * st;
    let b1 = (stt * sy - st * sty) / det;
    let b2 = (nn * sty - st * sy) / det;
    let z: Vec<Vec<f64>> = c
        .design
        .z_powers
        .iter()
        .map(|&pw| t.iter().map(|tj| tj.powi(pw as i32)).collect())
        .collect();
    let q = z.len();
    let mut g = vec![vec![0.0; q]; q];
    for a in 0..q {
        for b in 0..q {
            g[a][b] = (0..n).map(|j| z[a][j] * z[b][j]).sum();
        }
    }
    let ginv = invert(&g);
    let mut s = vec![vec![0.0; q]; q];
    let mut w = 0.0;
    let mut rss = 0.0;
    for yi in &y {
        let r: Vec<f64> = (0..n).map(|j| yi[j] - b1 - b2 * t[j]).collect();
        rss += r.iter().map(|v| v * v).sum::<f64>();
        let zr: Vec<f64> = (0..q)
            .map(|a| (0..n).map(|j| z[a][j] * r[j]).sum())
            .collect();
        let b: Vec<f64> = (0..q)
            .map(|a| (0..q).map(|k| ginv[a][k] * zr[k]).sum())
            .collect();
        for a in 0..q {
            for k in 0..q {
                s[a][k] += b[a] * b[k] / m as f64;
            }
        }
        w += (0..n)
            .map(|j| {
                let fit: f64 = (0..q).map(|a| z[a][j] * b[a]).sum();
                (r[j] - fit) * (r[j] - fit)
            })
            .sum::<f64>();
    }
    let two_pi_n = nn * (2.0 * std::f64::consts::PI).ln();
    let mf = m as f64;
    let s2 = w / (mf * (n - q) as f64);
    let omega: Vec<Vec<f64>> = (0..q)
        .map(|a| (0..q).map(|k| s[a][k] - s2 * ginv[a][k]).collect())
        .collect();
    if q == 1 && omega[0][0] <= 0.0 {
        let s2 = rss / nn;
        return Closed {
            point: Point {
                theta: [b1, b2],
                omega: vec![vec![0.0]],
                sigma: s2.sqrt(),
            },
            ofv_full: two_pi_n + nn * s2.ln() + nn,
            boundary: true,
        };
    }
    let det_of = |a: &[Vec<f64>]| {
        if a.len() == 1 {
            a[0][0]
        } else {
            a[0][0] * a[1][1] - a[0][1] * a[1][0]
        }
    };
    let ofv = two_pi_n
        + mf * det_of(&g).ln()
        + mf * det_of(&s).ln()
        + mf * q as f64
        + mf * (n - q) as f64 * s2.ln()
        + mf * (n - q) as f64;
    Closed {
        point: Point {
            theta: [b1, b2],
            omega,
            sigma: s2.sqrt(),
        },
        ofv_full: ofv,
        boundary: false,
    }
}

#[test]
fn the_closed_forms_in_double_precision_reproduce_the_stored_estimates() {
    let mut worst: f64 = 0.0;
    let mut tested = Vec::new();
    for c in cases() {
        if c.expected.method != "closed_form" {
            continue;
        }
        let cf = closed_form(&c);
        assert_eq!(
            cf.boundary,
            c.expected.kind == "boundary",
            "{}: boundary or interior",
            c.id
        );
        let exact = exact_point(&c);
        for label in c.labels() {
            let (a, e) = (get(&cf.point, &label), get(&exact, &label));
            let d = (a - e).abs() / e.abs().max(1.0);
            worst = worst.max(d);
            assert!(
                d < 1e-9,
                "{} {label}: closed form {a:e}, stored {e:e}",
                c.id
            );
        }
        let d = (cf.ofv_full - parse(&c.expected.ofv_full)).abs();
        worst = worst.max(d);
        assert!(
            d < 1e-8,
            "{}: objective {:e} against stored {}",
            c.id,
            cf.ofv_full,
            c.expected.ofv_full
        );
        tested.push(c.id.clone());
    }
    eprintln!(
        "closed forms in doubles against the stored values: largest difference {worst:e} in {tested:?}"
    );
    assert!(
        tested.len() >= 3,
        "q = 1 interior, q = 1 boundary, q = 2 block: {tested:?}"
    );
}

// ---------------------------------------------------------------- the objective, the minimum, the curvature

#[test]
fn the_naive_objective_at_an_interior_estimate_is_the_stored_objective() {
    let mut worst: f64 = 0.0;
    let mut n = 0;
    for c in cases() {
        if c.expected.kind != "interior" {
            continue;
        }
        let d = (ofv_at(&c, &exact_point(&c)) - parse(&c.expected.ofv_full)).abs();
        worst = worst.max(d);
        n += 1;
        assert!(
            d <= AbsoluteTolerance::NAIVE_POP_OFV.abs,
            "{}: naive objective differs by {d:e}",
            c.id
        );
    }
    eprintln!(
        "naive_pop at the stored estimates of {n} interior cases: largest difference {worst:e}"
    );
    assert!(n >= 3);
}

/// A point of a case with the variances held on their bound moved to `delta` (a rank-one Omega gets `delta` on the
/// diagonal), so that the matrix is positive definite for the objective of the testkit.
fn lifted(c: &PopFitCase, delta: f64) -> Point {
    let mut p = exact_point(c);
    if c.expected
        .at_bound
        .iter()
        .any(|b| b == "omega_full (rank one)")
    {
        for k in 0..p.omega.len() {
            p.omega[k][k] += delta;
        }
    } else {
        for b in &c.expected.at_bound {
            set(&mut p, b, delta);
        }
    }
    p
}

/// The stored point is a local minimum in every free direction, and the stored diagonal of `R^-1` is that of the
/// Hessian of the objective by finite differences. The increase of the objective for a step `h` along a parameter
/// is `h^2 R_jj / 2 >= h^2 / (2 (R^-1)_jj)`, which the check uses. On a boundary the other parameters are free and
/// the bound is held at `1e-7` (`1e-5` for a rank-one Omega; the objective moves by `1e-7` times the stored gradient, which does not change the curvature of the others at the digits compared).
#[test]
fn the_stored_point_is_a_local_minimum_and_the_curvature_basis_matches_finite_differences() {
    let mut worst_rel: f64 = 0.0;
    for c in cases() {
        // the mode of the testkit does not converge for an Omega this close to singular and a lift this small
        // when Omega has rank one, so the lift is larger there
        let rank_one = c
            .expected
            .at_bound
            .iter()
            .any(|b| b == "omega_full (rank one)");
        let lift = if rank_one { 1e-5 } else { 1e-7 };
        let (labels, rinv): (Vec<String>, Vec<f64>) = if !c.expected.at_bound.is_empty() {
            // A parameter is held on its bound: it cannot be stepped (below the bound Omega is not positive
            // definite). The block of the other parameters, conditional on it, is compared with the stored standard
            // errors of the kind `r` (the free block of POP-COV-03); the curvature of the full parameter vector
            // can only be no smaller than that of the block (a Schur complement), which is checked.
            let se = &c.standard_errors;
            let names: Vec<String> = se["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            let block: Vec<f64> = (0..names.len())
                .map(|i| se["r"][i].as_f64().unwrap().powi(2) / 2.0)
                .collect();
            for (name, b) in names.iter().zip(&block) {
                let j = c
                    .tolerance_basis
                    .parameters
                    .iter()
                    .position(|l| l == name)
                    .unwrap();
                assert!(
                    c.tolerance_basis.rinv_diag[j] >= b * (1.0 - 1e-9),
                    "{} {name}: the curvature of the full vector is below that of the block",
                    c.id
                );
            }
            // The mode of the testkit does not converge for every subject at the points shifted around a singular
            // Omega (even for a step of theta or xi), so the finite differences are not taken here; the minimum of a
            // rank-one boundary is checked by the sign conditions in
            // `the_sign_conditions_of_the_boundaries_are_the_slopes_of_the_objective` and the curvature by the
            // comparison with the standard errors above.
            if rank_one {
                continue;
            }
            (names, block)
        } else {
            (
                c.tolerance_basis.parameters.clone(),
                c.tolerance_basis.rinv_diag.clone(),
            )
        };
        let p0 = lifted(&c, lift);
        let f0 = ofv_at(&c, &p0);
        // the objective moves by `lift` times the stored rate (at most 100 here)
        assert!(
            (f0 - parse(&c.expected.ofv_full)).abs() < 1e-7 + 100.0 * lift,
            "{}: the lifted point is the stored objective",
            c.id
        );
        let h: Vec<f64> = rinv.iter().map(|v| 0.005 * v.sqrt()).collect();
        let k = labels.len();
        // the objective at the shifted points
        let shifted = |steps: &[(usize, f64)]| {
            let mut p = p0.clone();
            for &(j, s) in steps {
                let v = get(&p, &labels[j]) + s;
                set(&mut p, &labels[j], v);
            }
            ofv_at(&c, &p)
        };
        let mut hess = vec![vec![0.0; k]; k];
        for a in 0..k {
            let (fp, fm) = (shifted(&[(a, h[a])]), shifted(&[(a, -h[a])]));
            hess[a][a] = (fp - 2.0 * f0 + fm) / (h[a] * h[a]);
            let floor = 0.9 * h[a] * h[a] / (2.0 * rinv[a]);
            assert!(
                fp - f0 >= floor && fm - f0 >= floor,
                "{} {}: not a minimum ({:e}, {:e} against {floor:e})",
                c.id,
                labels[a],
                fp - f0,
                fm - f0
            );
            for b in 0..a {
                let fpp = shifted(&[(a, h[a]), (b, h[b])]);
                let fpm = shifted(&[(a, h[a]), (b, -h[b])]);
                let fmp = shifted(&[(a, -h[a]), (b, h[b])]);
                let fmm = shifted(&[(a, -h[a]), (b, -h[b])]);
                let v = (fpp - fpm - fmp + fmm) / (4.0 * h[a] * h[b]);
                hess[a][b] = v;
                hess[b][a] = v;
            }
        }
        let inv = invert(&hess);
        for a in 0..k {
            let rel = (inv[a][a] - rinv[a]).abs() / rinv[a];
            worst_rel = worst_rel.max(rel);
            assert!(
                rel < 2e-3,
                "{} {}: (R^-1) by finite differences {:e}, stored {:e}",
                c.id,
                labels[a],
                inv[a][a],
                rinv[a]
            );
        }
    }
    eprintln!(
        "(R^-1) diagonal, stored against finite differences: largest relative difference {worst_rel:e}"
    );
}

/// The standard errors of the kind `r` are the square roots of `2 R^-1`, on the parameters that are free.
#[test]
fn the_r_standard_errors_are_the_roots_of_twice_the_inverse_curvature() {
    for c in cases() {
        let se = &c.standard_errors;
        let names: Vec<&str> = se["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for kind in ["r", "s", "sandwich"] {
            if let Some(a) = se[kind].as_array() {
                assert_eq!(a.len(), names.len(), "{} {kind}", c.id);
                assert!(
                    a.iter().all(|v| v.as_f64().is_some_and(|x| x > 0.0)),
                    "{} {kind}",
                    c.id
                );
            }
        }
        // the free block is the basis when the basis is `free_block`; with `all_parameters` the free ones are a subset
        for (i, name) in names.iter().enumerate() {
            if let Some(j) = c.tolerance_basis.parameters.iter().position(|l| l == name) {
                if c.tolerance_basis.basis == "free_block" {
                    let want = (2.0 * c.tolerance_basis.rinv_diag[j]).sqrt();
                    let got = se["r"][i].as_f64().unwrap();
                    assert!(
                        (got - want).abs() <= 1e-12 * want,
                        "{} {name}: r {got:e} against sqrt(2 R^-1) {want:e}",
                        c.id
                    );
                }
            }
        }
        if c.tolerance_basis.basis == "all_parameters" && c.expected.kind == "interior" {
            for (i, name) in names.iter().enumerate() {
                let j = c
                    .tolerance_basis
                    .parameters
                    .iter()
                    .position(|l| l == name)
                    .unwrap();
                let want = (2.0 * c.tolerance_basis.rinv_diag[j]).sqrt();
                let got = se["r"][i].as_f64().unwrap();
                assert!(
                    (got - want).abs() <= 1e-12 * want,
                    "{} {name}: r {got:e} against sqrt(2 R^-1) {want:e}",
                    c.id
                );
            }
        }
    }
}

// ---------------------------------------------------------------- the boundaries

/// On a boundary the objective rises at the stored rate when the variance leaves 0: for a variance on its bound by
/// the stored gradient; for the rank-one block by the stored trace of `dOFV/dOmega`, which is the rate along
/// `Omega + delta I`. The stored `G l` (the gradient along the rank-one direction) is 0.
#[test]
fn the_sign_conditions_of_the_boundaries_are_the_slopes_of_the_objective() {
    let mut n = 0;
    for c in cases() {
        if c.expected.kind != "boundary" {
            continue;
        }
        let delta = 1e-5;
        let f0 = ofv_at(&c, &lifted(&c, 1e-9));
        let slope = (ofv_at(&c, &lifted(&c, delta)) - f0) / (delta - 1e-9);
        let rank_one = c
            .expected
            .at_bound
            .iter()
            .any(|b| b == "omega_full (rank one)");
        let stored = if rank_one {
            let tr = c
                .expected
                .sign_conditions
                .iter()
                .find(|s| {
                    s["parameter"]
                        .as_str()
                        .is_some_and(|p| p.starts_with("other eigenvalue"))
                })
                .unwrap()["gradient"]
                .as_f64()
                .unwrap();
            let gl = c
                .expected
                .sign_conditions
                .iter()
                .find(|s| s["gradient"].is_array())
                .unwrap()["gradient"]
                .as_array()
                .unwrap();
            for v in gl {
                assert!(v.as_f64().unwrap().abs() < 1e-8, "{}: G l = 0", c.id);
            }
            tr
        } else {
            assert_eq!(c.expected.at_bound.len(), 1, "{}", c.id);
            c.bound_gradient(&c.expected.at_bound[0]).unwrap()
        };
        assert!(stored > 0.0, "{}: the rate must be positive", c.id);
        eprintln!(
            "{}: stored rate {stored:e}, slope of the naive objective {slope:e}",
            c.id
        );
        assert!(
            (slope - stored).abs() <= 0.02 * stored,
            "{}: slope {slope:e} against stored {stored:e}",
            c.id
        );
        n += 1;
    }
    assert!(n >= 3, "q = 1, diagonal and block boundaries: {n}");
}

/// The q = 1 boundary is what the card says: `lambda <= sigma^2`, the unconstrained maximum has a negative
/// variance; the interior one has a positive variance. Decided from the closed form written again above.
#[test]
fn the_q1_boundary_is_the_case_where_the_unconstrained_variance_is_negative() {
    let b = closed_form(&load_pop_fit_case("fit_q1_boundary").unwrap());
    assert!(b.boundary);
    let i = closed_form(&load_pop_fit_case("fit_q1_h38").unwrap());
    assert!(!i.boundary && i.point.omega[0][0] > 0.0);
}

// ---------------------------------------------------------------- the references

/// The three references (the exact point, `nlme`, the direct maximisation) agree. The limits are the measured worst
/// disagreements (printed) times a margin of about ten; they are not tolerances of the engine.
#[test]
fn the_three_references_agree_within_the_limits_chosen_from_the_measurements() {
    const THETA: f64 = 1e-6;
    const OMEGA: f64 = 1e-4;
    const SIGMA: f64 = 1e-6;
    const OFV: f64 = AbsoluteTolerance::POP_OFV.abs;
    let mut worst = [0.0f64; 4];
    for c in cases() {
        let w = &c.references["worst_disagreement"]["worst_of_the_three"];
        let vals: Vec<f64> = ["theta", "omega", "sigma", "ofv"]
            .iter()
            .map(|k| w[*k].as_f64().unwrap_or_else(|| panic!("{} {k}", c.id)))
            .collect();
        eprintln!(
            "{}: worst disagreement of the references: theta {:e} omega {:e} sigma {:e} ofv {:e}",
            c.id, vals[0], vals[1], vals[2], vals[3]
        );
        for (k, v) in vals.iter().enumerate() {
            worst[k] = worst[k].max(*v);
        }
        for (v, lim, what) in [
            (vals[0], THETA, "theta"),
            (vals[1], OMEGA, "omega"),
            (vals[2], SIGMA, "sigma"),
            (vals[3], OFV, "ofv"),
        ] {
            assert!(v <= lim, "{} {what}: {v:e} exceeds {lim:e}", c.id);
        }
        assert!(
            c.exact_reference["stationarity_gradient_max"]
                .as_f64()
                .unwrap()
                < 1e-40,
            "{}: the exact point is stationary to 256-bit rounding",
            c.id
        );
    }
    eprintln!(
        "worst over the cases: theta {:e} omega {:e} sigma {:e} ofv {:e}",
        worst[0], worst[1], worst[2], worst[3]
    );
}

// ---------------------------------------------------------------- errors.json

/// No control of an expected-error row is itself the refused request of another row (a control must be accepted,
/// and two rows cannot ask for opposite outcomes of the same request).
#[test]
fn no_control_is_the_refused_request_of_another_row() {
    let rows = caladrius_testkit::load_pop_errors().unwrap();
    let refused: Vec<(&str, String)> = rows
        .iter()
        .filter(|r| r.expect == "refused")
        .filter_map(|r| r.request.as_ref().map(|q| (r.case.as_str(), q.to_string())))
        .collect();
    let mut with_control = 0;
    for r in &rows {
        if let Some(control) = &r.control {
            with_control += 1;
            let text = control.request.to_string();
            for (name, req) in &refused {
                assert!(
                    *req != text,
                    "the control of `{}` is the refused request of `{name}`",
                    r.case
                );
            }
        }
    }
    assert!(with_control > 20, "{with_control}");
}
