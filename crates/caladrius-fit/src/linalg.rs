//! Small dense linear algebra for the fit (P parameters, N observations, P small): a matrix with
//! bounds-checked access, Householder QR least squares on a column-scaled Jacobian (FIT-ALG-02:
//! the normal equations are never formed for the step), triangular inverse, and the cyclic Jacobi
//! method for symmetric eigenvalues (FIT-OUT-05).

/// Dense row-major matrix. Access outside the matrix reads 0 and ignores writes (never panics).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mat {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl Mat {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows.saturating_mul(cols)],
        }
    }

    /// From columns of equal length.
    pub fn from_columns(columns: &[Vec<f64>]) -> Self {
        let rows = columns.first().map_or(0, Vec::len);
        let mut m = Self::zeros(rows, columns.len());
        for (j, col) in columns.iter().enumerate() {
            for (i, &x) in col.iter().enumerate() {
                m.set(i, j, x);
            }
        }
        m
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn at(&self, i: usize, j: usize) -> f64 {
        if i < self.rows && j < self.cols {
            self.data.get(i * self.cols + j).copied().unwrap_or(0.0)
        } else {
            0.0
        }
    }

    pub fn set(&mut self, i: usize, j: usize, x: f64) {
        if i < self.rows && j < self.cols {
            if let Some(slot) = self.data.get_mut(i * self.cols + j) {
                *slot = x;
            }
        }
    }

    /// Euclidean norm of column j.
    pub fn column_norm(&self, j: usize) -> f64 {
        (0..self.rows)
            .map(|i| self.at(i, j).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

/// Least-squares solution of A·x ≈ b (A is m × n with m >= n) by Householder QR. Returns `None`
/// when A is numerically rank deficient (a pivot below `1e-13` times the largest one; the caller
/// scales the columns of A to unit length first, so this is a relative test). Also returns R.
pub(crate) fn least_squares(a: &Mat, b: &[f64]) -> Option<(Vec<f64>, Mat)> {
    let (m, n) = (a.rows(), a.cols());
    if m < n || b.len() != m {
        return None;
    }
    let mut r = a.clone();
    let mut y: Vec<f64> = b.to_vec();
    for k in 0..n {
        let norm = (k..m).map(|i| r.at(i, k).powi(2)).sum::<f64>().sqrt();
        if norm == 0.0 {
            continue;
        }
        let alpha = if r.at(k, k) > 0.0 { -norm } else { norm };
        // v = x − alpha·e_k.
        let mut v: Vec<f64> = (k..m).map(|i| r.at(i, k)).collect();
        if let Some(first) = v.first_mut() {
            *first -= alpha;
        }
        let vnorm2: f64 = v.iter().map(|x| x * x).sum();
        if vnorm2 == 0.0 {
            continue;
        }
        for j in k..n {
            let dot: f64 = v
                .iter()
                .enumerate()
                .map(|(o, vi)| vi * r.at(k + o, j))
                .sum();
            let f = 2.0 * dot / vnorm2;
            for (o, vi) in v.iter().enumerate() {
                r.set(k + o, j, r.at(k + o, j) - f * vi);
            }
        }
        let dot: f64 = v
            .iter()
            .enumerate()
            .map(|(o, vi)| vi * y.get(k + o).copied().unwrap_or(0.0))
            .sum();
        let f = 2.0 * dot / vnorm2;
        for (o, vi) in v.iter().enumerate() {
            if let Some(slot) = y.get_mut(k + o) {
                *slot -= f * vi;
            }
        }
    }
    let largest = (0..n).map(|k| r.at(k, k).abs()).fold(0.0, f64::max);
    let tiny = |k: usize| r.at(k, k).abs() <= 1e-13 * largest || r.at(k, k).is_nan();
    if largest.is_nan() || largest <= 0.0 || (0..n).any(tiny) {
        return None;
    }
    // Back substitution on the leading n × n block.
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let s: f64 = (k + 1..n)
            .map(|j| r.at(k, j) * x.get(j).copied().unwrap_or(0.0))
            .sum();
        let value = (y.get(k).copied().unwrap_or(0.0) - s) / r.at(k, k);
        if let Some(slot) = x.get_mut(k) {
            *slot = value;
        }
    }
    let mut upper = Mat::zeros(n, n);
    for i in 0..n {
        for j in i..n {
            upper.set(i, j, r.at(i, j));
        }
    }
    Some((x, upper))
}

/// (RᵀR)⁻¹ = R⁻¹·R⁻ᵀ for an upper-triangular, non-singular R.
pub(crate) fn inverse_gram_from_r(r: &Mat) -> Option<Mat> {
    let n = r.cols();
    let mut inv = Mat::zeros(n, n);
    for j in 0..n {
        // Solve R·x = e_j (x is column j of R⁻¹, zero below the diagonal).
        for i in (0..=j).rev() {
            let rhs = if i == j { 1.0 } else { 0.0 };
            let s: f64 = (i + 1..=j).map(|k| r.at(i, k) * inv.at(k, j)).sum();
            let d = r.at(i, i);
            if d == 0.0 {
                return None;
            }
            inv.set(i, j, (rhs - s) / d);
        }
    }
    let mut out = Mat::zeros(n, n);
    for a in 0..n {
        for b in 0..n {
            let s = (0..n).map(|k| inv.at(a, k) * inv.at(b, k)).sum();
            out.set(a, b, s);
        }
    }
    Some(out)
}

/// Eigenvalues of a symmetric matrix by cyclic Jacobi rotations, in decreasing order.
pub(crate) fn symmetric_eigenvalues(a: &Mat) -> Vec<f64> {
    let n = a.rows();
    let mut m = a.clone();
    for _sweep in 0..100 {
        let mut off = 0.0;
        let mut diag = 0.0;
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    diag += m.at(i, j).powi(2);
                } else {
                    off += m.at(i, j).powi(2);
                }
            }
        }
        if off <= 1e-30 * diag.max(f64::MIN_POSITIVE) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = m.at(p, q);
                if apq == 0.0 {
                    continue;
                }
                let theta = (m.at(q, q) - m.at(p, p)) / (2.0 * apq);
                let t = if theta == 0.0 {
                    1.0
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (mkp, mkq) = (m.at(k, p), m.at(k, q));
                    m.set(k, p, c * mkp - s * mkq);
                    m.set(k, q, s * mkp + c * mkq);
                }
                for k in 0..n {
                    let (mpk, mqk) = (m.at(p, k), m.at(q, k));
                    m.set(p, k, c * mpk - s * mqk);
                    m.set(q, k, s * mpk + c * mqk);
                }
            }
        }
    }
    let mut values: Vec<f64> = (0..n).map(|i| m.at(i, i)).collect();
    values.sort_by(|a, b| b.total_cmp(a));
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn least_squares_of_a_line_and_its_covariance() {
        // y = 1 + 2x on x = 0, 1, 2, 3 exactly.
        let a = Mat::from_columns(&[vec![1.0; 4], vec![0.0, 1.0, 2.0, 3.0]]);
        let (x, r) = least_squares(&a, &[1.0, 3.0, 5.0, 7.0]).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-14 && (x[1] - 2.0).abs() < 1e-14);
        // (AᵀA)⁻¹ = [[14, -6], [-6, 4]] / 20.
        let inv = inverse_gram_from_r(&r).unwrap();
        assert!((inv.at(0, 0) - 0.7).abs() < 1e-14);
        assert!((inv.at(0, 1) + 0.3).abs() < 1e-14);
        assert!((inv.at(1, 1) - 0.2).abs() < 1e-14);
    }

    #[test]
    fn rank_deficient_systems_are_refused() {
        let a = Mat::from_columns(&[vec![1.0, 2.0, 3.0], vec![2.0, 4.0, 6.0]]);
        assert!(least_squares(&a, &[1.0, 1.0, 1.0]).is_none());
    }

    #[test]
    fn eigenvalues_of_a_symmetric_matrix() {
        let mut a = Mat::zeros(3, 3);
        for (i, j, x) in [
            (0, 0, 2.0),
            (1, 1, 3.0),
            (2, 2, 4.0),
            (0, 1, 1.0),
            (1, 0, 1.0),
        ] {
            a.set(i, j, x);
        }
        // Eigenvalues: 4 and (5 ± √5)/2.
        let e = symmetric_eigenvalues(&a);
        let s5 = 5f64.sqrt();
        for (x, y) in e.iter().zip([4.0, (5.0 + s5) / 2.0, (5.0 - s5) / 2.0]) {
            assert!((x - y).abs() < 1e-12, "{e:?}");
        }
    }
}
