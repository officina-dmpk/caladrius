//! The Cholesky factor and its triangular solves, for the small symmetric matrices of the population
//! objective: Ω (q × q), C_i (n_i × n_i) and the observed Hessian of the polish (q × q). Nothing is
//! inverted except the lower factor of Ω, whose inverse L⁻¹ is the block of prior rows (POP-INN-02).

use crate::linalg::Mat;

/// The lower factor L of a symmetric matrix A = L·Lᵀ, or `None` when A is not positive definite
/// (a pivot that is not finite and strictly positive). Only the lower triangle of A is read.
pub(crate) fn cholesky(a: &Mat) -> Option<Mat> {
    let n = a.rows();
    if a.cols() != n {
        return None;
    }
    let mut l = Mat::zeros(n, n);
    for j in 0..n {
        let mut d = a.at(j, j);
        for k in 0..j {
            d -= l.at(j, k) * l.at(j, k);
        }
        if !(d.is_finite() && d > 0.0) {
            return None;
        }
        let ljj = d.sqrt();
        l.set(j, j, ljj);
        for i in (j + 1)..n {
            let mut s = a.at(i, j);
            for k in 0..j {
                s -= l.at(i, k) * l.at(j, k);
            }
            l.set(i, j, s / ljj);
        }
    }
    Some(l)
}

/// x with L·x = b (L lower triangular).
pub(crate) fn forward(l: &Mat, b: &[f64]) -> Vec<f64> {
    let n = l.rows();
    let mut x = vec![0.0; n];
    for i in 0..n {
        let mut s = b.get(i).copied().unwrap_or(f64::NAN);
        for (k, xk) in x.iter().enumerate().take(i) {
            s -= l.at(i, k) * xk;
        }
        if let Some(slot) = x.get_mut(i) {
            *slot = s / l.at(i, i);
        }
    }
    x
}

/// x with Lᵀ·x = b (L lower triangular).
pub(crate) fn backward_transposed(l: &Mat, b: &[f64]) -> Vec<f64> {
    let n = l.rows();
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b.get(i).copied().unwrap_or(f64::NAN);
        for k in (i + 1)..n {
            s -= l.at(k, i) * x.get(k).copied().unwrap_or(f64::NAN);
        }
        if let Some(slot) = x.get_mut(i) {
            *slot = s / l.at(i, i);
        }
    }
    x
}

/// x with A·x = b, from the lower factor L of A.
pub(crate) fn solve(l: &Mat, b: &[f64]) -> Vec<f64> {
    backward_transposed(l, &forward(l, b))
}

/// L⁻¹ of a lower-triangular L (lower triangular too), column by column.
pub(crate) fn lower_inverse(l: &Mat) -> Mat {
    let n = l.rows();
    let mut inv = Mat::zeros(n, n);
    for j in 0..n {
        let mut e = vec![0.0; n];
        if let Some(slot) = e.get_mut(j) {
            *slot = 1.0;
        }
        for (i, x) in forward(l, &e).into_iter().enumerate() {
            inv.set(i, j, x);
        }
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_factor_reproduces_the_matrix_and_solves() {
        let mut a = Mat::zeros(3, 3);
        let rows = [[4.0, 2.0, 0.4], [2.0, 3.0, 0.5], [0.4, 0.5, 2.0]];
        for (i, row) in rows.iter().enumerate() {
            for (j, x) in row.iter().enumerate() {
                a.set(i, j, *x);
            }
        }
        let l = cholesky(&a).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let s: f64 = (0..3).map(|k| l.at(i, k) * l.at(j, k)).sum();
                assert!((s - a.at(i, j)).abs() < 1e-14);
            }
        }
        let x = solve(&l, &[1.0, 2.0, 3.0]);
        for i in 0..3 {
            let s: f64 = (0..3).map(|k| a.at(i, k) * x[k]).sum();
            assert!((s - [1.0, 2.0, 3.0][i]).abs() < 1e-13);
        }
        let inv = lower_inverse(&l);
        for i in 0..3 {
            for j in 0..3 {
                let s: f64 = (0..3).map(|k| inv.at(i, k) * l.at(k, j)).sum();
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((s - want).abs() < 1e-14);
            }
        }
    }

    #[test]
    fn a_matrix_that_is_not_positive_definite_has_no_factor() {
        let mut a = Mat::zeros(2, 2);
        a.set(0, 0, 0.04);
        a.set(1, 0, 0.1);
        a.set(0, 1, 0.1);
        a.set(1, 1, 0.04);
        assert!(cholesky(&a).is_none());
    }
}
